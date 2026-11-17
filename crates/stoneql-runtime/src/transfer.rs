//! M007 streaming export (D022): one table, a chosen set of tables, or every base table of the
//! connection's database, as SQL, CSV or JSON. Rows stream on one dedicated connection inside an
//! engine-enforced read-only snapshot transaction; nothing is held beyond one INSERT batch.
//! Values are written exactly: no type is reinterpreted, NULL and the empty string stay
//! distinct, and a value that cannot be written exactly in the chosen format fails the export
//! and names its column. The connection is always discarded afterwards.
use super::{
    ConnectionId, Connections, EnginePool, OwnerId, ReadError, mysql, postgres,
    read_only::driver_error,
};
use futures_util::TryStreamExt;
use sqlx::{Column, Connection, Executor, Row, SqlSafeStr, Statement, database::HasStatementCache};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use stoneql_contracts::v1::CellValue;
use tokio::io::{AsyncWrite, AsyncWriteExt};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Csv,
    Sql,
    Json,
}
#[derive(Debug, Clone)]
pub struct ExportRequest {
    /// Tables in the order given, or `None` for every base table of the connection's database
    /// (name order). CSV takes exactly one table.
    pub tables: Option<Vec<String>>,
    pub format: ExportFormat,
    /// SQL only: include `CREATE TABLE` (MySQL/MariaDB `SHOW CREATE TABLE`).
    pub structure: bool,
    pub data: bool,
    pub options: ExportOptions,
}
/// Format options (phpMyAdmin-style custom export). Defaults match the quick export.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportOptions {
    /// SQL with structure (MySQL/MariaDB): `DROP TABLE [IF EXISTS]` before each `CREATE TABLE`.
    pub drop_table: bool,
    pub if_exists: bool,
    /// SQL: rows per INSERT, 1 (one row per statement) to `SQL_BATCH_ROWS` (extended INSERT).
    pub rows_per_insert: usize,
    /// SQL (MySQL/MariaDB): `INSERT IGNORE` or `REPLACE` instead of `INSERT`.
    pub insert_verb: InsertVerb,
    /// SQL with structure (MySQL/MariaDB): keep the table's `AUTO_INCREMENT=n` counter.
    pub auto_increment: bool,
    /// SQL data: wrap the INSERTs in one transaction.
    pub transaction: bool,
    /// SQL (MySQL/MariaDB): `SET FOREIGN_KEY_CHECKS = 0` while the file is imported.
    pub disable_foreign_keys: bool,
    /// CSV: one-character separator; enclosure `None` writes values bare (refusing any value
    /// that would then be ambiguous); `null` is written for NULL; optional header row.
    pub csv_delimiter: char,
    pub csv_enclosure: Option<char>,
    pub csv_null: String,
    pub csv_header: bool,
}
impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            drop_table: false,
            if_exists: true,
            rows_per_insert: SQL_BATCH_ROWS,
            insert_verb: InsertVerb::Insert,
            auto_increment: true,
            transaction: false,
            disable_foreign_keys: false,
            csv_delimiter: ',',
            csv_enclosure: Some('"'),
            csv_null: "NULL".into(),
            csv_header: true,
        }
    }
}
impl ExportOptions {
    fn valid(&self) -> bool {
        let special = |c: char| matches!(c, '\n' | '\r') || c.is_control();
        (1..=SQL_BATCH_ROWS).contains(&self.rows_per_insert)
            && !special(self.csv_delimiter)
            && self
                .csv_enclosure
                .is_none_or(|e| !special(e) && e != self.csv_delimiter)
            && self.csv_null.len() <= 32
            && !self.csv_null.chars().any(special)
            && !self.csv_null.contains(self.csv_delimiter)
            && self
                .csv_enclosure
                .is_none_or(|e| !self.csv_null.contains(e))
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertVerb {
    Insert,
    InsertIgnore,
    Replace,
}
/// Shared with the host for status polling and cancellation; checked between rows.
#[derive(Debug, Default)]
pub struct ExportProgress {
    pub rows: AtomicU64,
    pub bytes: AtomicU64,
    pub cancel: AtomicBool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportSummary {
    pub rows: u64,
    pub bytes: u64,
    pub columns: usize,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferError {
    Forbidden,
    InvalidRequest,
    /// The engine or format cannot represent this export exactly (message names why).
    Unsupported(String),
    Cancelled,
    /// The destination could not be written (for example, the disk is full).
    Io,
    Read(ReadError),
}
impl From<ReadError> for TransferError {
    fn from(error: ReadError) -> Self {
        Self::Read(error)
    }
}

/// Most rows per multi-row INSERT in SQL exports (Proposed, alpha plan import batch size).
pub const SQL_BATCH_ROWS: usize = 500;
/// Most tables named in one export request (Proposed).
pub const MAX_EXPORT_TABLES: usize = 4096;

/// Exact string literal for generated SQL, independent of server escaping settings; the same
/// encoding as the renderer's `liveLiteral` (shared vectors in `tests/fixtures/sql-literals.json`).
pub fn string_literal(mysql_family: bool, value: &str) -> String {
    if mysql_family {
        let hex: String = value.bytes().map(|byte| format!("{byte:02x}")).collect();
        format!("_utf8mb4 X'{hex}'")
    } else {
        format!("E'{}'", value.replace('\\', "\\\\").replace('\'', "''"))
    }
}
pub fn quote_identifier(mysql_family: bool, name: &str) -> String {
    if mysql_family {
        format!("`{}`", name.replace('`', "``"))
    } else {
        format!("\"{}\"", name.replace('"', "\"\""))
    }
}
fn unsupported_type(column: &str, database_type: &str) -> TransferError {
    TransferError::Unsupported(format!(
        "column {column} has type {database_type}, which StoneQL cannot export exactly"
    ))
}
/// One SQL value. Numbers are written as exact text literals too, so the server converts them
/// with its own rules (no float or precision step in StoneQL).
fn sql_value(mysql_family: bool, column: &str, cell: &CellValue) -> Result<String, TransferError> {
    Ok(match cell {
        CellValue::Null => "NULL".into(),
        CellValue::Boolean { value } => if *value { "TRUE" } else { "FALSE" }.into(),
        CellValue::Text { text }
        | CellValue::Integer { text }
        | CellValue::Decimal { text }
        | CellValue::Float { text }
        | CellValue::Temporal { text, .. }
        | CellValue::Json { text } => string_literal(mysql_family, text),
        CellValue::Binary { hex } => {
            if mysql_family {
                format!("X'{hex}'")
            } else {
                format!("E'\\\\x{hex}'")
            }
        }
        CellValue::Unknown { database_type, .. } => {
            return Err(unsupported_type(column, database_type));
        }
    })
}
/// Text of a non-NULL value for CSV/JSON; binary has no exact text form there and is refused.
fn plain_text<'a>(
    column: &str,
    cell: &'a CellValue,
    format: &str,
) -> Result<Option<std::borrow::Cow<'a, str>>, TransferError> {
    Ok(match cell {
        CellValue::Null => None,
        CellValue::Boolean { value } => Some(if *value { "true" } else { "false" }.into()),
        CellValue::Text { text }
        | CellValue::Integer { text }
        | CellValue::Decimal { text }
        | CellValue::Float { text }
        | CellValue::Temporal { text, .. }
        | CellValue::Json { text } => Some(text.as_str().into()),
        CellValue::Binary { .. } => {
            return Err(TransferError::Unsupported(format!(
                "column {column} holds binary data, which {format} cannot represent exactly; export as SQL"
            )));
        }
        CellValue::Unknown { database_type, .. } => {
            return Err(unsupported_type(column, database_type));
        }
    })
}
/// CSV field. With an enclosure (default `"`, doubled inside), every non-NULL value is enclosed
/// and NULL is the bare NULL text, so `""` is the empty string and `"NULL"` the four-letter
/// string (MySQL `LOAD DATA … ENCLOSED BY '"' ESCAPED BY ''`, PostgreSQL `COPY … (FORMAT csv,
/// NULL 'NULL')`). Without an enclosure a value containing the separator or a line break, or
/// equal to the NULL text, would be ambiguous and is refused, naming its column.
fn csv_field(
    column: &str,
    cell: &CellValue,
    options: &ExportOptions,
) -> Result<String, TransferError> {
    let Some(text) = plain_text(column, cell, "CSV")? else {
        return Ok(options.csv_null.clone());
    };
    match options.csv_enclosure {
        Some(e) => Ok(format!("{e}{}{e}", text.replace(e, &format!("{e}{e}")))),
        None if text.contains(options.csv_delimiter)
            || text.contains(['\n', '\r'])
            || *text == options.csv_null =>
        {
            Err(TransferError::Unsupported(format!(
                "column {column} has a value that cannot be written without an enclosure; choose an enclosure"
            )))
        }
        None => Ok(text.into_owned()),
    }
}
/// JSON value: NULL is `null`, booleans are booleans, every other value is its exact text as a
/// JSON string (numbers too, so BIGINT and DECIMAL keep every digit).
fn json_value(column: &str, cell: &CellValue) -> Result<String, TransferError> {
    Ok(match (cell, plain_text(column, cell, "JSON")?) {
        (CellValue::Boolean { value }, _) => value.to_string(),
        (_, None) => "null".into(),
        (_, Some(text)) => serde_json::Value::String(text.into_owned()).to_string(),
    })
}

struct Sink<'a, W> {
    out: &'a mut W,
    progress: &'a ExportProgress,
}
impl<W: AsyncWrite + Unpin + Send> Sink<'_, W> {
    async fn write(&mut self, text: &str) -> Result<(), TransferError> {
        if text.is_empty() {
            return Ok(());
        }
        self.out
            .write_all(text.as_bytes())
            .await
            .map_err(|_| TransferError::Io)?;
        self.progress
            .bytes
            .fetch_add(text.len() as u64, Ordering::Relaxed);
        Ok(())
    }
    fn cancelled(&self) -> Result<(), TransferError> {
        if self.progress.cancel.load(Ordering::Relaxed) {
            Err(TransferError::Cancelled)
        } else {
            Ok(())
        }
    }
}

