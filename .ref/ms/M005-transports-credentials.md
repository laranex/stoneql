# M005 — SSH and secure credentials

## Status
Planned / not started. The user authorized roadmap continuation and local milestone commits. Prerequisites, unresolved technical decisions and security/release acceptance gates still apply; no push, publication or deployment is authorized.

## Objective and scope
Desktop secret references and Direct/SSH transport integration; allow work as soon as connection ownership/cleanup is stable. Do not implement Teleport, SSM or a server vault here.

## Dependencies and prerequisites
Accepted M003-L lifecycle checkpoint and M001 contracts; approve D005 desktop provider and D006 trust/agent policy. **M004, full M003 SQL/UI completion, M008 and server-vault selection are not prerequisites.**

## Technical approach
Compose verified SSH agent/key/password transports with the existing database lifecycle. Preserve original database identity for TLS through forwarding. Define secret provider and endpoint/cleanup boundaries without host-framework imports in shared crates.

## Deliverables
Secure desktop provider binding, transport lifecycle and MySQL/MariaDB-over-SSH evidence; later engine parity added when those adapters are ready.

## Acceptance criteria
- G1: unknown/changed host keys fail or require explicit verified enrollment; certificates remain verified; no secrets in profile/browser/logs/arguments or unintended agent forwarding. Private keys/passphrases stay within approved host/secret boundaries.
- G2: forwarded endpoint never changes database/profile identity or raw values; credential/session isolation persists; interrupted setup never leaves a usable misleading connection handle.
- G3: approved desktop OS/auth-method matrix passes with MySQL/MariaDB; unsupported engine/transport pairs are rejected. PostgreSQL support is a later parity test, not a start gate; SQLite has no implied network tunnel.
- G4/G5: 100 setup/connect/disconnect/failure cycles with zero orphan tunnels/handles within the approved cleanup bound; measure SSH overhead separately from the Direct baseline and approve a transport budget before completion.
- Future Teleport/SSM variants remain unimplemented, with owned endpoint/expiry interfaces available for later review.

## Required tests and verification
Host-key rejection/rotation; failed auth, agent unavailable, passphrase expiry, TLS mismatch through tunnel, cancellation of setup, crash/disconnect cleanup and credential isolation; lifecycle measurements per supported OS/auth path.

## Risks and limitations
Agent/keyring differences and host-key enrollment require human policy decisions. Server credential isolation is owned by M008/M011; it must not delay desktop SSH.


Public context: `docs/architecture/roadmap.md` (G1–G6 and proposed measurement profile). Approval records: `.ref/planning/architecture-decisions.md`. No acceptance criterion is satisfied by this planning edit; record executed evidence before marking complete.
