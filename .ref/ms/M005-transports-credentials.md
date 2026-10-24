# M005 — SSH and secure credentials

## Status
Proposed / awaiting approval. No backend implementation authorized by this document.

## Objective and scope
SSH and secure credentials. Preserve existing Vue behavior and use existing abstractions; no unrelated redesign.

## Dependencies and prerequisites
M003/M004; D005/D006 approval.

## Technical approach
Integrate OS secret references, verified SSH host keys/agent/key/password flows, tunnel-owned lifecycles and cleanup.

## Deliverables
Secret provider abstraction and secure Direct/SSH workflows.

## Acceptance criteria
No secrets in profile/log/browser persistence; host-key/TLS failures explicit; cancelled tunnels cleaned.

## Required tests and verification
Credential isolation; rejected host keys; expiry; passphrases; crash/disconnect cleanup per OS.

## Risks and limitations
Server vault is separate gate; Teleport/SSM remain deferred.

Public context: `docs/architecture/roadmap.md`. Approval records: `.ref/planning/architecture-decisions.md`. Mark complete only after executing and recording all acceptance checks.