/// Formats one table's rows as they arrive: a CSV line each, a JSON object each, or SQL INSERTs
/// batched by `rows_per_insert`.
struct Writer<'o> {
    mysql_family: bool,
    format: ExportFormat,
    options: &'o ExportOptions,
    table: String,
    columns: Vec<String>,
    batch: Vec<String>,
    rows: u64,
}
impl Writer<'_> {
    fn header(&self) -> String {
        match self.format {
            ExportFormat::Csv if self.options.csv_header => {
                let names: Vec<String> = self
                    .columns
                    .iter()
                    .map(|name| match self.options.csv_enclosure {
                        Some(e) => format!("{e}{}{e}", name.replace(e, &format!("{e}{e}"))),
                        None => name.clone(),
                    })
                    .collect();
                format!("{}\n", names.join(&self.options.csv_delimiter.to_string()))
            }
            _ => String::new(),
        }
    }
    fn row(&mut self, cells: &[CellValue]) -> Result<Option<String>, TransferError> {
        self.rows += 1;
        match self.format {
            ExportFormat::Csv => {
                let fields = cells
                    .iter()
                    .zip(&self.columns)
                    .map(|(cell, column)| csv_field(column, cell, self.options))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Some(format!(
                    "{}\n",
                    fields.join(&self.options.csv_delimiter.to_string())
                )))
            }
            ExportFormat::Json => {
                let fields = cells
                    .iter()
                    .zip(&self.columns)
                    .map(|(cell, column)| {
                        Ok(format!(
                            "{}: {}",
                            serde_json::Value::String(column.clone()),
                            json_value(column, cell)?
                        ))
                    })
                    .collect::<Result<Vec<_>, TransferError>>()?;
                let separator = if self.rows == 1 { "" } else { ",\n" };
                Ok(Some(format!("{separator}  {{{}}}", fields.join(", "))))
            }
            ExportFormat::Sql => {
                let values = cells
                    .iter()
                    .zip(&self.columns)
                    .map(|(cell, column)| sql_value(self.mysql_family, column, cell))
                    .collect::<Result<Vec<_>, _>>()?;
                self.batch.push(format!("({})", values.join(", ")));
                Ok((self.batch.len() >= self.options.rows_per_insert).then(|| self.flush()))
            }
        }
    }
    fn flush(&mut self) -> String {
        if self.batch.is_empty() {
            return String::new();
        }
        let columns: Vec<String> = self
            .columns
            .iter()
            .map(|name| quote_identifier(self.mysql_family, name))
            .collect();
        let rows = std::mem::take(&mut self.batch).join(",\n");
        let verb = match (self.mysql_family, self.options.insert_verb) {
            (true, InsertVerb::InsertIgnore) => "INSERT IGNORE",
            (true, InsertVerb::Replace) => "REPLACE",
            _ => "INSERT",
        };
        format!(
            "{verb} INTO {} ({}) VALUES\n{rows};\n",
            quote_identifier(self.mysql_family, &self.table),
            columns.join(", ")
        )
    }
}

