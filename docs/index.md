---
layout: home
hero:
  name: StoneQL
  text: Old habits. New engine.
  tagline: StoneAge SQL Admin — formerly dbAdmin. A compact, familiar database client for macOS, now in public beta.
  image:
    src: /icon.png
    alt: StoneQL faceted-stone database icon
  actions:
    - theme: brand
      text: Get started
      link: /getting-started/
    - theme: alt
      text: Download v0.1.0-beta.1
      link: https://github.com/stoneAce/stoneql/releases/tag/v0.1.0-beta.1
    - theme: alt
      text: User guide
      link: /user-guide/
features:
  - title: Live MySQL, MariaDB and PostgreSQL
    details: Connect directly or through an SSH tunnel, with verified TLS for remote servers and explicitly trusted SSH host keys. Passwords can be remembered in the macOS Keychain.
  - title: Familiar workflows
    details: Browse with server-side paging, sort and filter, switch and create MySQL/MariaDB databases, change columns and indexes with an SQL preview, run SQL scripts in transactions and edit rows of tables with a primary key.
  - title: Careful by default
    details: Read-only connections on request, PROD acknowledgement before read-write, confirmation of destructive scripts and honest "outcome unknown" reporting.
  - title: Import and export
    details: Export a table or database as SQL, CSV or JSON (optionally GZIP) from one read-only snapshot. Import SQL files (MySQL/MariaDB) and CSV into a table.
  - title: Beta boundaries
    details: macOS on Apple Silicon only, ad-hoc signed and not notarized. Search, table Operations, bulk Copy and managing views, routines, triggers and events are not available for live connections yet.
  - title: Browser demo
    details: The same Vue interface runs in a browser with sample data and simulated operations. Nothing in the demo touches a real database.
---
