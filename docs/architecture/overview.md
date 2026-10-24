# Architecture overview

**Status: production architecture remains proposed.** M001 Rust core/contracts and isolated driver evaluation are implemented under explicit approval and await completion review. Tauri, Axum and production connectivity remain unimplemented; Vue mocks remain the product baseline.

## Current system

Vue components call Pinia state and session-only mock services. Pure schema validation, engine capabilities and export configuration are separate from UI components. Non-secret profile/preferences persistence is browser-local. See [object coverage](../database-objects.md) and [SQL completion status](../user-guide/sql-editor.md) for verified boundaries.

## Proposed components

```mermaid
flowchart TB
  Vue[Shared Vue frontend] --> Mock[Current mock adapter]
  Vue --> IPC[Future Tauri IPC adapter]
  Vue --> HTTP[Future HTTP adapter]
  IPC --> Core[Framework-independent Rust application core]
  HTTP --> Axum[Axum authentication and HTTP boundary]
  Axum --> Core
  Core --> DB[Engine adapters and introspection]
  Core --> Transport[Direct or SSH transport]
  Core --> Jobs[Bounded execution and transfer jobs]
  DB --> Engines[MySQL / MariaDB / PostgreSQL / SQLite]
```

Desktop targets macOS, Windows and Linux. A later standalone Axum binary serves the same built frontend on Linux. The shared core owns connection/job lifecycles, typed values, capabilities, errors and validation; it imports neither Tauri nor Axum. Host adapters authenticate/authorize and translate versioned DTOs, without duplicating domain rules.

## Decisions and alternatives

| Concern | Proposed baseline | Alternative / approval gate |
| --- | --- | --- |
| Hosting | Desktop first; one server process later | Distributed services only with measured need |
| Drivers | SQLx proposed; adopt only evidenced engine capabilities | Native-driver exceptions require a recorded capability gap and approval |
| Contracts | Rust-owned versioned wire DTOs with generated TypeScript; IPC/HTTP bindings | Generator, runtime validation and framing require approval/spikes |
| Persistence | OS credential references; separate non-secret profiles | Server vault/key management needs threat review |
| Jobs | In-process bounded queues | Durable resumable queue is a separate requirement |

Potential future paths are `crates/` for focused shared libraries, `apps/desktop/` for Tauri and `apps/server/` for Axum. They are reserved names, not scaffolded applications. Split crates when ownership and compile dependencies justify it, rather than creating one crate per type. M001 should keep the domain/application core and wire-contract generation independent of both host frameworks; generated frontend contracts cannot introduce a Tauri dependency. Database adapters and connection transports remain separate extension boundaries. The approved M001 now implements core/contracts crates and a separate experiment; it creates no native/server host. See [driver evaluation](./driver-evaluation.md).

## Failure boundaries and risks

Network loss does not establish whether a mutation committed. Cancellation is best-effort. Typed errors must distinguish rejected, rolled-back, committed and unknown outcomes. Do not automatically retry arbitrary SQL. Bound memory, open connections and queued jobs per principal. No performance targets or supported server versions have been approved yet.

[Roadmap](./roadmap.md) defines review gates, including M003 basic read-only SQL, the M003-L checkpoint unblocking SSH, M006 advanced execution, and independent M009 Desktop/M011 Server release paths. Security, integrity and compatibility are milestone acceptance criteria, not deferred hardening work. [API contracts](./api-contract.md), [integrity](./data-integrity.md) and [security](./security.md) describe shared responsibilities. [Tauri's security model](https://v2.tauri.app/security/) informs the IPC trust boundary; applying it to dbAdmin is a proposal, not a claim of existing protection.
