use sqlx::{
    ConnectOptions, Connection, MySqlConnection,
    mysql::{MySqlConnectOptions, MySqlSslMode},
};
use std::{num::NonZeroUsize, time::Duration};
use tokio::{io::AsyncWriteExt, net::TcpListener, sync::oneshot};

// A hostile greeting header is enough: the driver must refuse it without waiting for payload.
#[tokio::test]
async fn oversized_header_is_rejected_before_payload_or_authentication() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let (release, wait) = oneshot::channel::<()>();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        socket.write_all(&[0, 0, 2, 0]).await.unwrap(); // 131072-byte payload, deliberately never sent.
        let _ = wait.await;
    });
    let options = MySqlConnectOptions::new()
        .host("127.0.0.1")
        .port(port)
        .username("synthetic")
        .ssl_mode(MySqlSslMode::VerifyIdentity)
        .max_receive_packet_size(NonZeroUsize::new(65536).unwrap())
        .disable_statement_logging();
    let error = tokio::time::timeout(
        Duration::from_secs(2),
        MySqlConnection::connect_with(&options),
    )
    .await
    .expect("must refuse before payload arrives")
    .expect_err("oversize must fail");
    assert!(
        matches!(error,sqlx::Error::Protocol(message) if message=="receive packet exceeds configured byte limit")
    );
    release.send(()).unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn continuation_header_is_checked_against_accumulated_payload() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let (release, wait) = oneshot::channel::<()>();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        socket.write_all(&[255, 255, 255, 0]).await.unwrap();
        let chunk = vec![0; 65536];
        let mut remaining = 0xff_ffff;
        while remaining > 0 {
            let count = remaining.min(chunk.len());
            socket.write_all(&chunk[..count]).await.unwrap();
            remaining -= count;
        }
        socket.write_all(&[0, 0, 128, 1]).await.unwrap(); // another 8 MiB, exceeding 20 MiB total.
        let _ = wait.await;
    });
    let options = MySqlConnectOptions::new()
        .host("127.0.0.1")
        .port(port)
        .max_receive_packet_size(NonZeroUsize::new(20 * 1024 * 1024).unwrap())
        .disable_statement_logging();
    let error = tokio::time::timeout(
        Duration::from_secs(3),
        MySqlConnection::connect_with(&options),
    )
    .await
    .expect("must refuse second header before its payload")
    .expect_err("aggregate limit must fail");
    assert!(
        matches!(error,sqlx::Error::Protocol(message) if message=="receive packet exceeds configured byte limit")
    );
    release.send(()).unwrap();
    server.await.unwrap();
}
