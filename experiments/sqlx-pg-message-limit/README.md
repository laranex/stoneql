# sqlx-postgres message-limit evaluation

Isolated, unadopted prototype. Findings, results and the decision request:
`.ref/planning/m004-pg-message-limit/findings.md`. `driver/` is regenerated from the registry
sqlx-postgres 0.9.0 source plus `message-limit.patch` by `prepare.mjs` and is never committed.
The runtime and `vendor/` are unchanged. SQLx is MIT licensed (`SQLX-LICENSE-MIT`).
