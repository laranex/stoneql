//! Adversarial backend headers against the patched sqlx-postgres. A fake server answers the
//! startup message with one crafted message; no real database or TLS is involved.
use sqlx::{
    ConnectOptions,
    postgres::{PgConnectOptions, PgSslMode},
};
use std::{num::NonZeroUsize, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

/// Serves `reply`, then `body` further bytes in 1 MiB chunks, then holds the socket open.
async fn serve(reply: Vec<u8>, body: usize) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut startup = [0u8; 512];
        let _ = socket.read(&mut startup).await;
        if socket.write_all(&reply).await.is_err() {
            return;
        }
        let chunk = vec![0u8; 1 << 20];
        let mut sent = 0;
        while sent < body {
            if socket.write_all(&chunk).await.is_err() {
                return;
            }
            sent += chunk.len();
        }
        tokio::time::sleep(Duration::from_secs(30)).await;
    });
    port
}
fn options(port: u16) -> PgConnectOptions {
    PgConnectOptions::new_without_pgpass()
        .host("127.0.0.1")
        .port(port)
        .username("synthetic")
        .password("synthetic")
        .database("synthetic")
        .ssl_mode(PgSslMode::Disable)
        .disable_statement_logging()
}
fn header(tag: u8, length: u32) -> Vec<u8> {
    let mut bytes = vec![tag];
    bytes.extend_from_slice(&length.to_be_bytes());
    bytes
}
/// An ErrorResponse whose length field equals `length` exactly (fields padded in the message).
fn error_response(length: u32) -> Vec<u8> {
    let fixed = b"SERROR\0VERROR\0C28P01\0M".to_vec();
    let pad = length as usize - 4 - fixed.len() - 2; // message NUL + terminator NUL
    let mut message = header(b'E', length);
    message.extend_from_slice(&fixed);
    message.extend(std::iter::repeat_n(b'x', pad));
    message.extend_from_slice(&[0, 0]);
    message
}
async fn connect(options: PgConnectOptions) -> sqlx::Error {
    tokio::time::timeout(Duration::from_secs(3), options.connect())
        .await
        .expect("must finish without waiting for the declared payload")
        .expect_err("hostile server must not yield a connection")
}
fn limit_error(error: &sqlx::Error) -> bool {
    matches!(error, sqlx::Error::Protocol(message) if message.starts_with("receive message exceeds configured byte limit"))
}

#[tokio::test]
async fn length_shorter_than_its_own_field_is_a_protocol_error_not_a_panic() {
    for length in 0..4 {
        let port = serve(header(b'R', length), 0).await;
        // Upstream 0.9.0 panics here ("cannot advance past `remaining`"); no limit is configured.
        let error = tokio::spawn(connect(options(port))).await.expect("no panic");
        assert!(
            matches!(&error, sqlx::Error::Protocol(message) if message.starts_with("invalid backend message length")),
            "{error:?}"
        );
    }
}
#[tokio::test]
async fn oversized_header_is_refused_before_its_payload_is_buffered() {
    let port = serve(header(b'R', 1 << 30), 64 << 20).await;
    let limit = NonZeroUsize::new(2 << 20).unwrap();
    let error = connect(options(port).max_receive_message_size(limit)).await;
    assert!(limit_error(&error), "{error:?}");
}
#[tokio::test]
async fn exact_limit_is_read_and_one_byte_more_is_refused() {
    let limit = 4096;
    let port = serve(error_response(limit), 0).await;
    let error = connect(options(port).max_receive_message_size(NonZeroUsize::new(limit as usize).unwrap())).await;
    // Read and parsed: the server's ErrorResponse surfaces as a database error.
    assert!(matches!(&error, sqlx::Error::Database(e) if e.code().as_deref() == Some("28P01")), "{error:?}");
    let port = serve(error_response(limit + 1), 0).await;
    let error = connect(options(port).max_receive_message_size(NonZeroUsize::new(limit as usize).unwrap())).await;
    assert!(limit_error(&error), "{error:?}");
}
#[tokio::test]
async fn default_keeps_upstream_unbounded_behavior() {
    // Without the option the declared 1 GiB is still awaited (documents that the bound is opt-in).
    let port = serve(header(b'R', 1 << 30), 0).await;
    let waited = tokio::time::timeout(Duration::from_millis(500), options(port).connect()).await;
    assert!(waited.is_err(), "upstream default waits for the declared payload");
}
