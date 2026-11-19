//! Disposable fixture harness only; never opens arbitrary URLs. All data belongs to this run.
use sqlx::{
    ConnectOptions, Connection, MySqlConnection,
    mysql::{MySqlConnectOptions, MySqlSslMode},
};
use std::{
    env,
    time::{Duration, Instant},
};
use stoneql_contracts::v1::CellValue;
use stoneql_core::Engine;
use stoneql_runtime::{Connections, DirectServer, LifecycleError, OwnerId, QueryLimits, ReadError};
fn config(engine: Engine, port: u16, password: String, ca: Vec<u8>) -> DirectServer {
    DirectServer {
        engine,
        host: "127.0.0.1".into(),
        port,
        username: "stoneql_reader".into(),
        password,
        database: "stoneql_m001_fixture".into(),
        root_certificate_pem: Some(ca),
        tls: true,
        rsa_key: stoneql_runtime::PasswordRsaKey::None,
    }
}
fn certificate_failure(error: &sqlx::Error) -> Option<String> {
    let reason = match error {
        sqlx::Error::Tls(cause) => cause.to_string(),
        sqlx::Error::Io(cause) if cause.kind() == std::io::ErrorKind::InvalidData => {
            cause.get_ref().map(ToString::to_string).unwrap_or_default()
        }
        _ => return None,
    }
    .to_ascii_lowercase();
    reason
        .starts_with("invalid peer certificate:")
        .then_some(reason)
}
fn expired_certificate(error: &sqlx::Error) -> bool {
    certificate_failure(error).is_some_and(|reason| {
        reason.starts_with("invalid peer certificate: certificate expired")
            || reason.starts_with("invalid peer certificate: expired")
    })
}
async fn reader_count(connection: &mut MySqlConnection) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM information_schema.PROCESSLIST WHERE USER = 'stoneql_reader'",
    )
    .fetch_one(connection)
    .await
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let engine = match env::var("STONEQL_LIFECYCLE_ENGINE")?.as_str() {
        "mysql" => Engine::MySql,
        "mariadb" => Engine::MariaDb,
        _ => return Err("fixture engine required".into()),
    };
    let port: u16 = env::var("STONEQL_LIFECYCLE_PORT")?.parse()?;
    let password = env::var("STONEQL_FIXTURE_PASSWORD")?;
    let marker = env::var("STONEQL_SPIKE_FIXTURE_ID")?;
    if marker.len() != 36 {
        return Err("fixture marker required".into());
    }
    let ca = std::fs::read(env::var("STONEQL_LIFECYCLE_CA")?)?;
    let bad_ca = std::fs::read(env::var("STONEQL_LIFECYCLE_BAD_CA")?)?;
    // Confirm ownership with verified TLS and the restricted fixture account before evaluation.
    let options = MySqlConnectOptions::new()
        .host("127.0.0.1")
        .port(port)
        .username("stoneql_reader")
        .password(&password)
        .database("stoneql_m001_fixture")
        .ssl_mode(MySqlSslMode::VerifyIdentity)
        .ssl_ca_from_pem(ca.clone())
        .disable_statement_logging();
    let arguments: Vec<_> = env::args().skip(1).collect();
    if arguments == ["--expired-certificate"] {
        let error = MySqlConnection::connect_with(&options)
            .await
            .expect_err("expired fixture must reject TLS");
        if !expired_certificate(&error) {
            return Err(
                "expected an expired-certificate error, not a generic connection failure".into(),
            );
        }
        let registry = Connections::new(1, Duration::from_secs(3), Duration::from_secs(60))?;
        assert!(
            registry
                .connect(OwnerId::new(), config(engine, port, password, ca))
                .await
                .is_err()
        );
        registry.shutdown().await;
        println!("{{\"expiredCertificateRejected\":true,\"runtimeRejected\":true}}");
        return Ok(());
    }
    if !arguments.is_empty() {
        return Err("unknown fixture mode".into());
    }
    let mut probe = MySqlConnection::connect_with(&options)
        .await
        .map_err(|error| match error {
            sqlx::Error::Io(cause) => {
                format!("fixture TLS connection failed: IO {:?}", cause.kind())
            }
            sqlx::Error::Tls(_) => "fixture TLS connection failed: TLS".into(),
            _ => "fixture TLS connection failed: driver".into(),
        })?;
    let actual: String = sqlx::query_scalar("SELECT id FROM stoneql_fixture_marker")
        .fetch_one(&mut probe)
        .await
        .map_err(|_| "fixture marker query failed")?;
    if actual != marker {
        return Err("fixture ownership mismatch".into());
    }
    let error = MySqlConnection::connect_with(&options.clone().host("localhost"))
        .await
        .expect_err("hostname mismatch must fail");
    let reason =
        certificate_failure(&error).ok_or("hostname failure was not a certificate rejection")?;
    if !reason.contains("not valid for name") && !reason.contains("notvalidforname") {
        return Err("expected a hostname certificate failure".into());
    }
    let error = MySqlConnection::connect_with(&options.clone().ssl_ca_from_pem(bad_ca.clone()))
        .await
        .expect_err("untrusted CA must fail");
    let reason = certificate_failure(&error).ok_or("CA failure was not a certificate rejection")?;
    if !reason.contains("unknownissuer") && !reason.contains("unknown issuer") {
        return Err("expected an untrusted issuer failure".into());
    }
    let error = MySqlConnection::connect_with(&options.clone().password("wrong_fixture_password"))
        .await
        .expect_err("wrong password must fail");
    assert_eq!(
        error
            .as_database_error()
            .and_then(|error| error.try_downcast_ref::<sqlx::mysql::MySqlDatabaseError>())
            .map(|error| error.number()),
        Some(1045)
    );
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
    let mut writer = MySqlConnection::connect_with(&options.clone().username("stoneql_writer"))
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
    let mut observer = MySqlConnection::connect_with(&options)
        .await
        .map_err(|_| "cleanup observer failed")?;
    tokio::time::timeout(Duration::from_secs(3), async {
        while reader_count(&mut observer).await? != 1 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        Ok::<_, sqlx::Error>(())
    })
    .await
    .map_err(|_| "initial server connections were not cleaned up")?
    .map_err(|_| "cleanup baseline failed")?;
    let baseline = 1;
    let expiring = Connections::new(1, Duration::from_secs(3), Duration::from_secs(1))?;
    let expiry_owner = OwnerId::new();
    let expiry_id = expiring
        .connect(
            expiry_owner,
            config(engine, port, password.clone(), ca.clone()),
        )
        .await?;
    assert_eq!(
        reader_count(&mut observer)
            .await
            .map_err(|_| "active connection inspection failed")?,
        baseline + 1
    );
    tokio::time::timeout(Duration::from_secs(3), async {
        while expiring.is_connected(expiry_owner, expiry_id)
            || reader_count(&mut observer).await? != baseline
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        Ok::<_, sqlx::Error>(())
    })
    .await
    .map_err(|_| "expired server connection was not cleaned up")?
    .map_err(|_| "expiry inspection failed")?;
    let reopened = expiring
        .connect(
            expiry_owner,
            config(engine, port, password.clone(), ca.clone()),
        )
        .await?;
    expiring.disconnect(expiry_owner, reopened).await?;
    expiring.shutdown().await;
    observer
        .close()
        .await
        .map_err(|_| "observer shutdown failed")?;
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
    let packet_id = registry
        .connect(owner, config(engine, port, password.clone(), ca.clone()))
        .await?;
    assert_eq!(
        registry
            .read_only(owner, packet_id, "SELECT payload FROM raw_large", limits)
            .await,
        Err(ReadError::PacketTooLarge)
    );
    let recovered = registry
        .read_only(
            owner,
            packet_id,
            "SELECT id FROM raw_values ORDER BY id",
            limits,
        )
        .await?;
    assert_eq!(recovered.rows.len(), 2);
    registry.disconnect(owner, packet_id).await?;
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
        "{{\"verifiedTls\":true,\"runtimePacketLimitBytes\":2097152,\"oversizedPacketRejected\":true,\"freshConnectionRecovery\":true,\"serverExpiryCleanup\":true,\"hostnameMismatchRejected\":true,\"certificateReasonsVerified\":true,\"readOnlyGuards\":true,\"losslessRawValues\":true,\"boundedResults\":true,\"ownerIsolation\":true,\"wrongPasswordRejected\":true,\"untrustedCaRejected\":true,\"engineMismatchRejected\":true,\"capacityAndShutdown\":true,\"lifecycleSamples\":100,\"p50Us\":{},\"p95Us\":{},\"maxUs\":{}}}",
        samples[50], samples[95], samples[99]
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn expiration_probe_rejects_generic_connection_and_unrelated_tls_failures() {
        use std::io::{Error, ErrorKind};
        for cause in [
            "socket expired",
            "invalid peer certificate: UnknownIssuer",
            "invalid peer certificate: certificate not valid for name expired.example",
        ] {
            assert!(!expired_certificate(&sqlx::Error::Io(Error::new(
                ErrorKind::InvalidData,
                cause
            ))));
        }
        assert!(!expired_certificate(&sqlx::Error::Io(Error::from(
            ErrorKind::ConnectionRefused
        ))));
        assert!(expired_certificate(&sqlx::Error::Io(Error::new(
            ErrorKind::InvalidData,
            "invalid peer certificate: certificate expired: verification time is later"
        ))));
    }
}