/// Strict UTF-8 text of a character column the driver delivered as bytes.
fn text_from_hex(column: &str, hex: &str) -> Result<String, TransferError> {
    let bytes = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16))
        .collect::<Result<Vec<u8>, _>>()
        .map_err(|_| TransferError::Read(ReadError::Database))?;
    String::from_utf8(bytes).map_err(|_| {
        TransferError::Unsupported(format!(
            "column {column} holds text that is not valid UTF-8"
        ))
    })
}
fn valid_table(table: &str) -> bool {
    !table.is_empty() && table.len() <= 128 && !table.chars().any(char::is_control)
}

impl Connections {
    /// Streams the requested tables (all base tables when `tables` is `None`) to `out` inside
    /// one read-only snapshot. The caller owns the destination (temporary file, atomic rename,
    /// removal on any error). Read-write handles are refused while they run other work, like
    /// reads (one physical session of work per handle).
    pub async fn export_table<W: AsyncWrite + Unpin + Send>(
        &self,
        owner: OwnerId,
        id: ConnectionId,
        request: &ExportRequest,
        out: &mut W,
        progress: &ExportProgress,
    ) -> Result<ExportSummary, TransferError> {
        let named = request.tables.as_deref();
        if named.is_some_and(|tables| {
            tables.is_empty()
                || tables.len() > MAX_EXPORT_TABLES
                || tables.iter().any(|table| !valid_table(table))
        }) || (request.format == ExportFormat::Csv && named.is_none_or(|t| t.len() != 1))
            || (!request.data && !request.structure)
            || (request.format != ExportFormat::Sql && (request.structure || !request.data))
            || !request.options.valid()
        {
            return Err(TransferError::InvalidRequest);
        }
        let pool = self.owned_pool(owner, id).ok_or(TransferError::Forbidden)?;
        let _exclusive = self.exclusive_read(owner, id)?;
        let mut sink = Sink { out, progress };
        let columns = match pool {
            EnginePool::MySql(pool) => {
                let mut connection = pool.acquire().await.map_err(driver_error)?;
                connection.close_on_drop();
                // A consistent snapshot for InnoDB tables, enforced read-only by the server.
                sqlx::raw_sql("START TRANSACTION WITH CONSISTENT SNAPSHOT, READ ONLY")
                    .execute(&mut *connection)
                    .await
                    .map_err(driver_error)?;
                let outcome: Result<usize, TransferError> = async {
                    let tables = match named {
                        Some(tables) => tables.to_vec(),
                        None => sqlx::query_scalar(
                            "SELECT TABLE_NAME FROM information_schema.TABLES WHERE TABLE_SCHEMA = DATABASE() AND TABLE_TYPE = 'BASE TABLE' ORDER BY TABLE_NAME",
                        )
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(driver_error)?,
                    };
                    let mut file = File::new(true, request, &tables);
                    sink.write(&file.begin()).await?;
                    let mut columns = 0;
                    for table in &tables {
                        sink.cancelled()?;
                        sink.write(&file.table(table)).await?;
                        columns += mysql_table(&mut connection, table, request, &mut sink)
                            .await
                            .map_err(|error| in_table(error, file.many, true, table))?;
                    }
                    sink.write(&file.end()).await?;
                    Ok(columns)
                }
                .await;
                end_snapshot(connection, outcome.is_err()).await;
                outcome?
            }
            EnginePool::Postgres(pool) => {
                if request.structure {
                    return Err(TransferError::Unsupported(
                        "PostgreSQL structure export needs pg_dump; export data only".into(),
                    ));
                }
                let mut connection = pool.acquire().await.map_err(driver_error)?;
                connection.close_on_drop();
                sqlx::raw_sql("START TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
                    .execute(&mut *connection)
                    .await
                    .map_err(driver_error)?;
                let outcome: Result<usize, TransferError> = async {
                    let tables = match named {
                        Some(tables) => tables.to_vec(),
                        None => sqlx::query_scalar(
                            "SELECT table_name::text FROM information_schema.tables WHERE table_schema = current_schema() AND table_type = 'BASE TABLE' ORDER BY table_name",
                        )
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(driver_error)?,
                    };
                    let mut file = File::new(false, request, &tables);
                    sink.write(&file.begin()).await?;
                    let mut columns = 0;
                    for table in &tables {
                        sink.cancelled()?;
                        sink.write(&file.table(table)).await?;
                        columns += stream::<sqlx::Postgres, _>(
                            &mut connection,
                            &format!("SELECT * FROM {}", quote_identifier(false, table)),
                            postgres::decode_cell,
                            false,
                            &[],
                            table,
                            request,
                            &mut sink,
                        )
                        .await
                        .map_err(|error| in_table(error, file.many, false, table))?;
                    }
                    sink.write(&file.end()).await?;
                    Ok(columns)
                }
                .await;
                end_snapshot(connection, outcome.is_err()).await;
                outcome?
            }
            #[cfg(feature = "sqlite")]
            EnginePool::Sqlite(_) => {
                return Err(TransferError::Unsupported(
                    "SQLite export is not part of the supported scope".into(),
                ));
            }
        };
        sink.out.flush().await.map_err(|_| TransferError::Io)?;
        Ok(ExportSummary {
            rows: progress.rows.load(Ordering::Relaxed),
            bytes: progress.bytes.load(Ordering::Relaxed),
            columns,
        })
    }
}

/// The text around the tables of one export file: SQL session settings and optional
/// transaction/foreign-key wrapping, per-table comments, or the JSON array/object brackets.
struct File<'r> {
    mysql_family: bool,
    request: &'r ExportRequest,
    /// More than one table (or the whole database): tables are labelled.
    many: bool,
    written: usize,
}
impl<'r> File<'r> {
    fn new(mysql_family: bool, request: &'r ExportRequest, tables: &[String]) -> Self {
        Self {
            mysql_family,
            request,
            many: request.tables.is_none() || tables.len() > 1,
            written: 0,
        }
    }
    fn begin(&self) -> String {
        let options = &self.request.options;
        match self.request.format {
            ExportFormat::Sql => {
                let subject = match self.request.tables.as_deref() {
                    Some([table]) => quote_identifier(self.mysql_family, table),
                    Some(tables) => format!("{} tables", tables.len()),
                    None if self.mysql_family => "every table of the current database".into(),
                    None => "every table of the current schema".into(),
                };
                let mut text = if self.mysql_family {
                    format!(
                        "-- StoneQL export of {subject} (data in a read-only consistent snapshot).\nSET NAMES utf8mb4;\nSET SESSION sql_mode = 'NO_AUTO_VALUE_ON_ZERO';\n"
                    )
                } else {
                    format!(
                        "-- StoneQL export of {subject} (data in a read-only repeatable-read snapshot).\n"
                    )
                };
                if self.mysql_family && options.disable_foreign_keys {
                    text.push_str("SET FOREIGN_KEY_CHECKS = 0;\n");
                }
                if options.transaction && self.request.data {
                    text.push_str(if self.mysql_family {
                        "START TRANSACTION;\n"
                    } else {
                        "BEGIN;\n"
                    });
                }
                text
            }
            ExportFormat::Json if self.many => "{\n".into(),
            ExportFormat::Json | ExportFormat::Csv => String::new(),
        }
    }
    fn table(&mut self, table: &str) -> String {
        self.written += 1;
        match self.request.format {
            ExportFormat::Sql => {
                let quoted = quote_identifier(self.mysql_family, table);
                let mut text = if self.many {
                    format!("\n-- Table {quoted}\n")
                } else {
                    String::new()
                };
                if self.request.structure && self.request.options.drop_table && self.mysql_family {
                    let exists = if self.request.options.if_exists {
                        "IF EXISTS "
                    } else {
                        ""
                    };
                    text.push_str(&format!("DROP TABLE {exists}{quoted};\n"));
                }
                text
            }
            ExportFormat::Json => {
                let close = if self.written > 1 { "\n],\n" } else { "" };
                if self.many {
                    format!("{close}{}: [\n", serde_json::Value::String(table.into()))
                } else {
                    "[\n".into()
                }
            }
            ExportFormat::Csv => String::new(),
        }
    }
    fn end(&self) -> String {
        let options = &self.request.options;
        match self.request.format {
            ExportFormat::Sql => {
                let mut text = String::new();
                if options.transaction && self.request.data {
                    text.push_str("COMMIT;\n");
                }
                if self.mysql_family && options.disable_foreign_keys {
                    text.push_str("SET FOREIGN_KEY_CHECKS = 1;\n");
                }
                text
            }
            ExportFormat::Json if self.many => {
                if self.written > 0 {
                    "\n]\n}\n".into()
                } else {
                    "}\n".into()
                }
            }
            ExportFormat::Json => "\n]\n".into(),
            ExportFormat::Csv => String::new(),
        }
    }
}

