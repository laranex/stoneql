# Security policy

## Supported versions

StoneQL is in public beta. Only the latest release receives security fixes.

| Version | Supported |
| --- | --- |
| 0.1.0-beta.1 (latest) | Yes |
| Earlier development builds | No |

## Reporting a vulnerability

**Please do not report security problems in public issues, discussions or pull requests.**

Report them privately through GitHub: open the repository's **Security** tab and choose **Report a vulnerability** ([direct link](https://github.com/stoneAce/stoneql/security/advisories/new)). Include:

- the StoneQL version and your macOS version;
- the database engine and connection type (Direct or SSH tunnel), if relevant;
- steps to reproduce, the impact you observed and any proof of concept;
- whether the issue is already public.

Never include real passwords, keys or production data in a report. Use test credentials and sample data.

We aim to acknowledge reports within 7 days and to agree a disclosure date with you once a fix is available.

## Scope notes

- The beta app is **ad-hoc signed and not notarized** (Developer ID signing is planned). Gatekeeper's first-launch warning is expected and not a vulnerability by itself; please do report anything that lets a modified app or update pass as genuine.
- Saved database and SSH passwords live in the macOS login Keychain; the app's files hold only opaque references. Reports about secrets reaching logs, files, the window or IPC replies are especially welcome.
- Vulnerabilities in dependencies are tracked with `npm run audit:release`; report them here if they affect StoneQL.
