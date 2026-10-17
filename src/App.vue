<script setup>
import { computed, onMounted } from 'vue';
import { store, loadBasics, go } from './store.js';
import Icon from './components/Icon.vue';
import ConfirmDialog from './components/ConfirmDialog.vue';
import Dashboard from './pages/Dashboard.vue';
import Analyze from './pages/Analyze.vue';
import Clean from './pages/Clean.vue';
import LargeFiles from './pages/LargeFiles.vue';
import Duplicates from './pages/Duplicates.vue';
import Purge from './pages/Purge.vue';
import Apps from './pages/Apps.vue';
import Startup from './pages/Startup.vue';
import Maintenance from './pages/Maintenance.vue';
import History from './pages/History.vue';
import Settings from './pages/Settings.vue';

const pages = {
  dashboard: Dashboard, analyze: Analyze, clean: Clean, large: LargeFiles, duplicates: Duplicates,
  purge: Purge, apps: Apps, startup: Startup, maintenance: Maintenance, history: History, settings: Settings,
};

const groups = [
  { title: 'Overview', items: [{ id: 'dashboard', label: 'Dashboard', icon: 'dashboard' }] },
  {
    title: 'Space',
    items: [
      { id: 'clean', label: 'Clean', icon: 'clean' },
      { id: 'analyze', label: 'Analyze', icon: 'analyze' },
      { id: 'large', label: 'Large files', icon: 'large' },
      { id: 'duplicates', label: 'Duplicates', icon: 'copy' },
      { id: 'purge', label: 'Projects', icon: 'purge' },
    ],
  },
  {
    title: 'System',
    items: [
      { id: 'apps', label: 'Apps', icon: 'apps' },
      { id: 'startup', label: 'Startup', icon: 'startup' },
      { id: 'maintenance', label: 'Maintenance', icon: 'wrench' },
    ],
  },
  {
    title: 'Pumice',
    items: [
      { id: 'history', label: 'History', icon: 'history' },
      { id: 'settings', label: 'Settings', icon: 'settings' },
    ],
  },
];

const current = computed(() => pages[store.page] || Dashboard);

// Pages are created on first visit and then kept, so a scan survives a
// trip to another page.
const visited = new Set([store.page]);
const mounted = computed(() => {
  visited.add(store.page);
  return [...visited];
});

onMounted(loadBasics);

function onKey(e) {
  if (e.ctrlKey && !e.shiftKey && !e.altKey && /^[1-9]$/.test(e.key)) {
    const flat = groups.flatMap((g) => g.items);
    const item = flat[Number(e.key) - 1];
    if (item) {
      go(item.id);
      e.preventDefault();
    }
  }
}
</script>

<template>
  <div class="shell" @keydown="onKey" tabindex="-1">
    <nav class="nav" aria-label="Sections">
      <div class="brand"><img src="/pumice.svg" alt="" width="26" height="26" /><span>Pumice</span></div>
      <template v-for="g in groups" :key="g.title">
        <div class="group">{{ g.title }}</div>
        <button v-for="it in g.items" :key="it.id" :class="{ on: store.page === it.id }" @click="go(it.id)">
          <Icon :name="it.icon" /><span>{{ it.label }}</span>
        </button>
      </template>
      <div class="grow" />
      <label class="dry" title="Show what would happen without removing anything">
        <span class="switch"><input type="checkbox" v-model="store.dryRun" /><span /></span>
        <span>Dry run</span>
      </label>
    </nav>
    <main class="main">
      <section v-for="id in mounted" :key="id" v-show="store.page === id" class="slot">
        <component :is="pages[id]" :active="store.page === id" />
      </section>
    </main>
    <ConfirmDialog v-if="store.dialog" />
    <div class="toasts" aria-live="polite">
      <div v-for="t in store.toasts" :key="t.id" class="toast" :class="t.kind">{{ t.text }}</div>
    </div>
  </div>
</template>

<style scoped>
.shell { display: flex; height: 100%; outline: none; }
.nav { width: 214px; flex: none; background: var(--panel); border-right: 1px solid var(--line); padding: 14px 10px; display: flex; flex-direction: column; gap: 2px; overflow-y: auto; }
.brand { display: flex; align-items: center; gap: 10px; padding: 4px 10px 14px; font-weight: 700; font-size: 17px; letter-spacing: -0.01em; }
.group { font-size: 11px; text-transform: uppercase; letter-spacing: 0.06em; color: var(--muted); padding: 12px 10px 4px; }
.nav button { display: flex; align-items: center; gap: 11px; height: 36px; padding: 0 10px; border: 0; border-radius: 8px; background: transparent; color: var(--text); font: inherit; font-weight: 500; cursor: pointer; text-align: left; }
.nav button:hover { background: var(--panel-2); }
.nav button.on { background: var(--accent-soft); color: var(--text); font-weight: 650; }
.nav button.on :deep(svg) { color: var(--accent); }
.dry { display: flex; align-items: center; gap: 10px; padding: 10px; color: var(--muted); font-size: 13px; cursor: pointer; }
.main { flex: 1; min-width: 0; position: relative; }
.slot { position: absolute; inset: 0; }
</style>
