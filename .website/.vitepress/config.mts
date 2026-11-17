import { defineConfig } from 'vitepress';
import { fileURLToPath } from 'node:url';
import { sidebar, siteBase, repositoryUrl } from '../site.mjs';
const base = siteBase();
const repository = repositoryUrl();
export default defineConfig({
  title: 'StoneQL', description: 'StoneAge SQL Admin. Old habits. New engine.',
  base, srcDir: fileURLToPath(new URL('../../docs/', import.meta.url)),
  outDir: fileURLToPath(new URL('../dist/', import.meta.url)),
  cacheDir: fileURLToPath(new URL('../cache/', import.meta.url)), appearance: true,
  head: [['link', { rel: 'icon', type: 'image/png', href: `${base}icon.png` }]],
  vite: { publicDir: fileURLToPath(new URL('../public/', import.meta.url)) },
  themeConfig: {
    logo: '/icon.png', nav: [{ text: 'Guide', link: '/getting-started/' }, { text: 'Architecture', link: '/architecture/overview' }, ...(repository ? [{ text: 'GitHub', link: repository }] : [])],
    sidebar, search: { provider: 'local' }, outline: [2, 3],
    socialLinks: repository ? [{ icon: 'github', link: repository }] : [],
    footer: { message: 'Public beta · macOS desktop app with live connections · Browser demo uses sample data.' }
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
