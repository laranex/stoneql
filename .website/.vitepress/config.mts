import { defineConfig } from 'vitepress';
import { fileURLToPath } from 'node:url';
import { sidebar, siteBase, repositoryUrl } from '../site.mjs';
const base = siteBase();
const repository = repositoryUrl();
export default defineConfig({
  title: 'dbAdmin', description: 'Database management, made familiar.',
  base, srcDir: fileURLToPath(new URL('../../docs/', import.meta.url)),
  outDir: fileURLToPath(new URL('../dist/', import.meta.url)),
  cacheDir: fileURLToPath(new URL('../cache/', import.meta.url)), appearance: true,
  head: [['link', { rel: 'icon', type: 'image/svg+xml', href: `${base}icon.svg` }]],
  vite: { publicDir: fileURLToPath(new URL('../public/', import.meta.url)) },
  themeConfig: {
    logo: '/icon.svg', nav: [{ text: 'Guide', link: '/getting-started/' }, { text: 'Architecture', link: '/architecture/overview' }, ...(repository ? [{ text: 'GitHub', link: repository }] : [])],
    sidebar, search: { provider: 'local' }, outline: [2, 3],
    socialLinks: repository ? [{ icon: 'github', link: repository }] : [],
    footer: { message: 'UI prototype · Database operations are simulated.' }
  },
  markdown: {
    config(md) {
      const fence = md.renderer.rules.fence!;
      md.renderer.rules.fence = (tokens, index, options, env, renderer) => {
        const token = tokens[index];
        if (token.info.trim() === 'mermaid') return `<MermaidDiagram :code="${md.utils.escapeHtml(JSON.stringify(token.content))}" />`;
        return fence(tokens, index, options, env, renderer);
      };
    }
  }
});
