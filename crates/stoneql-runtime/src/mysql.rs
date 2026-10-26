//! MySQL/MariaDB protocol options, identity check and lossless cell decoding.
use super::{DirectServer, LifecycleError, ReadError, connect_error, read_only::hex};
use sqlx::{
    ConnectOptions, MySqlPool, Row, TypeInfo, ValueRef,
    mysql::{MySqlConnectOptions, MySqlPoolOptions, MySqlRow, MySqlSslMode},
};
use std::time::Duration;
use stoneql_contracts::v1::CellValue;
use stoneql_core::Engine;

pub(crate) fn options(config: &DirectServer) -> Result<MySqlConnectOptions, LifecycleError> {
    config.validate()?;
    if !matches!(config.engine, Engine::MySql | Engine::MariaDb) {
        return Err(LifecycleError::InvalidConfiguration);
    }
    let mut options = MySqlConnectOptions::new()
        .host(&config.host)
        .port(config.port)
        .username(&config.username)
        .password(&config.password)
        .database(&config.database)
        .ssl_mode(MySqlSslMode::VerifyIdentity)
        .statement_cache_capacity(0)
        .max_receive_packet_size(
            std::num::NonZeroUsize::new(2 * 1024 * 1024).expect("nonzero packet limit"),
        )
        .timezone(None)
        .pipes_as_concat(false)
        .no_engine_substitution(false)
        .disable_statement_logging();
    if let Some(pem) = &config.root_certificate_pem {
        options = options.ssl_ca_from_pem(pem.clone());
    }
    Ok(options)
}

pub(crate) async fn open(
    config: &DirectServer,
    deadline: Duration,
) -> Result<(MySqlPool, String), LifecycleError> {
    let options = options(config)?;
    let pool = tokio::time::timeout(
        deadline,
        MySqlPoolOptions::new()
            .max_connections(1)
            .min_connections(0)
            .acquire_timeout(deadline)
            .connect_with(options),
    )
    .await
    .map_err(|_| LifecycleError::Timeout)?
    .map_err(connect_error)?;
    let identity = tokio::time::timeout(
        deadline,
        sqlx::query_scalar::<_, String>("SELECT VERSION()").fetch_one(&pool),
    )
    .await;
    match identity {
        Ok(Ok(version))
            if version.to_ascii_lowercase().contains("mariadb")
                == (config.engine == Engine::MariaDb) =>
        {
            Ok((pool, version))
        }
        other => {
            pool.close().await;
            Err(match other {
                Err(_) => LifecycleError::Timeout,
                Ok(Err(_)) => LifecycleError::Unavailable,
                Ok(Ok(_)) => LifecycleError::EngineMismatch,
            })
        }
    }
}

pub(crate) fn decode_cell(row: &MySqlRow, index: usize) -> Result<CellValue, ReadError> {
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
