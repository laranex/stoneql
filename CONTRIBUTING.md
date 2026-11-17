# Contributing to StoneQL

Thanks for helping improve StoneQL. This guide covers how to report problems, propose changes and send a pull request.

## Before you start

- **Bugs and feature ideas:** open an [issue](https://github.com/stoneAce/stoneql/issues/new/choose) using the matching template. Search existing issues first.
- **Security problems:** do not open a public issue. Follow [SECURITY.md](SECURITY.md).
- **Questions about using StoneQL:** start with the [user guide](https://stoneace.github.io/stoneql/guide/) and [troubleshooting](https://stoneace.github.io/stoneql/guide/troubleshooting); see [SUPPORT.md](SUPPORT.md).
- Larger changes (new features, architecture, dependencies): open an issue to discuss them before writing code.

Everyone taking part follows our [Code of Conduct](CODE_OF_CONDUCT.md).

## Development setup

You need **Node.js 26** with npm, and the pinned Rust toolchain from `rust-toolchain.toml` (installed automatically by `rustup`). The desktop app is built and tested on macOS (Apple Silicon); Docker is needed for the real-database test fixtures.

```sh
npm ci
npm run dev            # browser demo (sample data, no database needed)
npm run desktop:run    # desktop app with live connections
```

The [Developer Docs](https://stoneace.github.io/stoneql/development/) explain the architecture, the desktop host and the full [verification](docs/development/verification.md) process. `AGENTS.md` lists the project's coding, security and documentation rules.

## Checks to run

Run the checks that match your change before opening a pull request:

| Change | Commands |
| --- | --- |
| Frontend | `npm test`, `npm run build`, `npm run test:browser` (both themes) |
| Rust | `npm run rust:fmt`, `npm run rust:lint`, `npm run rust:test`, `npm run rust:boundaries`, `npm run contracts:check` |
| Desktop host | `npm run desktop:test` |
| Database behaviour | the matching Docker-backed suite, e.g. `npm run test:native-database`, `npm run test:native-postgres`, `npm run test:execution`, `npm run test:ssh` |
| Documentation | `npm run docs:build` |

CI runs the Rust and frontend checks on macOS for every push and pull request.

## Pull requests

- Keep each pull request focused on one change, and follow the existing code style and patterns.
- Add or update tests for behaviour you change, and a regression test for any bug you fix.
- Update the user guide or Developer Docs when behaviour changes. Label proposals and unverified checks honestly.
- Use short [Conventional Commit](https://www.conventionalcommits.org/) titles, for example `fix: keep NULL distinct from empty text in CSV export`.
- Never commit passwords, keys, tokens, real customer data or local progress files.
- Describe what you changed, how you tested it and any limitations in the pull request.

## Licence

By contributing, you agree that your contributions are licensed under the [MIT Licence](LICENSE). Vendored third-party code keeps its own licence (for example `vendor/sqlx-mysql/LICENSE-MIT`).