/// Ends the read-only snapshot. After a failure the connection is closed before the error is
/// returned, so no snapshot transaction outlives the export on the server.
async fn end_snapshot<DB: sqlx::Database>(
    mut connection: sqlx::pool::PoolConnection<DB>,
    failed: bool,
) where
    for<'c> &'c mut DB::Connection: Executor<'c, Database = DB>,
{
    let _ = sqlx::raw_sql("ROLLBACK").execute(&mut *connection).await;
    if failed {
        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), connection.close()).await;
    }
}
/// With several tables, a row over the packet limit names its table (the export stops).
fn in_table(error: TransferError, many: bool, mysql_family: bool, table: &str) -> TransferError {
    match error {
        TransferError::Read(ReadError::PacketTooLarge) if many => {
            TransferError::Unsupported(format!(
                "table {} has a row larger than the 2 MiB limit, so it cannot be exported",
                quote_identifier(mysql_family, table)
            ))
        }
        other => other,
    }
}
/// Drops the ` AUTO_INCREMENT=n` table option, which the server prints only after the closing
/// parenthesis of the column list (a column's `AUTO_INCREMENT` attribute has no `=`).
fn without_auto_increment(ddl: &str) -> String {
    let Some(close) = ddl.rfind(')') else {
        return ddl.to_owned();
    };
    let (columns, options) = ddl.split_at(close);
    let options = options
        .split(' ')
        .filter(|word| {
            !word
                .strip_prefix("AUTO_INCREMENT=")
                .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        })
        .collect::<Vec<_>>()
        .join(" ");
    format!("{columns}{options}")
}
/// One MySQL/MariaDB table of an export: `CREATE TABLE` (when requested) and its rows.
async fn mysql_table<W: AsyncWrite + Unpin + Send>(
    connection: &mut sqlx::MySqlConnection,
    table: &str,
    request: &ExportRequest,
    sink: &mut Sink<'_, W>,
) -> Result<usize, TransferError> {
    let quoted = quote_identifier(true, table);
    if request.structure {
        let row = sqlx::raw_sql(
            sqlx::AssertSqlSafe(format!("SHOW CREATE TABLE {quoted}")).into_sql_str(),
        )
        .fetch_one(&mut *connection)
        .await
        .map_err(driver_error)?;
        let bytes: Vec<u8> = row
            .try_get_unchecked(1)
            .map_err(|_| TransferError::Read(ReadError::Database))?;
        let ddl = String::from_utf8(bytes).map_err(|_| TransferError::Read(ReadError::Database))?;
        let ddl = if request.options.auto_increment {
            ddl
        } else {
            without_auto_increment(&ddl)
        };
        sink.write(&format!("{ddl};\n")).await?;
    }
    if !request.data {
        return Ok(0);
    }
    // Servers set the BINARY column flag for `_bin` collations too (for example MariaDB JSON),
    // so the driver names such text columns binary. The catalog's character set is
    // authoritative: a column with one holds text.
    let text_columns: Vec<String> = sqlx::query_scalar(
        "SELECT COLUMN_NAME FROM information_schema.COLUMNS WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = ? AND CHARACTER_SET_NAME IS NOT NULL",
    )
    .bind(table)
    .fetch_all(&mut *connection)
    .await
    .map_err(driver_error)?;
    stream::<sqlx::MySql, _>(
        connection,
        &format!("SELECT * FROM {quoted}"),
        mysql::decode_cell,
        true,
        &text_columns,
        table,
        request,
        sink,
    )
    .await
}

