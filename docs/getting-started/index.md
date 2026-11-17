# Getting started

StoneQL is in **public beta**. It comes in two forms that share one interface:

- **Desktop app (macOS on Apple Silicon):** connects live to MySQL, MariaDB and PostgreSQL servers, directly or through an SSH tunnel. There are no sample connections or Mock mode in the desktop app.
- **Browser demo:** runs the same interface in a browser with sample connections and simulated operations. Nothing in the demo touches a real database.

## Desktop app (macOS)

### Download

Download the current beta, **[v0.1.0-beta.1](https://github.com/stoneAce/stoneql/releases/tag/v0.1.0-beta.1)** (a GitHub pre-release; later builds appear on the [releases page](https://github.com/stoneAce/stoneql/releases)):

- `StoneQL-0.1.0-beta-970ade2.dmg`: macOS on **Apple Silicon (arm64) only**. Intel Macs, Windows and Linux builds are not released.
- `StoneQL-0.1.0-beta-970ade2.dmg.sha256`: its SHA-256 checksum.

The beta was verified on macOS 27.0.1 only; older macOS versions are untested and not claimed as supported. It is ad-hoc signed with Hardened Runtime and **not notarized** (no Developer ID signature yet).

Put both files in the same folder and verify the download in Terminal before opening it:

```sh
shasum -a 256 -c StoneQL-0.1.0-beta-970ade2.dmg.sha256
```

It must print `StoneQL-0.1.0-beta-970ade2.dmg: OK`. The expected SHA-256 is `3da858b6cd3c5819bd7d44ae5674cf257a28dfef58bdb6f0b7731c0ea9467a1a`. Do not open a file that fails the check.

### Install from the DMG

1. Open `StoneQL-0.1.0-beta-970ade2.dmg`. A window shows StoneQL on the left and an **Applications** shortcut on the right.
2. Drag **StoneQL** onto **Applications**, then close the window and eject the StoneQL disk.
3. Open StoneQL from your Applications folder (not from the DMG).

The bundle identifier is `me.thixpin.stoneql`.

### First launch of the downloaded beta

Because the beta is not notarized, macOS Gatekeeper blocks the first launch of a downloaded copy with **“StoneQL” Not Opened** (“Apple could not verify “StoneQL” is free of malware…”). The dialog offers only **Move to Trash** and **Done**; there is no Open button. To open it anyway:

1. Choose **Done** (not Move to Trash).
2. Open **System Settings → Privacy & Security** and scroll to the message about StoneQL.
3. Choose **Open Anyway**, then confirm (macOS may ask for your login password).

After that, open StoneQL from Applications as usual. Only open a copy whose checksum matched. A build you compiled yourself is not quarantined and opens without this step.

### Update to a newer build

Download the newer DMG, verify its checksum, quit StoneQL and drag the new StoneQL onto **Applications**, replacing the old one. Your saved connections, groups, theme and grid preferences and saved-password references are kept. A newly downloaded build may need the first-launch step above again, and macOS asks once more for access to saved passwords (next section).

### Saved passwords and the Keychain prompt

**Remember database password in the system keychain** is ticked by default for new connections. StoneQL then stores the password in your macOS login Keychain (never in its own files). macOS controls which apps may read that item:

- The first time a build reads a password that an earlier build saved, macOS asks for your **login** password twice for that one item: first “…wants to use your confidential information…” (the item's access list), then “…wants to access key…” (its key). Both refer to the same saved password.
- Choose **Always Allow**. **Allow** permits this one read only; **Deny** refuses it, and StoneQL reports that the saved credential is unavailable (nothing is retried).
- Every beta build is a new code identity, so expect this once after each update. If the prompts keep returning (with an ad-hoc signed build, Always Allow does not always stick), edit the connection, type the database password again with **Remember** ticked and save: the item is then owned by the installed build and reconnects silently.

### Uninstall

Quit StoneQL and move it from Applications to the Trash. This leaves your data behind: saved connections and preferences (WebKit data under `~/Library/WebKit/me.thixpin.stoneql` and `~/Library/Caches/me.thixpin.stoneql`), trusted SSH host keys and the unknown-outcome list (`~/Library/Application Support/me.thixpin.stoneql`) and saved passwords in the login Keychain. To delete saved passwords, remove the connections in StoneQL **before** uninstalling (removing a connection deletes its saved password), or delete leftover `me.thixpin.stoneql.credentials` items in Keychain Access afterwards. Reinstalling keeps using the data that was left behind.

### After installing

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
