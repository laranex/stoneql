# M003 native read-only binding evidence

Local macOS ARM verification on 2026-10-25, Node 26.11.1 and pinned Rust 1.99.1. This is an incremental acceptance record, not M003/release completion.

- `npm test`: 134 frontend + 17 repository tests passed (151 total).
- `npm run test:browser`: 21 scenarios passed, Chromium sandbox enabled. Original 19 preserved; two added live-bridge simulations cover Light/Dark connection dialogs, CA/password non-persistence, independent workspaces, SQL execution/refusal/truncation, reload and 800×600 action visibility. Screenshots visually inspected for Light results and Dark connection configuration; screenshots remain generated local artifacts.
- `npm run rust:test`: 30 passed; one owning-fixture integration test deliberately ignored by the default suite and separately executed for each engine below.
- `npm run test:native-database`: actual Tauri command handlers under MockRuntime against disposable MySQL 8.4.6 and MariaDB 11.4.8 verified-TLS fixtures. Both passed. JSON summaries contain synthetic fixture assertions only. Both cleanup reports confirm zero owned containers/networks and removed private temporary directories.
- Rust build, formatting, all-target Clippy, generated-contract check, core/contracts dependency boundaries and 76-file SQLx provenance check passed. The boundary command was rerun with the explicit toolchain PATH after an initial cargo-not-found environment failure.
- Frontend and native custom-protocol builds passed. Documentation source/navigation/output checks and production build passed. Existing frontend chunk-size advisory remains nonfatal.

The browser bridge is simulated; the database fixture uses real SQLx and native handlers but no graphical WebView. Real macOS Wry startup smoke passed separately in 1,597 ms with the final embedded frontend. No native GUI-to-real-database end-to-end, Windows/Linux, signed distribution, aggregate metadata/allocation bound or indirect view/function safety acceptance is claimed. Renderer reload does not yet reconcile previous handles; leases expire or close on application shutdown. Existing two Linux GTK dependency audit findings remain open; no new audit pass is claimed here.
