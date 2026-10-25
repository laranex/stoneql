//! Disposable fixture harness only; never opens arbitrary URLs. All data belongs to this run.
use dbadmin_contracts::v1::CellValue;
use dbadmin_core::Engine;
use dbadmin_runtime::{Connections, DirectMySql, LifecycleError, OwnerId, QueryLimits, ReadError};
use sqlx::{
    ConnectOptions, Connection, MySqlConnection,
    mysql::{MySqlConnectOptions, MySqlSslMode},
};
use std::{
    env,
    time::{Duration, Instant},
};
fn config(engine: Engine, port: u16, password: String, ca: Vec<u8>) -> DirectMySql {
    DirectMySql {
        engine,
        host: "127.0.0.1".into(),
        port,
        username: "dbadmin_reader".into(),
        password,
        database: "dbadmin_m001_fixture".into(),
        root_certificate_pem: Some(ca),
    }
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let engine = match env::var("DBADMIN_LIFECYCLE_ENGINE")?.as_str() {
        "mysql" => Engine::MySql,
        "mariadb" => Engine::MariaDb,
        _ => return Err("fixture engine required".into()),
    };
    let port: u16 = env::var("DBADMIN_LIFECYCLE_PORT")?.parse()?;
    let password = env::var("DBADMIN_FIXTURE_PASSWORD")?;
    let marker = env::var("DBADMIN_SPIKE_FIXTURE_ID")?;
    if marker.len() != 36 {
        return Err("fixture marker required".into());
    }
    let ca = std::fs::read(env::var("DBADMIN_LIFECYCLE_CA")?)?;
    let bad_ca = std::fs::read(env::var("DBADMIN_LIFECYCLE_BAD_CA")?)?;
    // Confirm ownership with verified TLS and the restricted fixture account before evaluation.
    let options = MySqlConnectOptions::new()
        .host("127.0.0.1")
        .port(port)
        .username("dbadmin_reader")
        .password(&password)
        .database("dbadmin_m001_fixture")
        .ssl_mode(MySqlSslMode::VerifyIdentity)
        .ssl_ca_from_pem(ca.clone())
        .disable_statement_logging();
    let mut probe = MySqlConnection::connect_with(&options)
        .await
        .map_err(|_| "fixture TLS connection failed")?;
    let actual: String = sqlx::query_scalar("SELECT id FROM dbadmin_fixture_marker")
        .fetch_one(&mut probe)
        .await
        .map_err(|_| "fixture marker query failed")?;
    if actual != marker {
        return Err("fixture ownership mismatch".into());
    }
    let cipher: String =
        sqlx::query_as::<_, (String, String)>("SHOW SESSION STATUS LIKE 'Ssl_cipher'")
            .fetch_one(&mut probe)
            .await
            .map_err(|_| "TLS inspection failed")?
            .1;
    if cipher.is_empty() {
        return Err("fixture connection must be encrypted".into());
    }
    // Restricted-account permission boundary is tested independently from the frontend policy.
    let denied = sqlx::query("DELETE FROM raw_values WHERE id=1")
        .execute(&mut probe)
        .await
        .unwrap_err();
    assert_eq!(
        denied
            .as_database_error()
            .and_then(|error| error.try_downcast_ref::<sqlx::mysql::MySqlDatabaseError>())
            .map(|error| error.number()),
        Some(1142)
    );
    probe.close().await.map_err(|_| "fixture close failed")?;
    let mut writer = MySqlConnection::connect_with(&options.clone().username("dbadmin_writer"))
        .await
        .map_err(|_| "writer fixture failed")?;
    sqlx::raw_sql("START TRANSACTION READ ONLY")
        .execute(&mut writer)
        .await
        .map_err(|error| {
            format!(
                "read-only guard failed: {}",
                error
                    .as_database_error()
                    .and_then(|error| error.try_downcast_ref::<sqlx::mysql::MySqlDatabaseError>())
                    .map(|error| error.number())
                    .unwrap_or(0)
            )
        })?;
    let denied = sqlx::query("DELETE FROM raw_values WHERE id=1")
        .execute(&mut writer)
        .await
        .unwrap_err();
    assert_eq!(
        denied
            .as_database_error()
            .and_then(|error| error.try_downcast_ref::<sqlx::mysql::MySqlDatabaseError>())
            .map(|error| error.number()),
        Some(1792)
    );
    writer.close().await.map_err(|_| "writer close failed")?;
    let registry = Connections::new(2, Duration::from_secs(3), Duration::from_secs(60))?;
    let owner = OwnerId::new();
    let other = OwnerId::new();
    let limits = QueryLimits {
        rows: 100,
        bytes: 1_048_576,
        deadline: Duration::from_secs(3),
    };
    let id = registry
        .connect(owner, config(engine, port, password.clone(), ca.clone()))
        .await?;
    let result = registry
        .read_only(owner, id, "SELECT * FROM raw_values ORDER BY id", limits)
        .await?;
    assert_eq!(result.rows.len(), 2);
    let cells = &result.rows[0];
    assert_eq!(
        cells[1],
        CellValue::Integer {
            text: "-9223372036854775808".into()
        }
    );
    assert_eq!(
        cells[2],
        CellValue::Integer {
            text: "18446744073709551615".into()
        }
    );
    assert_eq!(
        cells[3],
        CellValue::Decimal {
            text: "123456789012345678901234567890.4500".into()
        }
    );
    assert_eq!(
        cells[4],
        CellValue::Binary {
            hex: "00ff0180".into()
        }
    );
    assert_eq!(
        cells[5],
        CellValue::Temporal {
            database_type: "DATETIME".into(),
            text: "2024-02-29 12:34:56.123456".into()
        }
    );
    assert_eq!(
        cells[6],
        CellValue::Temporal {
            database_type: "DATE".into(),
            text: "0000-00-00".into()
        }
    );
    assert_eq!(
        cells[7],
        CellValue::Temporal {
            database_type: "DATETIME".into(),
            text: "0000-00-00 00:00:00".into()
        }
    );
    assert_eq!(
        cells[8],
        CellValue::Temporal {
            database_type: "TIME".into(),
            text: "-26:03:04.123456".into()
        }
    );
    assert_eq!(
        cells[9],
        CellValue::Text {
            text: "active".into()
        }
    );
    assert_eq!(cells[10], CellValue::Text { text: "".into() });
    assert_eq!(cells[11], CellValue::Null);
    let bounded = registry
        .read_only(
            owner,
            id,
            "SELECT * FROM raw_values",
            QueryLimits { rows: 1, ..limits },
        )
        .await?;
    assert!(bounded.truncated);
    assert_eq!(bounded.rows.len(), 1);
    let bounded = registry
        .read_only(
            owner,
            id,
            "SELECT txt FROM raw_values",
            QueryLimits {
                bytes: 170,
                ..limits
            },
        )
        .await?;
    assert!(bounded.truncated);
    let empty = registry
        .read_only(owner, id, "SELECT * FROM raw_values WHERE id=999", limits)
        .await?;
    assert!(empty.rows.is_empty());
    assert_eq!(empty.columns.len(), 12);
    assert_eq!(
        registry
            .read_only(other, id, "SELECT * FROM raw_values", limits)
            .await,
        Err(ReadError::Connection(LifecycleError::Forbidden))
    );
    for sql in [
        "DELETE FROM raw_values",
        "SELECT 1; DELETE FROM raw_values",
        "SELECT GET_LOCK('x',1)",
        "SELECT * FROM raw_values FOR UPDATE",
        "SELECT 1 INTO OUTFILE '/tmp/never-created'",
    ] {
        assert_eq!(
            registry.read_only(owner, id, sql, limits).await,
            Err(ReadError::UnsupportedSql)
        );
    }
    registry.disconnect(owner, id).await?;
    let mut samples = Vec::new();
    for _ in 0..100 {
        let start = Instant::now();
        let id = registry
            .connect(owner, config(engine, port, password.clone(), ca.clone()))
            .await?;
        assert!(registry.is_connected(owner, id));
        assert!(!registry.is_connected(other, id));
        assert_eq!(
            registry.disconnect(other, id).await,
            Err(LifecycleError::Forbidden)
        );
        registry.disconnect(owner, id).await?;
        assert!(!registry.is_connected(owner, id));
        samples.push(start.elapsed().as_micros() as u64);
    }
    assert!(
        registry
            .connect(
                owner,
                config(engine, port, "wrong_fixture_password".into(), ca.clone())
            )
            .await
            .is_err()
    );
    assert!(
        registry
            .connect(owner, config(engine, port, password.clone(), bad_ca))
            .await
            .is_err()
    );
    let mismatched = if engine == Engine::MySql {
        Engine::MariaDb
    } else {
        Engine::MySql
    };
    assert_eq!(
        registry
            .connect(
                owner,
                config(mismatched, port, password.clone(), ca.clone())
            )
            .await,
        Err(LifecycleError::EngineMismatch)
    );
    let one = registry
        .connect(owner, config(engine, port, password.clone(), ca.clone()))
        .await?;
    let two = registry
        .connect(other, config(engine, port, password.clone(), ca.clone()))
        .await?;
    assert_eq!(
        registry
            .connect(owner, config(engine, port, password, ca))
            .await,
        Err(LifecycleError::Capacity)
    );
    registry.shutdown().await;
    assert!(!registry.is_connected(owner, one));
    assert!(!registry.is_connected(other, two));
    samples.sort_unstable();
    println!(
        "{{\"verifiedTls\":true,\"readOnlyGuards\":true,\"losslessRawValues\":true,\"boundedResults\":true,\"ownerIsolation\":true,\"wrongPasswordRejected\":true,\"untrustedCaRejected\":true,\"engineMismatchRejected\":true,\"capacityAndShutdown\":true,\"lifecycleSamples\":100,\"p50Us\":{},\"p95Us\":{},\"maxUs\":{}}}",
        samples[50], samples[95], samples[99]
    );
    Ok(())
}
