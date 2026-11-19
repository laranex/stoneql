# SQLx MySQL RSA key patch (third reviewed delta)

`rsa-key.patch` is applied to `vendor/sqlx-mysql` (verified by `npm run rust:vendor`). It changes
how the RSA public key for a password exchange without TLS is obtained
(`caching_sha2_password` full authentication and `sha256_password`):

- `MySqlConnectOptions::server_public_key_pem(pem)`: a trusted key, used as is; the key is never
  requested from the server.
- `MySqlConnectOptions::allow_public_key_retrieval(bool)`: whether the key may be requested from
  the server, unverified, when no trusted key is set. `true` is the upstream behavior and the
  default; `false` fails such a sign-in with the configuration error "RSA public key retrieval is
  not allowed…" before the password is sent. StoneQL always sets it explicitly.
- Trailing NUL bytes and whitespace are trimmed before the PEM is parsed: MySQL's own
  `public_key.pem` ends with a NUL byte, which the `rsa` 0.10 PEM parser rejects.
- Upstream bug fix: after an authentication switch to `caching_sha2_password`, full
  authentication used the handshake's nonce instead of the switch's, so the server rejected the
  correct password ("Access denied"). Reproduced on MySQL 8.0.46 started with
  `--default-authentication-plugin=mysql_native_password` and a `caching_sha2_password` account,
  after a restart: the unpatched driver was refused, the patched one signed in with a trusted
  key and with retrieval. Regression test: `npm run test:mysql-rsa` (fails with the original
  code: "trusted key: expected Ok(()), got Err(Unavailable)").

The driver's `rsa` feature (`sqlx/mysql-rsa`) pulls `rsa` 0.10.0, whose constant-time
arithmetic is not affected by RUSTSEC-2023-0071; `npm run audit:release` and
`audit:release:linux` pass with it.

Covered automatically by `npm run test:native-database`
(`rsa_sign_in_without_tls_after_server_restart`, after real server restarts) and
`npm run test:mysql-rsa` (MySQL 8.4.6 and 8.0.46 with an authentication switch, fresh and after a
restart that keeps the key; `--network-address` on Linux connects to the container's own
non-loopback address).
