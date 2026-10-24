# Deployment proposal

**Status: Proposed / awaiting approval.** No Rust applications, packaging, infrastructure or production deployment are created here.

## Desktop

Tauri 2 should package the shared frontend and IPC adapter for macOS, Windows and Linux. Build/sign/notarize on supported native runners, test native credential stores and SSH agents, and constrain updater trust. Browser mode remains independent and uses mocks until a reviewed host adapter is selected. Offline desktop operation must not depend on the documentation site's Mermaid CDN.

## Future Linux server

Start with one standalone Axum binary serving the built Vue frontend behind a TLS reverse proxy. Use systemd, a dedicated service identity, scoped state/runtime directories and read-only application files. Configure resource ceilings, graceful shutdown, restart backoff and health checks. Where compatible, use systemd hardening such as ProtectSystem and NoNewPrivileges; verify required database networking and SQLite file access rather than copying an untested unit.

Secrets should arrive through the approved vault or restricted credential mechanism, never checked-in unit Environment values. Back up non-secret profiles and encrypted vault metadata separately from keys. Define supported upgrade/rollback and state migrations before release. No HA, distributed queue or Kubernetes is justified by current requirements; revisit only after concrete availability/load targets.

## Optional Docker

Package the same server binary with a non-root identity, minimal runtime, read-only root filesystem, dropped capabilities and explicit state/temp mounts. Do not mount the Docker socket or host credential stores. Networking and file policies still apply. Containerization does not replace authentication or secret isolation. Native systemd deployment remains supported.

## Documentation hosting

The VitePress workflow is prepared separately. It builds pull requests and deploys only on explicit manual dispatch with deployment selected. Repository subpaths and custom domains are configurable. It has not been published. See [website maintenance](../development/documentation.md).

## Release gates and unknowns

Define OS/engine support versions, signing identities, update channels, OIDC deployment, TLS ownership, resource budgets, backup/restore and incident response. Run signed native smoke tests on all three desktop OSes and systemd/container integration tests before claiming deployability. None of these production checks are part of the browser prototype's passing unit suite.
