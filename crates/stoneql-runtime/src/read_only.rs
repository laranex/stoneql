//! Intentionally restricted SELECT subset shared by every engine. Engine read-only transactions and database
//! privileges remain authoritative; this conservative lexer is an additional refusal policy.
use super::{ConnectionId, Connections, EnginePool, LifecycleError, OwnerId, mysql, postgres};
use futures_util::TryStreamExt;
use sqlx::{
    Column as _, Database, Executor, IntoArguments, Row, SqlSafeStr, Statement, TypeInfo,
    database::HasStatementCache, pool::PoolConnection,
};
use std::time::Duration;
use stoneql_contracts::v1::{CellValue, Column, ResultSet};

#[derive(Clone, Copy)]
pub struct QueryLimits {
    pub rows: usize,
    pub bytes: usize,
    pub deadline: Duration,
}
impl QueryLimits {
    /// Upper bounds enforced by `read_only`; hosts use them as defaults when a caller omits limits.
    pub const MAX_ROWS: usize = 1000;
    pub const MAX_BYTES: usize = 1_048_576;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadError {
    InvalidLimits,
    UnsupportedSql,
    Connection(LifecycleError),
    Timeout,
    Database,
    ResultTooWide,
    PacketTooLarge,
    /// A read-write handle is running a job or holds an open transaction.
    Busy,
}
impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidLimits => "Invalid result limits.",
            Self::UnsupportedSql => "This read-only SQL form is not supported.",
            Self::Connection(_) => "Connection is not available to this session.",
            Self::Timeout => "Query deadline exceeded; server cancellation is unconfirmed.",
            Self::Database => "The database rejected or could not complete the query.",
            Self::PacketTooLarge => "The database response exceeded the incoming packet limit; its connection was discarded.",
            Self::ResultTooWide => "Result metadata exceeds the configured byte limit.",
            Self::Busy => "The connection is busy with another statement or an open transaction.",
        })
    }
}
impl std::error::Error for ReadError {}
pub(crate) fn driver_error(error: sqlx::Error) -> ReadError {
    match error {
        sqlx::Error::Protocol(message)
            if message == "receive packet exceeds configured byte limit" =>
        {
            ReadError::PacketTooLarge
        }
        _ => ReadError::Database,
    }
}

impl Connections {
    pub async fn read_only(
        &self,
        owner: OwnerId,
        id: ConnectionId,
        sql: &str,
        limits: QueryLimits,
    ) -> Result<ResultSet, ReadError> {
        if !(1..=QueryLimits::MAX_ROWS).contains(&limits.rows)
            || !(1..=QueryLimits::MAX_BYTES).contains(&limits.bytes)
            || limits.deadline.is_zero()
            || limits.deadline > Duration::from_secs(30)
        {
            return Err(ReadError::InvalidLimits);
        }
        let pool = self
            .owned_pool(owner, id)
            .ok_or(ReadError::Connection(LifecycleError::Forbidden))?;
        // Read-only handles are unchanged; read-write handles never run this beside a job or
        // an open transaction (one physical session of work per handle).
        let _exclusive = self.exclusive_read(owner, id)?;
        validate_select(sql, pool.dialect())?;
        tokio::time::timeout(limits.deadline, async {
            match pool {
                EnginePool::MySql(pool) => {
                    let connection = pool.acquire().await.map_err(driver_error)?;
                    // MySQL accepts this only through the static text protocol.
                    finish(
                        connection,
                        &["START TRANSACTION READ ONLY".into()],
                        sql,
                        limits,
                        mysql::decode_cell,
                    )
                    .await
                }
                EnginePool::Postgres(pool) => postgres::read(&pool, sql, limits).await,
                #[cfg(feature = "sqlite")]
                EnginePool::Sqlite(pool) => super::sqlite::read(&pool, sql, limits).await,
            }
        })
        .await
        .map_err(|_| ReadError::Timeout)?
    }
}

