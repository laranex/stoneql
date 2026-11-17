import { migrateLegacyPreferences } from './services/brandingMigration';
import { createApp, nextTick } from 'vue';
import { createPinia } from 'pinia';
import App from './App.vue';
import { bootstrapNativeHost, confirmNativeRendered } from './services/nativeHost';
import './style.css';
async function start() {
  migrateLegacyPreferences(localStorage);
  await bootstrapNativeHost();
  createApp(App).use(createPinia()).mount('#app');
  await nextTick();
  // Rendered: the shell with either a workspace tree or the no-connection start screen.
  if (document.querySelector('.bottom-status') && document.querySelector('.table-tree, .empty-state')) {
    requestAnimationFrame(() => { void confirmNativeRendered().catch(() => {}); });
  }
}
void start();
