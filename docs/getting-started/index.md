# Getting started

StoneQL is in **public beta**. It comes in two forms that share one interface:

- **Desktop app (macOS):** connects live to MySQL, MariaDB and PostgreSQL servers, directly or through an SSH tunnel. There are no sample connections or Mock mode in the desktop app.
- **Browser demo:** runs the same interface in a browser with sample connections and simulated operations. Nothing in the demo touches a real database.

## Desktop app (macOS)

### Install from the DMG

1. Open `StoneQL-<version>.dmg`. A window shows StoneQL on the left and an **Applications** shortcut on the right.
2. Drag **StoneQL** onto **Applications**, then close the window and eject the StoneQL disk.
3. Open StoneQL from your Applications folder (not from the DMG).

The bundle identifier is `me.thixpin.stoneql`. Installing a newer build over the old one keeps your saved connections, groups, theme and grid preferences.

### First launch of the beta build

The beta app is ad-hoc signed and **not notarized**, so when it was downloaded, macOS Gatekeeper cannot confirm it is free of malware and blocks the first launch. Choose **Done**, then open System Settings → Privacy & Security, find the message about StoneQL and choose **Open Anyway**, and confirm. Later launches open normally. A build you compiled yourself is not quarantined and opens without this step.

### Saved passwords and the Keychain prompt

When **Remember database password in the system keychain** is ticked, StoneQL stores the password in your macOS login Keychain (never in its own files). macOS controls which apps may read that item:

- The first time a build reads a password that a different build saved, macOS asks for your **login** password, once for the item's access list ("…wants to use your confidential information…") and once for its key ("…wants to access key…"). Both refer to the same saved password.
- **Always Allow** lets that build read the item without asking again; **Allow** permits this one read only; **Deny** refuses it, and StoneQL reports that the saved credential is unavailable (nothing is retried).
- Each beta build has its own code identity, so after you install a new build macOS asks again. If prompts keep returning, edit the connection, type the database password again with **Remember** ticked and save: the item is then owned by the installed build and reconnects silently.

On first launch StoneQL opens on a start screen with the Light theme (a theme you choose later is remembered). Create a connection with **+** or Manage Connections; see [Connections](../user-guide/connections.md). Connections are read-write unless you tick **Read-only connection**, so start with a least-privilege account or a read-only connection when you are exploring a server that matters.

SQLite is not included in release builds (development builds only).

## Browser demo and development

Use Node.js **26** and npm from the repository root:

```sh
npm ci
npm run dev
```

Open the URL printed by Vite. No database server, Rust toolchain or credentials are required. Sample connections open realistic relational mock databases. Changes to records and schema last only for the session.

To build and run the desktop app from source, see the [desktop development host](../development/desktop-host.md).

## Commands

| Command | Purpose |
| --- | --- |
| `npm run dev` | Run the shared Vue frontend (browser demo) |
| `npm run build` | Type-check and bundle the frontend |
| `npm run preview` | Serve the frontend production bundle |
| `npm test` | Run frontend and workspace regression tests |
| `npm run test:browser` | Run the Playwright browser scenarios |
| `npm run docs:dev` | Run this documentation site |
| `npm run docs:build` | Check links and build documentation |
| `npm run docs:preview` | Preview built documentation |

Install Playwright Chromium with `npx playwright install chromium` when required. Browser execution needs a host that permits local listeners and sandboxed browser processes. See [verification](../development/verification.md) for current results.

## Beta boundaries

In the **browser demo**, connection testing, SSH setup, SQL execution, object management and transfers are simulations. A success notification there does not mean a real database changed or a file was created. Never enter real credentials into the demo.

In the **desktop app**, everything is live. Features not yet available for live connections are hidden rather than simulated; the [workspace guide](../user-guide/index.md#not-yet-available-in-the-desktop-app) lists them. StoneQL is beta software: keep backups and verify important changes on the server.

Continue with the [workspace guide](../user-guide/index.md).
