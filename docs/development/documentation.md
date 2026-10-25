# Documentation maintenance

`docs/` is the single Markdown source. VitePress runs from `.website/`; its `srcDir` points to root `docs/`. No copied Markdown or Rust application is generated. The homepage content is `docs/index.md`; presentation, navigation, branding and original icon preparation live under `.website/`.

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

The prepared workflow builds on documentation pull requests and manual dispatch. Deployment requires an explicit manual `deploy=true`; it has not been executed. Configure the GitHub Pages environment with required reviewers before use. Default base is `/REPOSITORY/`. For a future custom domain, configure DNS/Pages separately and supply `custom_domain`; the build uses `/` and creates CNAME. Local custom-domain builds use `DOCS_CUSTOM_DOMAIN` and `DOCS_BASE=/`.

Keep proposals labelled as awaiting approval. Do not publish `.ref/`, local progress, secrets or generated development artifacts. External reference links require periodic human review; source/build checks validate local targets only.
