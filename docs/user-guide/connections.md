# Connections

Use the top connection-tab **+** or Manage Connections to create a profile. MySQL, MariaDB, PostgreSQL and SQLite profiles select engine-specific mock capabilities. An `invalid` hostname simulates a connection failure.

## Profiles and groups

Create, rename and delete collapsible project groups. Move profiles with the group selector. Deleting a group preserves its connections in Ungrouped. A muted connection color appears as a dot; environment tags remain independent. Production profiles show PROD and require extra confirmation for destructive operations.

Names, groups, ordering, colors, tags and allowlisted non-secret connection settings persist locally. Record/schema changes remain session-only. Database passwords, SSH passwords, private keys and passphrases are not persisted in ordinary browser storage.

## Transport configuration

Direct and SSH Tunnel forms are configuration simulations. SSH exposes agent, private-key or password authentication and an optional passphrase. No network connection or tunnel is established. Teleport and AWS SSM are future proposals, not active transports.

[Connection management architecture](../architecture/connection-management.md) describes the proposed native lifecycle and credential boundary.
