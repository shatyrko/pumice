import { reactive, watch } from 'vue';
import { call } from './api.js';

function saved(key, fallback) {
  try {
    const v = localStorage.getItem(key);
    return v == null ? fallback : JSON.parse(v);
  } catch {
    return fallback;
  }
}

if (import.meta.env.DEV) {
  const t = new URLSearchParams(location.search).get('theme');
  if (t) document.documentElement.dataset.theme = t;
}

export const store = reactive({
  // ?page=… only in the dev server (used to take the screenshots of the README)
  page: (import.meta.env.DEV && new URLSearchParams(location.search).get('page')) || saved('pumice.page', 'dashboard'),
  dryRun: saved('pumice.dryRun', false),
  home: '',
  settings: { purge_roots: [], use_trash: true, min_file_bytes: 1 << 20, protected: [], purge_idle_days: 30, cpu_warn: 70, cpu_crit: 90, temp_warn: 80, temp_crit: 95, ram_warn: 75, ram_crit: 90, temp_unit: 'c', ai_provider: 'off', ai_base_url: '', ai_model: '' },
  helper: { installed: true, path: '' },
  toasts: [],
  dialog: null,
});

watch(
  () => [store.page, store.dryRun],
  () => {
    try {
      localStorage.setItem('pumice.page', JSON.stringify(store.page));
      localStorage.setItem('pumice.dryRun', JSON.stringify(store.dryRun));
    } catch {
      // Private mode: the choice is only kept for this session.
    }
  },
);

export async function loadBasics() {
  const [places, settings, helper] = await Promise.all([call('places'), call('settings_get'), call('helper_status')]);
  store.home = places.home;
  Object.assign(store.settings, settings);
  store.helper = helper;
}

export async function saveSettings() {
  await call('settings_set', { value: JSON.parse(JSON.stringify(store.settings)) });
}

let toastId = 0;
export function toast(text, kind = 'info', ms = 5000) {
  const id = ++toastId;
  store.toasts.push({ id, text, kind });
  setTimeout(() => {
    const i = store.toasts.findIndex((t) => t.id === id);
    if (i >= 0) store.toasts.splice(i, 1);
  }, ms);
}

export function errorText(e) {
  return typeof e === 'string' ? e : e?.message || String(e);
}

/** Asks the person to confirm. Resolves true or false. */
export function confirm(options) {
  return new Promise((resolve) => {
    store.dialog = { ...options, resolve };
  });
}

export function go(page) {
  store.page = page;
}
