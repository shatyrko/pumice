import { createApp } from 'vue';
import App from './App.vue';
import './style.css';

async function start() {
  // In a plain browser (vite dev server) there is no Rust side: use sample data.
  if (import.meta.env.DEV && !window.__TAURI_INTERNALS__) await import('./mock.js');
  createApp(App).mount('#app');
}
start();
