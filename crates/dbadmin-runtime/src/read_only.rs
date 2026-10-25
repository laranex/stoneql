//! Intentionally restricted M003 SELECT subset. Engine read-only transactions and database
//! privileges remain authoritative; this conservative lexer is an additional refusal policy.
use super::{ConnectionId, Connections, LifecycleError, OwnerId};
use dbadmin_contracts::v1::{CellValue, Column, ResultSet};
use futures_util::TryStreamExt;
use sqlx::{
    Column as _, Executor, Row, SqlSafeStr, Statement, TypeInfo, ValueRef, mysql::MySqlRow,
};
use std::time::Duration;
use tokio::time::Instant;

#[derive(Clone, Copy)]
pub struct QueryLimits {
    pub rows: usize,
    pub bytes: usize,
    pub deadline: Duration,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadError {
    InvalidLimits,
    UnsupportedSql,
    Connection(LifecycleError),
    Timeout,
    Database,
    ResultTooWide,
}
impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidLimits => "Invalid result limits.",
            Self::UnsupportedSql => "This read-only SQL form is not supported.",
            Self::Connection(_) => "Connection is not available to this session.",
            Self::Timeout => "Query deadline exceeded; server cancellation is unconfirmed.",
            Self::Database => "The database rejected or could not complete the query.",
            Self::ResultTooWide => "Result metadata exceeds the configured byte limit.",
        })
    }
}
impl std::error::Error for ReadError {}
impl Connections {
    pub async fn read_only(
        &self,
        owner: OwnerId,
        id: ConnectionId,
        sql: &str,
        limits: QueryLimits,
    ) -> Result<ResultSet, ReadError> {
        if !(1..=1000).contains(&limits.rows)
            || !(1..=1_048_576).contains(&limits.bytes)
            || limits.deadline.is_zero()
            || limits.deadline > Duration::from_secs(30)
        {
            return Err(ReadError::InvalidLimits);
        }
        validate_select(sql)?;
        let pool = {
            let entries = self.0.entries.lock().expect("connection registry lock");
            entries
                .get(&id)
                .filter(|entry| {
                    entry.owner == owner
                        && entry.expires > Instant::now()
                        && !entry.pool.is_closed()
                })
                .map(|entry| entry.pool.clone())
                .ok_or(ReadError::Connection(LifecycleError::Forbidden))?
        };
        tokio::time::timeout(limits.deadline, async {
            let mut connection = pool.acquire().await.map_err(|_| ReadError::Database)?;
            // Discard even successful connections: no caller-controlled session state is reused.
            // On timeout/error the socket is closed, never returned with an uncertain transaction.
            connection.close_on_drop();
            sqlx::raw_sql("START TRANSACTION READ ONLY")
                .execute(&mut *connection)
                .await
                .map_err(|_| ReadError::Database)?;
            // This is the complete, explicitly requested SQL-editor statement, never a
            // generated query containing interpolated data. The refusal policy ran above.
            let statement = connection
                .prepare(sqlx::AssertSqlSafe(sql).into_sql_str())
                .await
                .map_err(|_| ReadError::Database)?;
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
                while let Some(row) = rows.try_next().await.map_err(|_| ReadError::Database)? {
                    if result.rows.len() == limits.rows {
                        result.truncated = true;
                        break;
                    }
                    let values = (0..row.len())
                        .map(|index| decode_cell(&row, index))
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
            connection.close().await.map_err(|_| ReadError::Database)?;
            Ok(result)
        })
        .await
        .map_err(|_| ReadError::Timeout)?
    }
}

fn validate_select(sql: &str) -> Result<(), ReadError> {
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
        if byte == b'\'' || byte == b'`' {
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
        "COUNT",
        "SUM",
        "MIN",
        "MAX",
        "AVG",
        "ABS",
        "ROUND",
        "FLOOR",
        "CEIL",
        "LENGTH",
        "CHAR_LENGTH",
        "LOWER",
        "UPPER",
        "COALESCE",
        "IFNULL",
        "NULLIF",
        "CONCAT",
        "SUBSTRING",
        "DATE",
        "YEAR",
        "MONTH",
        "DAY",
        "VERSION",
        "DATABASE",
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
            if !FUNCTIONS.contains(&token.as_str())
                || bytes.get(*end) != Some(&b'(')
                || position > 0 && tokens[position - 1].0 == "."
            {
                return Err(ReadError::UnsupportedSql);
            }
        }
    }
    Ok(())
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn decode_cell(row: &MySqlRow, index: usize) -> Result<CellValue, ReadError> {
    let raw = row.try_get_raw(index).map_err(|_| ReadError::Database)?;
    let kind = raw.type_info().name().to_owned();
    // SQLx treats MySQL zero dates as NULL. Decode non-optional bytes first so a present zero
    // date remains distinct from the protocol NULL bitmap. No unsafe/private API is needed.
    let bytes = match row.try_get_unchecked::<Vec<u8>, _>(index) {
        Ok(bytes) => bytes,
        Err(_) if raw.is_null() => return Ok(CellValue::Null),
        Err(_) => return Err(ReadError::Database),
    };
    let unknown = || CellValue::Unknown {
        database_type: kind.clone(),
        hex: hex(&bytes),
    };
    let text = || String::from_utf8(bytes.clone()).ok();
    Ok(match kind.as_str() {
        "BOOLEAN" | "TINYINT" | "SMALLINT" | "MEDIUMINT" | "INT" | "BIGINT" | "YEAR" => row
            .try_get_unchecked::<i64, _>(index)
            .map(|value| CellValue::Integer {
                text: value.to_string(),
            })
            .unwrap_or_else(|_| unknown()),
        "TINYINT UNSIGNED" | "SMALLINT UNSIGNED" | "MEDIUMINT UNSIGNED" | "INT UNSIGNED"
        | "BIGINT UNSIGNED" => row
            .try_get_unchecked::<u64, _>(index)
            .map(|value| CellValue::Integer {
                text: value.to_string(),
            })
            .unwrap_or_else(|_| unknown()),
        "DECIMAL" => text()
            .map(|text| CellValue::Decimal { text })
            .unwrap_or_else(unknown),
        "FLOAT" => row
            .try_get_unchecked::<f32, _>(index)
            .map(|value| CellValue::Float {
                text: value.to_string(),
            })
            .unwrap_or_else(|_| unknown()),
        "DOUBLE" => row
            .try_get_unchecked::<f64, _>(index)
            .map(|value| CellValue::Float {
                text: value.to_string(),
            })
            .unwrap_or_else(|_| unknown()),
        "DATE" | "DATETIME" | "TIMESTAMP" | "TIME" => temporal(&kind, &bytes)
            .map(|text| CellValue::Temporal {
                database_type: kind.clone(),
                text,
            })
            .unwrap_or_else(unknown),
        "CHAR" | "VARCHAR" | "TINYTEXT" | "TEXT" | "MEDIUMTEXT" | "LONGTEXT" | "ENUM" | "SET" => {
            text()
                .map(|text| CellValue::Text { text })
                .unwrap_or_else(unknown)
        }
        "JSON" => text()
            .map(|text| CellValue::Json { text })
            .unwrap_or_else(unknown),
        "BINARY" | "VARBINARY" | "TINYBLOB" | "BLOB" | "MEDIUMBLOB" | "LONGBLOB" | "BIT" => {
            CellValue::Binary { hex: hex(&bytes) }
        }
        _ => unknown(),
    })
}
fn temporal(kind: &str, bytes: &[u8]) -> Option<String> {
    let length = *bytes.first()? as usize;
    if length + 1 != bytes.len() {
        return None;
    }
    if kind == "TIME" {
        if length == 0 {
            return Some("00:00:00".into());
        }
        if ![8, 12].contains(&length) {
            return None;
        }
        let days = u32::from_le_bytes(bytes[2..6].try_into().ok()?) as u64;
        let mut text = format!(
            "{}{:02}:{:02}:{:02}",
            if bytes[1] == 1 { "-" } else { "" },
            days * 24 + u64::from(bytes[6]),
            bytes[7],
            bytes[8]
        );
        if length == 12 {
            text.push_str(&format!(
                ".{:06}",
                u32::from_le_bytes(bytes[9..13].try_into().ok()?)
            ));
        }
        return Some(text);
    }
    if length == 0 {
        return Some(
            if kind == "DATE" {
                "0000-00-00"
            } else {
                "0000-00-00 00:00:00"
            }
            .into(),
        );
    }
    if ![4, 7, 11].contains(&length) {
        return None;
    }
    let mut text = format!(
        "{:04}-{:02}-{:02}",
        u16::from_le_bytes(bytes[1..3].try_into().ok()?),
        bytes[3],
        bytes[4]
    );
    if kind != "DATE" {
        text.push_str(&format!(
            " {:02}:{:02}:{:02}",
            bytes.get(5).unwrap_or(&0),
            bytes.get(6).unwrap_or(&0),
            bytes.get(7).unwrap_or(&0)
        ));
        if length == 11 {
            text.push_str(&format!(
                ".{:06}",
                u32::from_le_bytes(bytes[8..12].try_into().ok()?)
            ));
        }
    }
    Some(text)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn select_subset_accepts_joins_aggregates_and_quoted_data() {
        for sql in [
            "SELECT * FROM `orders` WHERE `state` = 'active' ORDER BY id LIMIT 10;",
            "SELECT u.id, COUNT(o.id) FROM users u JOIN orders o ON o.user_id=u.id GROUP BY u.id",
            "SELECT 'DROP TABLE users', '' AS empty, NULL AS n",
            "SELECT a FROM t WHERE a IN (1,2)",
        ] {
            assert!(validate_select(sql).is_ok(), "{sql}");
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
                validate_select(sql),
                Err(ReadError::UnsupportedSql),
                "{sql}"
            );
        }
    }
    #[test]
    fn temporal_decoder_preserves_zero_dates_microseconds_and_extended_times() {
        assert_eq!(temporal("DATE", &[0]).as_deref(), Some("0000-00-00"));
        assert_eq!(
            temporal("DATETIME", &[0]).as_deref(),
            Some("0000-00-00 00:00:00")
        );
        assert_eq!(
            temporal(
                "DATETIME",
                &[11, 0xe8, 0x07, 2, 29, 12, 34, 56, 0x40, 0xe2, 1, 0]
            )
            .as_deref(),
            Some("2024-02-29 12:34:56.123456")
        );
        assert_eq!(
            temporal("TIME", &[8, 1, 1, 0, 0, 0, 2, 3, 4]).as_deref(),
            Some("-26:03:04")
        );
        assert!(temporal("DATE", &[11, 0]).is_none());
    }
}
