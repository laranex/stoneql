//! M007 streaming export of one table to SQL or CSV (alpha slice, D022). Rows stream on one
//! dedicated connection inside an engine-enforced read-only snapshot transaction; nothing is held
//! beyond one INSERT batch. Values are written exactly: no type is reinterpreted, NULL and the
//! empty string stay distinct, and a value that cannot be written exactly fails the export and
//! names its column. The connection is always discarded afterwards.
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
}
#[derive(Debug, Clone)]
pub struct ExportRequest {
    pub table: String,
    pub format: ExportFormat,
    /// SQL only: include `CREATE TABLE` (MySQL/MariaDB `SHOW CREATE TABLE`).
    pub structure: bool,
    pub data: bool,
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

/// Rows per multi-row INSERT in SQL exports (Proposed, alpha plan import batch size).
pub const SQL_BATCH_ROWS: usize = 500;

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
            return Err(TransferError::Unsupported(format!(
                "column {column} has type {database_type}, which StoneQL cannot export exactly"
            )));
        }
    })
}
/// CSV (RFC 4180 quoting, `\n` rows): every non-NULL value is quoted, NULL is the unquoted word
/// `NULL`, so `""` is the empty string and `"NULL"` the four-letter string. MySQL
/// `LOAD DATA … FIELDS ENCLOSED BY '"' ESCAPED BY ''` and PostgreSQL `COPY … (FORMAT csv,
/// NULL 'NULL')` read it back exactly. Binary values have no exact CSV form and are refused.
fn csv_field(column: &str, cell: &CellValue) -> Result<String, TransferError> {
    let quoted = |text: &str| format!("\"{}\"", text.replace('"', "\"\""));
    Ok(match cell {
        CellValue::Null => "NULL".into(),
        CellValue::Boolean { value } => quoted(if *value { "true" } else { "false" }),
        CellValue::Text { text }
        | CellValue::Integer { text }
        | CellValue::Decimal { text }
        | CellValue::Float { text }
        | CellValue::Temporal { text, .. }
        | CellValue::Json { text } => quoted(text),
        CellValue::Binary { .. } => {
            return Err(TransferError::Unsupported(format!(
                "column {column} holds binary data, which CSV cannot represent exactly; export as SQL"
            )));
        }
        CellValue::Unknown { database_type, .. } => {
            return Err(TransferError::Unsupported(format!(
                "column {column} has type {database_type}, which StoneQL cannot export exactly"
            )));
        }
    })
}

