# M003 lifecycle hardening evidence

Local macOS ARM verification on 2026-10-25; pinned Rust 1.99.1 and Node 26.11.1. Digest-pinned engine fixtures and environment isolation follow the existing M001/M003 runner. Only generated disposable accounts, certificates and databases were used.

- `mysql.json` / `mariadb.json`: 100 verified-TLS lifecycle samples each, exact raw values, ownership, read-only/row-byte guards, wrong-password errno1045, hostname and valid-but-untrusted CA rejection, physical server-session expiry and capacity reuse.
- `expired-*.json`: fixed January 2000 leaf validity; both direct driver and runtime reject the expired certificate. Classification rejects generic I/O, unrelated certificate failures and misleading hostnames containing “expired”.
- `cleanup-*.json` / `packet-cleanup-*.json`: zero owned containers/networks and private certificate directory removed. Unknown cleanup status is represented as null by the runner, not zero.
- `packet-*.json`: existing isolated receive-limit evaluation still passes with explicit v3 CAs and corrected readiness. Root driver remains unpatched.
- Additional regressions: five isolated setup/shutdown/remote-context failure cases, socket-init/TCP-startup ordering, Docker context precedence. Both early setup and failed teardown previously leaked temporary directories; tests failed before the fixes.

Executed: 131 frontend + 14 root JavaScript tests; 26 Rust workspace tests; Rust build/format/Clippy; generated TS/boundary checks; frontend/docs builds; 28 original driver assertions; local lifecycle and packet fixtures. No new browser run: frontend source was unchanged. Historical 19 browser results remain separate.

`SOURCE_SHA256.json` identifies code/lock inputs; `SHA256.json` fingerprints these summaries. Fixture credentials, private keys, certificate material and connection strings are not retained. Debug microbenchmark timings are not release performance gates. Native live binding, driver-patch adoption, cross-OS/profile coverage and indirect view/function trust remain open.
