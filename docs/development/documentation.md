# Documentation maintenance

`docs/` is the single Markdown source. VitePress runs from `.website/`; its `srcDir` points to root `docs/`. No copied Markdown or Rust application is generated. The homepage content is `docs/index.md`; presentation, navigation, branding and original icon preparation live under `.website/`.

## Structure

- **User guide** (`docs/guide/`): task pages for people using the released app, in the order a first-time user needs them (download, install, connect, then daily tasks, troubleshooting and the beta's scope). Plain language and numbered steps; no milestone or decision ids, test commands or architecture terms. A user must be able to reach a working connection without leaving this section; links to Developer Docs belong only in an optional closing "For developers" note.
- **Developer Docs** (`docs/development/`, `docs/architecture/`, `docs/database-objects.md`, `docs/ux-review-remediation.md`): architecture, contributor setup, browser demo, test commands, verification and milestone status.

Navigation lives in `.website/site.mjs`: `guideSidebar` and `developerSidebar` are mapped by path prefix in `sidebar` (VitePress multi-sidebar), and `sidebarGroups` lists every group for the link checks. Add a page to the matching sidebar and, for a new top-level developer path, a prefix key in `sidebar`.

## Local workflow

```sh
npm run docs:dev
npm run docs:build
npm run docs:preview
DOCS_BASE=/stoneql/ npm run docs:build
```

The build validates source navigation and generated local page/asset URLs. VitePress provides local search, syntax highlighting, Light/Dark themes and previous/next links. Repository links appear when `DOCS_REPOSITORY_URL=https://github.com/OWNER/REPO` is supplied; Actions derives the URL automatically. No owner is invented when no remote exists.

## Diagrams

Use fenced `mermaid` blocks. The theme loads pinned Mermaid 11.12.0 from jsDelivr in the browser, with strict security and theme-aware rendering. Static builds work offline. Offline browsers show the diagram source and an unavailable message instead of a blank diagram. This CDN dependency and browser rendering remain unverified in the restricted environment; a bundled dependency can replace it when registry access is available. Do not claim offline graphical rendering.

## GitHub Pages

`.github/workflows/docs.yml` builds on documentation pull requests and manual dispatch. Deployment requires an explicit manual `deploy=true`; the site is published at <https://stoneace.github.io/stoneql/>. Configure the GitHub Pages environment with required reviewers before use. Default base is `/REPOSITORY/`. For a future custom domain, configure DNS/Pages separately and supply `custom_domain`; the build uses `/` and creates CNAME. Local custom-domain builds use `DOCS_CUSTOM_DOMAIN` and `DOCS_BASE=/`.

Keep proposals labelled as awaiting approval. Do not publish `.ref/`, local progress, secrets or generated development artifacts. External reference links require periodic human review; source/build checks validate local targets only.
