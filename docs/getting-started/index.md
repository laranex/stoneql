# Getting started

StoneQL is in **public beta**. It comes in two forms that share one interface:

- **Desktop app (macOS):** connects live to MySQL, MariaDB and PostgreSQL servers, directly or through an SSH tunnel. There are no sample connections or Mock mode in the desktop app.
- **Browser demo:** runs the same interface in a browser with sample connections and simulated operations. Nothing in the demo touches a real database.

## Desktop app (macOS)

The beta app is ad-hoc signed and **not notarized**, so macOS Gatekeeper warns that it cannot check the app for malicious software. To open it the first time, Control-click (or right-click) StoneQL in Finder, choose **Open**, then confirm **Open**. If the warning offers no **Open** button (recent macOS versions), choose **Done**, then **Open Anyway** in System Settings → Privacy & Security. Later launches open normally. A DMG installer is being prepared. The bundle identifier is `me.thixpin.stoneql`.

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
