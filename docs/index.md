---
layout: home
hero:
  name: StoneQL
  text: Old habits. New engine.
  tagline: StoneAge SQL Admin — formerly dbAdmin. A compact, familiar database client for MySQL, MariaDB and PostgreSQL on macOS, now in public beta.
  image:
    src: /icon.png
    alt: StoneQL faceted-stone database icon
  actions:
    - theme: brand
      text: Download v0.1.0-beta.1
      link: https://github.com/stoneAce/stoneql/releases/tag/v0.1.0-beta.1
    - theme: alt
      text: Install
      link: /guide/install
    - theme: alt
      text: Connect to a database
      link: /guide/connect
features:
  - title: Connect to your servers
    details: Live MySQL, MariaDB and PostgreSQL, directly or through an SSH tunnel, with verified TLS and explicitly trusted SSH host keys. Remember passwords in the macOS Keychain.
    link: /guide/connect
    linkText: Connect to a database
  - title: Browse and edit tables
    details: Page, sort and filter rows on the server; insert, edit and delete rows of tables with a primary key; change columns and indexes with an SQL preview.
    link: /guide/browse-and-edit
    linkText: Browse and edit
  - title: Run SQL safely
    details: Read-only connections on request, transactions, confirmation of destructive scripts, a PROD safeguard and honest "outcome unknown" reporting.
    link: /guide/run-sql
    linkText: Run SQL
  - title: Import and export
    details: Export a table or database as SQL, CSV or JSON (optionally GZIP) from one consistent snapshot. Import SQL files (MySQL/MariaDB) and CSV into a table.
    link: /guide/import-export
    linkText: Import and export
  - title: Help when something goes wrong
    details: First-launch approval, Keychain prompts and connection errors, with what each message means and what to do.
    link: /guide/troubleshooting
    linkText: Troubleshooting
  - title: What's in the beta
    details: macOS on Apple Silicon only, ad-hoc signed and not notarized. Search, table Operations and managing views, routines, triggers and events are not available yet.
    link: /guide/beta
    linkText: Beta scope
---

<div class="home-developer-link">

Building or contributing to StoneQL? See the [Developer Docs](./development/index.md).

</div>
