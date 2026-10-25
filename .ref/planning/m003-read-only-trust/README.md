# M003 indirect read-only trust evidence

2026-10-25, local macOS ARM, pinned Rust1.99.1 and Node26.11.1. The existing real native-handler fixture now includes definer and nested views backed by writing stored functions. No production runtime, public API or UI behavior changes.

Both MySQL8.4.6 and MariaDB11.4.8 passed `npm run test:native-database` after restoring the original runtime guard. Positive controls demonstrate effects really occur outside the guard with the SELECT-only reader. Independent raw-driver checks observe engine1792; actual native commands report fixed QUERY_REJECTED/unknown and fresh safe queries succeed. Persistent InnoDB/MyISAM counters remain zero after native calls. Indirect advisory locking is possible but released when the query's physical connection closes.

A deliberate local mutation omitted only START TRANSACTION READ ONLY. The native regression failed because the indirect writing query succeeded. The original runtime source was restored in finally, verified unchanged by Git diff, and both real fixtures rerun successfully. No mutation implementation is committed. Cleanup reports confirm zero owned containers/networks and removed private temporary directories, including the failed mutation run.

Safe report JSON contains only fixture booleans/counts. Original evidence remains unchanged. Full native GUI/database interaction, cross-OS runs, aggregate metadata/memory bounds, renderer-reload lease reconciliation and hostile UDF/external side effects remain unverified. M003 is not complete. See public `docs/development/read-only-safety.md` for the supported boundary and reproduction.

Final local checks:154 JavaScript tests (137 frontend+17 repository),30 default Rust tests, all-target Clippy, rustfmt and docs production/source/navigation/output verification passed. Native owning-fixture test executed separately on both engines. No Vue code changed and no new browser/theme verification claimed; prior22 passing scenarios remain historical evidence. CI workflow has not been remotely run for this increment; the native fixture remains an explicit opt-in command, not a default Rust test pass.