/// Shared bounded execution for every engine. The caller supplies an acquired physical
/// connection and fixed, trusted engine statements (never request data) that establish the
/// engine's read-only enforcement before the user statement is prepared.
pub(crate) async fn finish<DB>(
    mut connection: PoolConnection<DB>,
    prelude: &[String],
    sql: &str,
    limits: QueryLimits,
    decode: fn(&DB::Row, usize) -> Result<CellValue, ReadError>,
) -> Result<ResultSet, ReadError>
where
    DB: Database + HasStatementCache,
    DB::Arguments: IntoArguments<DB>,
    for<'c> &'c mut DB::Connection: Executor<'c, Database = DB>,
{
    // Discard even successful connections: no caller-controlled session state is reused.
    // On timeout/error the socket/worker is closed, never returned with an uncertain transaction.
    connection.close_on_drop();
    for statement in prelude {
        sqlx::raw_sql(sqlx::AssertSqlSafe(statement.clone()).into_sql_str())
            .execute(&mut *connection)
            .await
            .map_err(driver_error)?;
    }
    // This is the complete, explicitly requested SQL-editor statement, never a
    // generated query containing interpolated data. The refusal policy ran above.
    let statement = (&mut *connection)
        .prepare(sqlx::AssertSqlSafe(sql.to_owned()).into_sql_str())
        .await
        .map_err(driver_error)?;
    let columns: Vec<_> = statement
        .columns()
        .iter()
        .map(|column| Column {
            name: column.name().into(),
            database_type: column.type_info().name().into(),
            nullable: None,
        })
        .collect();
    let mut result = ResultSet {
        index: 0,
        columns,
        rows: Vec::new(),
        affected_rows: "0".into(),
        truncated: false,
    };
    let mut bytes = serde_json::to_vec(&result)
        .map_err(|_| ReadError::Database)?
        .len();
    if bytes > limits.bytes {
        return Err(ReadError::ResultTooWide);
    }
    {
        let mut rows = statement.query().persistent(false).fetch(&mut *connection);
        while let Some(row) = rows.try_next().await.map_err(driver_error)? {
            if result.rows.len() == limits.rows {
                result.truncated = true;
                break;
            }
            let values = (0..row.len())
                .map(|index| decode(&row, index))
                .collect::<Result<Vec<_>, _>>()?;
            let size = serde_json::to_vec(&values)
                .map_err(|_| ReadError::Database)?
                .len()
                + 1;
            if bytes + size > limits.bytes {
                result.truncated = true;
                break;
            }
            bytes += size;
            result.rows.push(values);
        }
    }
    // Closing also rolls back this private safety transaction. Never expose a lease.
    // No need to drain an oversized stream or reuse its protocol state.
    connection.close().await.map_err(driver_error)?;
    Ok(result)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Dialect {
    MySql,
    Postgres,
    Sqlite,
}
impl Dialect {
    /// Identifier quote characters; single quotes are always string literals.
    fn identifier_quotes(self) -> &'static [u8] {
        match self {
            Self::MySql => b"`",
            Self::Postgres => b"\"",
            Self::Sqlite => b"\"`",
        }
    }
    /// Known built-ins only. Each list is limited to functions of that engine whose effects
    /// are confined to the returned value.
    fn functions(self) -> &'static [&'static str] {
        match self {
            Self::MySql => &[
                "CHAR_LENGTH",
                "IFNULL",
                "CONCAT",
                "SUBSTRING",
                "DATE",
                "YEAR",
                "MONTH",
                "DAY",
                "VERSION",
                "DATABASE",
            ],
            Self::Postgres => &["CHAR_LENGTH", "CONCAT", "SUBSTRING", "VERSION"],
            Self::Sqlite => &["IFNULL", "SUBSTR", "TYPEOF", "SQLITE_VERSION"],
        }
    }
}

