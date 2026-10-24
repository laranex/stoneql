import { createApp, nextTick } from 'vue';
import { createPinia } from 'pinia';
import App from './App.vue';
import { bootstrapNativeHost, confirmNativeRendered } from './services/nativeHost';
import './style.css';
async function start() {
  await bootstrapNativeHost();
  createApp(App).use(createPinia()).mount('#app');
  await nextTick();
  if (document.querySelector('.bottom-status') && document.querySelector('.table-tree')) {
    requestAnimationFrame(() => { void confirmNativeRendered().catch(() => {}); });
  }
}
void start();