struct Sink<'a, W> {
    out: &'a mut W,
    progress: &'a ExportProgress,
}
impl<W: AsyncWrite + Unpin + Send> Sink<'_, W> {
    async fn write(&mut self, text: &str) -> Result<(), TransferError> {
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

/// Formats rows as they arrive: a CSV line each, or SQL INSERTs batched by `SQL_BATCH_ROWS`.
struct Writer {
    mysql_family: bool,
    format: ExportFormat,
    table: String,
    columns: Vec<String>,
    batch: Vec<String>,
}
impl Writer {
    fn header(&self) -> String {
        match self.format {
            ExportFormat::Csv => {
                let names: Vec<String> = self
                    .columns
                    .iter()
                    .map(|name| format!("\"{}\"", name.replace('"', "\"\"")))
                    .collect();
                format!("{}\n", names.join(","))
            }
            ExportFormat::Sql => String::new(),
        }
    }
    fn row(&mut self, cells: &[CellValue]) -> Result<Option<String>, TransferError> {
        match self.format {
            ExportFormat::Csv => {
                let fields = cells
                    .iter()
                    .zip(&self.columns)
                    .map(|(cell, column)| csv_field(column, cell))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Some(format!("{}\n", fields.join(","))))
            }
            ExportFormat::Sql => {
                let values = cells
                    .iter()
                    .zip(&self.columns)
                    .map(|(cell, column)| sql_value(self.mysql_family, column, cell))
                    .collect::<Result<Vec<_>, _>>()?;
                self.batch.push(format!("({})", values.join(", ")));
                Ok((self.batch.len() >= SQL_BATCH_ROWS).then(|| self.flush()))
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
        format!(
            "INSERT INTO {} ({}) VALUES\n{rows};\n",
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
    /// Streams one table to `out`. The caller owns the destination (temporary file, atomic
    /// rename, removal on any error). Read-write handles are refused while they run other work,
    /// like reads (one physical session of work per handle).
    pub async fn export_table<W: AsyncWrite + Unpin + Send>(
        &self,
        owner: OwnerId,
        id: ConnectionId,
        request: &ExportRequest,
        out: &mut W,
        progress: &ExportProgress,
    ) -> Result<ExportSummary, TransferError> {
        if !valid_table(&request.table)
            || (!request.data && !request.structure)
            || (request.format == ExportFormat::Csv && (request.structure || !request.data))
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
                let table = quote_identifier(true, &request.table);
                if request.format == ExportFormat::Sql {
                    sink.write(&format!(
                        "-- StoneQL export of {} (data in a read-only consistent snapshot).\nSET NAMES utf8mb4;\nSET SESSION sql_mode = 'NO_AUTO_VALUE_ON_ZERO';\n",
                        table
                    ))
                    .await?;
                }
                if request.structure {
                    let row = sqlx::raw_sql(
                        sqlx::AssertSqlSafe(format!("SHOW CREATE TABLE {table}")).into_sql_str(),
                    )
                    .fetch_one(&mut *connection)
                    .await
                    .map_err(driver_error)?;
                    let bytes: Vec<u8> = row
                        .try_get_unchecked(1)
                        .map_err(|_| TransferError::Read(ReadError::Database))?;
                    let ddl = String::from_utf8(bytes)
                        .map_err(|_| TransferError::Read(ReadError::Database))?;
                    sink.write(&format!("{ddl};\n")).await?;
                }
                let columns = if request.data {
                    // Servers set the BINARY column flag for `_bin` collations too (for example
                    // MariaDB JSON), so the driver names such text columns binary. The catalog's
                    // character set is authoritative: a column with one holds text.
                    let text_columns: Vec<String> = sqlx::query_scalar(
                        "SELECT COLUMN_NAME FROM information_schema.COLUMNS WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = ? AND CHARACTER_SET_NAME IS NOT NULL",
                    )
                    .bind(&request.table)
                    .fetch_all(&mut *connection)
                    .await
                    .map_err(driver_error)?;
                    stream::<sqlx::MySql, _>(
                        &mut connection,
                        &format!("SELECT * FROM {table}"),
                        mysql::decode_cell,
                        true,
                        &text_columns,
                        request,
                        &mut sink,
                    )
                    .await?
                } else {
                    0
                };
                let _ = sqlx::raw_sql("ROLLBACK").execute(&mut *connection).await;
                columns
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
                if request.format == ExportFormat::Sql {
                    sink.write(&format!(
                        "-- StoneQL export of {} (data in a read-only repeatable-read snapshot).\n",
                        quote_identifier(false, &request.table)
                    ))
                    .await?;
                }
                let columns = stream::<sqlx::Postgres, _>(
                    &mut connection,
                    &format!("SELECT * FROM {}", quote_identifier(false, &request.table)),
                    postgres::decode_cell,
                    false,
                    &[],
                    request,
                    &mut sink,
                )
                .await?;
                let _ = sqlx::raw_sql("ROLLBACK").execute(&mut *connection).await;
                columns
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

/// Prepares the generated `SELECT *` (identifiers quoted, no request data in values), then
/// formats each row as it arrives. Returns the column count.
async fn stream<DB, W>(
    connection: &mut DB::Connection,
    sql: &str,
    decode: fn(&DB::Row, usize) -> Result<CellValue, ReadError>,
    mysql_family: bool,
    text_columns: &[String],
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
        table: request.table.clone(),
        columns: columns.clone(),
        batch: Vec::new(),
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
    #[test]
    fn csv_keeps_null_empty_and_quotes_distinct_and_refuses_binary() {
        assert_eq!(csv_field("c", &CellValue::Null).unwrap(), "NULL");
        let text = |t: &str| CellValue::Text { text: t.into() };
        assert_eq!(csv_field("c", &text("")).unwrap(), "\"\"");
        assert_eq!(csv_field("c", &text("NULL")).unwrap(), "\"NULL\"");
        assert_eq!(csv_field("c", &text("a\"b,\nc")).unwrap(), "\"a\"\"b,\nc\"");
        assert!(matches!(
            csv_field("blob", &CellValue::Binary { hex: "00".into() }),
            Err(TransferError::Unsupported(message)) if message.contains("blob")
        ));
        assert!(matches!(
            sql_value(true, "iv", &CellValue::Unknown { database_type: "INTERVAL".into(), hex: "00".into() }),
            Err(TransferError::Unsupported(message)) if message.contains("iv")
        ));
    }
    #[test]
    fn sql_batches_rows_and_encodes_binary_per_engine() {
        let mut writer = Writer {
            mysql_family: true,
            format: ExportFormat::Sql,
            table: "t".into(),
            columns: vec!["id".into(), "b".into()],
            batch: Vec::new(),
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
        assert_eq!(
            sql_value(false, "b", &CellValue::Binary { hex: "00ff".into() }).unwrap(),
            "E'\\\\x00ff'"
        );
        assert_eq!(
            sql_value(false, "f", &CellValue::Boolean { value: true }).unwrap(),
            "TRUE"
        );
    }
}
