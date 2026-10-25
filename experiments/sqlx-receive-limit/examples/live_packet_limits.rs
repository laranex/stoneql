//! Runs only against the owning TLS fixture created by tools/test-connection-lifecycle.mjs.
use sqlx::{
    ConnectOptions, Connection, MySqlConnection,
    mysql::{MySqlConnectOptions, MySqlSslMode},
};
use std::{env, num::NonZeroUsize};
fn certificate_error(error: &sqlx::Error) -> bool {
    match error {
        sqlx::Error::Tls(_) => true,
        sqlx::Error::Io(error) => {
            error.kind() == std::io::ErrorKind::InvalidData
                && error
                    .get_ref()
                    .is_some_and(|cause| cause.to_string().starts_with("invalid peer certificate:"))
        }
        _ => false,
    }
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let port: u16 = env::var("DBADMIN_LIFECYCLE_PORT")?.parse()?;
    let ca = std::fs::read(env::var("DBADMIN_LIFECYCLE_CA")?)?;
    let password = env::var("DBADMIN_FIXTURE_PASSWORD")?;
    let marker = env::var("DBADMIN_SPIKE_FIXTURE_ID")?;
    let options = MySqlConnectOptions::new()
        .host("127.0.0.1")
        .port(port)
        .username("dbadmin_reader")
        .password(&password)
        .database("dbadmin_m001_fixture")
        .ssl_mode(MySqlSslMode::VerifyIdentity)
        .ssl_ca_from_pem(ca)
        .max_receive_packet_size(NonZeroUsize::new(65536).unwrap())
        .disable_statement_logging();
    let mut connection = MySqlConnection::connect_with(&options)
        .await
        .map_err(|_| "verified fixture connection failed")?;
    let actual: String = sqlx::query_scalar("SELECT id FROM dbadmin_fixture_marker")
        .fetch_one(&mut connection)
        .await
        .map_err(|_| "fixture ownership probe failed")?;
    if actual != marker {
        return Err("fixture ownership mismatch".into());
    }
    let cipher = sqlx::query_as::<_, (String, String)>("SHOW SESSION STATUS LIKE 'Ssl_cipher'")
        .fetch_one(&mut connection)
        .await
        .map_err(|_| "TLS status unavailable")?
        .1;
    if cipher.is_empty() {
        return Err("fixture must use TLS".into());
    }
    let error = sqlx::query("SELECT REPEAT('x',131072)")
        .fetch_one(&mut connection)
        .await
        .err()
        .ok_or("oversized TLS row accepted")?;
    assert!(
        matches!(error,sqlx::Error::Protocol(message) if message=="receive packet exceeds configured byte limit")
    );
    connection
        .close_hard()
        .await
        .map_err(|_| "oversized connection shutdown failed")?;
    let error = MySqlConnection::connect_with(&options.clone().host("localhost"))
        .await
        .err()
        .ok_or("hostname mismatch accepted")?;
    assert!(
        certificate_error(&error),
        "hostname test must fail on a certificate, not generic connectivity"
    );
    let bad_ca = std::fs::read(env::var("DBADMIN_LIFECYCLE_BAD_CA")?)?;
    let error = MySqlConnection::connect_with(&options.clone().ssl_ca_from_pem(bad_ca))
        .await
        .err()
        .ok_or("untrusted CA accepted")?;
    assert!(
        certificate_error(&error),
        "untrusted CA test must fail on a certificate"
    );
    let mut connection = MySqlConnection::connect_with(&options)
        .await
        .map_err(|_| "fresh verified connection failed")?;
    let value: i32 = sqlx::query_scalar("SELECT 1")
        .fetch_one(&mut connection)
        .await
        .map_err(|_| "fresh query failed")?;
    assert_eq!(value, 1);
    connection.close().await.map_err(|_| "fresh close failed")?;
    println!(
        "{{\"verifiedTls\":true,\"packetLimitBytes\":65536,\"oversizedTlsRowRejected\":true,\"hostnameMismatchRejected\":true,\"untrustedCaRejected\":true,\"freshConnectionAfterRejection\":true}}"
    );
    Ok(())
}
