# Getting started

Use Node.js **26** and npm from the repository root:

```sh
npm ci
npm run dev
```

Open the URL printed by Vite. No database server, Rust toolchain or credentials are required. Sample connections open realistic relational mock databases. Changes to records and schema last only for the session.

## Commands

| Command | Purpose |
| --- | --- |
| `npm run dev` | Run the shared Vue frontend |
| `npm run build` | Type-check and bundle the frontend |
| `npm run preview` | Serve the frontend production bundle |
| `npm test` | Run frontend and workspace regression tests |
| `npm run test:browser` | Run the 17 Playwright scenarios |
| `npm run docs:dev` | Run this documentation site |
| `npm run docs:build` | Check links and build documentation |
| `npm run docs:preview` | Preview built documentation |

Install Playwright Chromium with `npx playwright install chromium` when required. Browser execution needs a host that permits local listeners and sandboxed browser processes. See [verification](../development/verification.md) for current results.

## Prototype boundaries

Connection testing, SSH setup, SQL execution, object management and transfers are simulations. A success notification does not mean a real database changed or a backup file was created. Never enter production credentials into a prototype.

Continue with the [workspace guide](../user-guide/index.md).