/// Prepares the generated `SELECT *` (identifiers quoted, no request data in values), then
/// formats each row as it arrives. Returns the column count.
#[allow(clippy::too_many_arguments)]
async fn stream<DB, W>(
    connection: &mut DB::Connection,
    sql: &str,
    decode: fn(&DB::Row, usize) -> Result<CellValue, ReadError>,
    mysql_family: bool,
    text_columns: &[String],
    table: &str,
    request: &ExportRequest,
    sink: &mut Sink<'_, W>,
) -> Result<usize, TransferError>
where
    DB: sqlx::Database + HasStatementCache,
    for<'c> &'c mut DB::Connection: Executor<'c, Database = DB>,
    DB::Arguments: sqlx::IntoArguments<DB>,
    W: AsyncWrite + Unpin + Send,
{
    let statement = connection
        .prepare(sqlx::AssertSqlSafe(sql.to_owned()).into_sql_str())
        .await
        .map_err(driver_error)?;
    let columns: Vec<String> = statement
        .columns()
        .iter()
        .map(|column| column.name().to_owned())
        .collect();
    let mut writer = Writer {
        mysql_family,
        format: request.format,
        options: &request.options,
        table: table.to_owned(),
        columns: columns.clone(),
        batch: Vec::new(),
        rows: 0,
    };
    sink.write(&writer.header()).await?;
    {
        let mut rows = statement.query().persistent(false).fetch(&mut *connection);
        while let Some(row) = rows.try_next().await.map_err(driver_error)? {
            sink.cancelled()?;
            let mut cells = (0..row.len())
                .map(|index| decode(&row, index))
                .collect::<Result<Vec<_>, _>>()?;
            for (cell, column) in cells.iter_mut().zip(&columns) {
                if let CellValue::Binary { hex } = cell
                    && text_columns.contains(column)
                {
                    *cell = CellValue::Text {
                        text: text_from_hex(column, hex)?,
                    };
                }
            }
            if let Some(text) = writer.row(&cells)? {
                sink.write(&text).await?;
            }
            sink.progress.rows.fetch_add(1, Ordering::Relaxed);
        }
    }
    sink.cancelled()?;
    let rest = writer.flush();
    sink.write(&rest).await?;
    let _ = Connection::ping(connection).await;
    Ok(columns.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn literals_match_the_shared_vectors() {
        let vectors: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/sql-literals.json")).unwrap();
        for vector in vectors.as_array().unwrap() {
            let value = vector["value"].as_str().unwrap();
            assert_eq!(string_literal(true, value), vector["mysql"], "{value}");
            assert_eq!(
                string_literal(false, value),
                vector["postgresql"],
                "{value}"
            );
        }
    }
    fn text(t: &str) -> CellValue {
        CellValue::Text { text: t.into() }
    }
    #[test]
    fn csv_keeps_null_empty_and_quotes_distinct_and_refuses_binary() {
        let o = ExportOptions::default();
        assert_eq!(csv_field("c", &CellValue::Null, &o).unwrap(), "NULL");
        assert_eq!(csv_field("c", &text(""), &o).unwrap(), "\"\"");
        assert_eq!(csv_field("c", &text("NULL"), &o).unwrap(), "\"NULL\"");
        assert_eq!(
            csv_field("c", &text("a\"b,\nc"), &o).unwrap(),
            "\"a\"\"b,\nc\""
        );
        assert!(matches!(
            csv_field("blob", &CellValue::Binary { hex: "00".into() }, &o),
            Err(TransferError::Unsupported(message)) if message.contains("blob")
        ));
        assert!(matches!(
            sql_value(true, "iv", &CellValue::Unknown { database_type: "INTERVAL".into(), hex: "00".into() }),
            Err(TransferError::Unsupported(message)) if message.contains("iv")
        ));
    }
    #[test]
    fn csv_options_change_separator_enclosure_and_null_but_never_lose_distinctions() {
        let single = ExportOptions {
            csv_delimiter: ';',
            csv_enclosure: Some('\''),
            csv_null: "\\N".into(),
            ..Default::default()
        };
        assert!(single.valid());
        assert_eq!(csv_field("c", &text("it's"), &single).unwrap(), "'it''s'");
        assert_eq!(csv_field("c", &CellValue::Null, &single).unwrap(), "\\N");
        let bare = ExportOptions {
            csv_enclosure: None,
            ..Default::default()
        };
        assert_eq!(csv_field("c", &text("plain"), &bare).unwrap(), "plain");
        for ambiguous in ["a,b", "line\nbreak", "NULL"] {
            assert!(
                matches!(
                    csv_field("c", &text(ambiguous), &bare),
                    Err(TransferError::Unsupported(message)) if message.contains("column c")
                ),
                "{ambiguous}"
            );
        }
        for invalid in [
            ExportOptions {
                csv_delimiter: '\n',
                ..Default::default()
            },
            ExportOptions {
                csv_enclosure: Some(','),
                ..Default::default()
            },
            ExportOptions {
                csv_null: "a,b".into(),
                ..Default::default()
            },
            ExportOptions {
                rows_per_insert: 0,
                ..Default::default()
            },
            ExportOptions {
                rows_per_insert: SQL_BATCH_ROWS + 1,
                ..Default::default()
            },
        ] {
            assert!(!invalid.valid(), "{invalid:?}");
        }
    }
    #[test]
    fn json_keeps_numbers_exact_and_null_distinct() {
        assert_eq!(
            json_value(
                "n",
                &CellValue::Integer {
                    text: "18446744073709551615".into()
                }
            )
            .unwrap(),
            "\"18446744073709551615\""
        );
        assert_eq!(json_value("n", &CellValue::Null).unwrap(), "null");
        assert_eq!(json_value("n", &text("")).unwrap(), "\"\"");
        assert_eq!(
            json_value("b", &CellValue::Boolean { value: false }).unwrap(),
            "false"
        );
        assert_eq!(json_value("q", &text("a\"\n")).unwrap(), "\"a\\\"\\n\"");
        assert!(
            matches!(json_value("blob", &CellValue::Binary { hex: "00".into() }), Err(TransferError::Unsupported(m)) if m.contains("blob"))
        );
    }
    #[test]
    fn sql_batches_rows_and_encodes_binary_per_engine() {
        let options = ExportOptions::default();
        let mut writer = Writer {
            mysql_family: true,
            format: ExportFormat::Sql,
            options: &options,
            table: "t".into(),
            columns: vec!["id".into(), "b".into()],
            batch: Vec::new(),
            rows: 0,
        };
        let cells = [
            CellValue::Integer {
                text: "18446744073709551615".into(),
            },
            CellValue::Binary { hex: "00ff".into() },
        ];
        for _ in 0..SQL_BATCH_ROWS - 1 {
            assert_eq!(writer.row(&cells).unwrap(), None);
        }
        let batch = writer.row(&cells).unwrap().unwrap();
        assert!(batch.starts_with("INSERT INTO `t` (`id`, `b`) VALUES\n(_utf8mb4 X'3138343436373434303733373039353531363135', X'00ff'),\n"));
        assert_eq!(writer.flush(), "");
        let single = ExportOptions {
            rows_per_insert: 1,
            ..Default::default()
        };
        let mut one = Writer {
            options: &single,
            batch: Vec::new(),
            rows: 0,
            ..writer
        };
        assert_eq!(
            one.row(&cells)
                .unwrap()
                .unwrap()
                .matches("INSERT INTO")
                .count(),
            1
        );
        assert_eq!(
            sql_value(false, "b", &CellValue::Binary { hex: "00ff".into() }).unwrap(),
            "E'\\\\x00ff'"
        );
        assert_eq!(
            sql_value(false, "f", &CellValue::Boolean { value: true }).unwrap(),
            "TRUE"
        );
    }
    #[test]
    fn auto_increment_counter_is_removed_only_from_table_options() {
        let ddl = "CREATE TABLE `t` (\n  `id` int NOT NULL AUTO_INCREMENT,\n  `note` varchar(9) DEFAULT 'x)AUTO_INCREMENT=5',\n  PRIMARY KEY (`id`)\n) ENGINE=InnoDB AUTO_INCREMENT=42 DEFAULT CHARSET=utf8mb4";
        assert_eq!(
            without_auto_increment(ddl),
            "CREATE TABLE `t` (\n  `id` int NOT NULL AUTO_INCREMENT,\n  `note` varchar(9) DEFAULT 'x)AUTO_INCREMENT=5',\n  PRIMARY KEY (`id`)\n) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4"
        );
        let options = ExportOptions {
            insert_verb: InsertVerb::Replace,
            ..Default::default()
        };
        let mut writer = Writer {
            mysql_family: true,
            format: ExportFormat::Sql,
            options: &options,
            table: "t".into(),
            columns: vec!["id".into()],
            batch: vec!["(1)".into()],
            rows: 1,
        };
        assert_eq!(writer.flush(), "REPLACE INTO `t` (`id`) VALUES\n(1);\n");
    }
    #[test]
    fn file_wrapping_follows_the_options() {
        let request = |tables: Option<Vec<String>>, format, options| ExportRequest {
            tables,
            format,
            structure: true,
            data: true,
            options,
        };
        let all = ExportOptions {
            drop_table: true,
            transaction: true,
            disable_foreign_keys: true,
            ..Default::default()
        };
        let r = request(None, ExportFormat::Sql, all);
        let mut file = File::new(true, &r, &["a".into(), "b".into()]);
        let begin = file.begin();
        assert!(begin.contains("SET FOREIGN_KEY_CHECKS = 0;\nSTART TRANSACTION;\n"));
        assert_eq!(
            file.table("a"),
            "\n-- Table `a`\nDROP TABLE IF EXISTS `a`;\n"
        );
        assert_eq!(file.end(), "COMMIT;\nSET FOREIGN_KEY_CHECKS = 1;\n");
        let plain = request(
            Some(vec!["a".into()]),
            ExportFormat::Sql,
            ExportOptions::default(),
        );
        let mut file = File::new(true, &plain, &["a".into()]);
        assert!(
            !file.begin().contains("FOREIGN_KEY")
                && file.table("a").is_empty()
                && file.end().is_empty()
        );
        let json = request(
            Some(vec!["a".into(), "b".into()]),
            ExportFormat::Json,
            ExportOptions::default(),
        );
        let mut file = File::new(true, &json, &["a".into(), "b".into()]);
        let text = [
            file.begin(),
            file.table("a"),
            "  {}".into(),
            file.table("b"),
            file.end(),
        ]
        .concat();
        assert_eq!(text, "{\n\"a\": [\n  {}\n],\n\"b\": [\n\n]\n}\n");
    }
}