fn validate_select(sql: &str, dialect: Dialect) -> Result<(), ReadError> {
    if sql.is_empty() || sql.len() > 65_536 {
        return Err(ReadError::UnsupportedSql);
    }
    let bytes = sql.as_bytes();
    let mut index = 0;
    let mut tokens: Vec<(String, bool, usize)> = Vec::new();
    while index < bytes.len() {
        let start = index;
        let byte = bytes[index];
        if byte.is_ascii_whitespace() {
            index += 1;
            continue;
        }
        if byte == b'\'' || dialect.identifier_quotes().contains(&byte) {
            let quote = byte;
            index += 1;
            let mut closed = false;
            while index < bytes.len() {
                if bytes[index] == b'\\' {
                    return Err(ReadError::UnsupportedSql);
                }
                if bytes[index] == quote {
                    index += 1;
                    if bytes.get(index) == Some(&quote) {
                        index += 1;
                        continue;
                    }
                    closed = true;
                    break;
                }
                index += 1;
            }
            if !closed {
                return Err(ReadError::UnsupportedSql);
            }
            tokens.push(("quoted".into(), true, index));
            continue;
        }
        if byte.is_ascii_alphabetic() || byte == b'_' {
            index += 1;
            while index < bytes.len()
                && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
            {
                index += 1;
            }
            tokens.push((sql[start..index].to_ascii_uppercase(), false, index));
            continue;
        }
        if byte.is_ascii_digit() {
            index += 1;
            while index < bytes.len() && bytes[index].is_ascii_digit() {
                index += 1;
            }
            tokens.push(("number".into(), true, index));
            continue;
        }
        if byte == b';' {
            if bytes[index + 1..].iter().all(u8::is_ascii_whitespace) {
                break;
            }
            return Err(ReadError::UnsupportedSql);
        }
        if !b"(),.*+-/%=<>!".contains(&byte)
            || bytes.get(index..index + 2).is_some_and(|value| {
                [b"--".as_slice(), b"/*".as_slice(), b"*/".as_slice()].contains(&value)
            })
        {
            return Err(ReadError::UnsupportedSql);
        }
        tokens.push(((byte as char).to_string(), false, index + 1));
        index += 1;
    }
    if tokens.first().map(|token| token.0.as_str()) != Some("SELECT") {
        return Err(ReadError::UnsupportedSql);
    }
    const DENIED: &[&str] = &[
        "INTO",
        "FOR",
        "LOCK",
        "PROCEDURE",
        "INSERT",
        "UPDATE",
        "DELETE",
        "REPLACE",
        "CREATE",
        "ALTER",
        "DROP",
        "TRUNCATE",
        "CALL",
        "SET",
        "WITH",
        "OUTFILE",
        "DUMPFILE",
        "LOAD",
        "DO",
        "HANDLER",
    ];
    const FUNCTIONS: &[&str] = &[
        "COUNT", "SUM", "MIN", "MAX", "AVG", "ABS", "ROUND", "FLOOR", "CEIL", "LENGTH", "LOWER",
        "UPPER", "COALESCE", "NULLIF",
    ];
    for (position, (token, quoted, end)) in tokens.iter().enumerate() {
        if !quoted && DENIED.contains(&token.as_str()) {
            return Err(ReadError::UnsupportedSql);
        }
        if tokens.get(position + 1).is_some_and(|next| next.0 == "(")
            && (*quoted
                || token
                    .as_bytes()
                    .first()
                    .is_some_and(u8::is_ascii_alphabetic))
        {
            // IN/EXISTS introduce subexpressions, not invocations. Only known built-ins may execute.
            if ["IN", "EXISTS"].contains(&token.as_str()) {
                continue;
            }
            if !(FUNCTIONS.contains(&token.as_str())
                || dialect.functions().contains(&token.as_str()))
                || bytes.get(*end) != Some(&b'(')
                || position > 0 && tokens[position - 1].0 == "."
            {
                return Err(ReadError::UnsupportedSql);
            }
        }
    }
    Ok(())
}
/// Shortest round-trip decimal text; non-finite values use PostgreSQL/SQLite spellings.
pub(crate) fn float_text(value: f64) -> String {
    if value.is_nan() {
        "NaN".into()
    } else if value.is_infinite() {
        if value > 0.0 { "Infinity" } else { "-Infinity" }.into()
    } else {
        value.to_string()
    }
}
pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn packet_limit_errors_are_distinct_and_other_driver_details_remain_private() {
        assert_eq!(
            driver_error(sqlx::Error::Protocol(
                "receive packet exceeds configured byte limit".into()
            )),
            ReadError::PacketTooLarge
        );
        let error = driver_error(sqlx::Error::Protocol(
            "synthetic private SQL or connection details".into(),
        ));
        assert_eq!(error, ReadError::Database);
        assert!(!error.to_string().contains("private"));
    }
    #[test]
    fn select_subset_accepts_joins_aggregates_and_quoted_data() {
        for sql in [
            "SELECT * FROM `orders` WHERE `state` = 'active' ORDER BY id LIMIT 10;",
            "SELECT u.id, COUNT(o.id) FROM users u JOIN orders o ON o.user_id=u.id GROUP BY u.id",
            "SELECT 'DROP TABLE users', '' AS empty, NULL AS n",
            "SELECT a FROM t WHERE a IN (1,2)",
        ] {
            assert!(validate_select(sql, Dialect::MySql).is_ok(), "{sql}");
        }
    }
    #[test]
    fn accepts_the_renderer_live_catalog_and_browse_statements() {
        // Exact shapes from apps/frontend/src/services/liveSchema.ts (tests/liveSchema.test.ts).
        for sql in [
            "SELECT TABLE_NAME AS name, TABLE_TYPE AS kind FROM information_schema.TABLES WHERE TABLE_SCHEMA = 'o''brien' ORDER BY TABLE_NAME;",
            "SELECT COLUMN_NAME AS name, COLUMN_TYPE AS type, IS_NULLABLE AS nullable, COLUMN_KEY AS ckey, COLUMN_DEFAULT AS dflt, EXTRA AS extra FROM information_schema.COLUMNS WHERE TABLE_SCHEMA = 'shop' AND TABLE_NAME = 'odd`name' ORDER BY ORDINAL_POSITION;",
            "SELECT * FROM `users`\nWHERE LOWER(COALESCE(`id`, '')) LIKE '%50!%!_off!!%' ESCAPE '!' OR LOWER(COALESCE(`na``me`, '')) LIKE '%50!%!_off!!%' ESCAPE '!'\nORDER BY `id` DESC\nLIMIT 30, 15;",
            "SELECT * FROM `t`\nORDER BY `na``me` IS NULL ASC, `na``me` ASC\nLIMIT 0, 25;",
            "SELECT COUNT(*) AS total FROM `users`\nWHERE LOWER(COALESCE(`id`, '')) LIKE '%50!%!_off!!%' ESCAPE '!';",
        ] {
            assert!(validate_select(sql, Dialect::MySql).is_ok(), "{sql}");
        }
    }
    #[test]
    fn accepts_the_renderer_postgresql_catalog_and_browse_statements() {
        // Exact shapes from apps/frontend/src/services/liveSchema.ts (PostgreSQL).
        for sql in [
            r#"SELECT table_name AS name, table_type AS kind FROM information_schema.tables WHERE table_schema = current_schema ORDER BY table_name;"#,
            r#"SELECT c.column_name AS name, c.data_type AS type, c.udt_name AS udt, c.character_maximum_length AS len, c.numeric_precision AS prec, c.numeric_scale AS scale, c.datetime_precision AS dprec, c.is_nullable AS nullable, c.column_default AS dflt, c.is_identity AS ident, (SELECT COUNT(*) FROM pg_catalog.pg_index i JOIN pg_catalog.pg_class r ON r.oid = i.indrelid JOIN pg_catalog.pg_namespace n ON n.oid = r.relnamespace JOIN pg_catalog.pg_attribute a ON a.attrelid = r.oid WHERE i.indisprimary AND n.nspname = c.table_schema AND r.relname = c.table_name AND a.attname = c.column_name AND CONCAT(' ', i.indkey, ' ') LIKE CONCAT('% ', a.attnum, ' %')) AS pk FROM information_schema.columns c WHERE c.table_schema = current_schema AND c.table_name = 'Mixed "Case" values' ORDER BY c.ordinal_position;"#,
            r#"SELECT * FROM "Mixed ""Case"" values"
WHERE LOWER(CONCAT("id")) LIKE '%5!_%' ESCAPE '!' OR LOWER(CONCAT("n")) LIKE '%5!_%' ESCAPE '!'
ORDER BY "n" DESC NULLS LAST
LIMIT 25 OFFSET 50;"#,
            r#"SELECT COUNT(*) AS total FROM "Mixed ""Case"" values"
WHERE LOWER(CONCAT("id")) LIKE '%5!_%' ESCAPE '!' OR LOWER(CONCAT("n")) LIKE '%5!_%' ESCAPE '!';"#,
        ] {
            assert!(validate_select(sql, Dialect::Postgres).is_ok(), "{sql}");
        }
    }
    #[test]
    fn accepts_the_renderer_index_and_foreign_key_catalog_statements() {
        // Exact shapes from apps/frontend/src/services/liveSchema.ts.
        for sql in [
            r#"SELECT INDEX_NAME AS name, NON_UNIQUE AS nonunique, COLUMN_NAME AS col, INDEX_TYPE AS type FROM information_schema.STATISTICS WHERE TABLE_SCHEMA = 'stoneql_m001_fixture' AND TABLE_NAME = 'fk_child' ORDER BY INDEX_NAME, SEQ_IN_INDEX;"#,
            r#"SELECT k.CONSTRAINT_NAME AS name, k.COLUMN_NAME AS col, k.REFERENCED_TABLE_NAME AS ref_table, k.REFERENCED_COLUMN_NAME AS ref_col, r.DELETE_RULE AS on_delete, r.UPDATE_RULE AS on_update FROM information_schema.KEY_COLUMN_USAGE k LEFT JOIN information_schema.REFERENTIAL_CONSTRAINTS r ON r.CONSTRAINT_SCHEMA = k.CONSTRAINT_SCHEMA AND r.CONSTRAINT_NAME = k.CONSTRAINT_NAME WHERE k.TABLE_SCHEMA = 'stoneql_m001_fixture' AND k.TABLE_NAME = 'fk_child' AND k.REFERENCED_TABLE_NAME IS NOT NULL ORDER BY k.CONSTRAINT_NAME, k.ORDINAL_POSITION;"#,
        ] {
            assert!(validate_select(sql, Dialect::MySql).is_ok(), "{sql}");
        }
        for sql in [
            r#"SELECT indexname AS name, indexdef AS definition FROM pg_catalog.pg_indexes WHERE schemaname = current_schema AND tablename = 'fk_child' ORDER BY indexname;"#,
            r#"SELECT c.conname AS name, a.attname AS col, fr.relname AS ref_table, fa.attname AS ref_col, c.confdeltype AS on_delete, c.confupdtype AS on_update, a.attnum AS pos FROM pg_catalog.pg_constraint c JOIN pg_catalog.pg_class r ON r.oid = c.conrelid JOIN pg_catalog.pg_namespace n ON n.oid = r.relnamespace JOIN pg_catalog.pg_class fr ON fr.oid = c.confrelid JOIN pg_catalog.pg_attribute a ON a.attrelid = c.conrelid JOIN pg_catalog.pg_attribute fa ON fa.attrelid = c.confrelid WHERE c.contype = 'f' AND n.nspname = current_schema AND r.relname = 'fk_child' AND CONCAT(',', SUBSTRING(CONCAT(c.conkey), 2, CHAR_LENGTH(CONCAT(c.conkey)) - 2), ',') LIKE CONCAT('%,', a.attnum, ',%') AND CONCAT(',', SUBSTRING(CONCAT(c.confkey), 2, CHAR_LENGTH(CONCAT(c.confkey)) - 2), ',') LIKE CONCAT('%,', fa.attnum, ',%') ORDER BY c.conname, a.attnum, fa.attnum;"#,
        ] {
            assert!(validate_select(sql, Dialect::Postgres).is_ok(), "{sql}");
        }
    }
    #[test]
    fn rejects_stacked_comments_locks_outfile_variables_and_unverified_functions() {
        for sql in [
            "SELECT 1; DELETE FROM users",
            "/* select */DELETE FROM users",
            "SELECT /*!50000 1 */ 1",
            "SELECT 1 INTO OUTFILE '/tmp/x'",
            "SELECT * FROM users FOR UPDATE",
            "SELECT GET_LOCK('x',1)",
            "SELECT SLEEP(9)",
            "SELECT custom_function()",
            "SELECT sys.COUNT(1)",
            "SELECT COUNT (1)",
            "SELECT @x:=1",
            "SELECT 'unterminated",
            "SELECT 'back\\slash'",
            "CALL p()",
            "WITH t AS (SELECT 1) SELECT * FROM t",
            "SELECT 1--comment",
        ] {
            assert_eq!(
                validate_select(sql, Dialect::MySql),
                Err(ReadError::UnsupportedSql),
                "{sql}"
            );
        }
    }
    #[test]
    fn dialects_quote_identifiers_and_allow_only_their_own_built_ins() {
        for (sql, dialect) in [
            (
                r#"SELECT "Mixed ""Case""" FROM "odd table""#,
                Dialect::Postgres,
            ),
            (r#"SELECT "a", `b`, typeof(c) FROM t"#, Dialect::Sqlite),
            ("SELECT version(), char_length(x) FROM t", Dialect::Postgres),
            (
                "SELECT sqlite_version(), ifnull(a, 0) FROM t",
                Dialect::Sqlite,
            ),
        ] {
            assert!(validate_select(sql, dialect).is_ok(), "{sql}");
        }
        for (sql, dialect) in [
            (r#"SELECT "x" FROM t"#, Dialect::MySql),
            ("SELECT `x` FROM t", Dialect::Postgres),
            ("SELECT x::int FROM t", Dialect::Postgres),
            ("SELECT $1", Dialect::Postgres),
            ("SELECT $$DROP$$", Dialect::Postgres),
            ("SELECT pg_sleep(1)", Dialect::Postgres),
            ("SELECT nextval('s')", Dialect::Postgres),
            ("SELECT pg_advisory_lock(1)", Dialect::Postgres),
            ("SELECT * FROM t FOR SHARE", Dialect::Postgres),
            ("SELECT * INTO copy FROM t", Dialect::Postgres),
            ("SELECT E'\\x'", Dialect::Postgres),
            ("SELECT database()", Dialect::Postgres),
            ("SELECT typeof(x) FROM t", Dialect::MySql),
            ("SELECT load_extension('x')", Dialect::Sqlite),
            ("SELECT version()", Dialect::Sqlite),
            ("SELECT [x] FROM t", Dialect::Sqlite),
        ] {
            assert_eq!(
                validate_select(sql, dialect),
                Err(ReadError::UnsupportedSql),
                "{sql}"
            );
        }
    }
}
