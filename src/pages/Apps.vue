<script setup>
import { computed, onMounted, reactive, ref, watch } from 'vue';
import { call, reveal } from '../api.js';
import { bytes, shortPath } from '../format.js';
import { confirm, errorText, store, toast } from '../store.js';
import Icon from '../components/Icon.vue';
import PageHead from '../components/PageHead.vue';

defineProps({ active: Boolean });

const apps = ref([]);
const loading = ref(true);
const query = ref('');
const source = ref('all');
const sort = ref('size');
const sel = ref(null);
const leftovers = ref([]);
const loadingLeft = ref(false);
const ticked = reactive(new Set());
const busy = ref(false);

async function load() {
  loading.value = true;
  try {
    apps.value = await call('apps_list');
  } catch (e) {
    toast(errorText(e), 'error');
  } finally {
    loading.value = false;
  }
}
onMounted(load);

const sources = computed(() => ['all', ...new Set(apps.value.map((a) => a.source))]);
const shown = computed(() => {
  const q = query.value.trim().toLowerCase();
  const list = apps.value.filter((a) => (source.value === 'all' || a.source === source.value) && (!q || a.name.toLowerCase().includes(q) || a.package.toLowerCase().includes(q)));
  return list.sort((a, b) => (sort.value === 'size' ? b.size - a.size : a.name.localeCompare(b.name)));
});

watch(sel, async (a) => {
  leftovers.value = [];
  ticked.clear();
  if (!a) return;
  loadingLeft.value = true;
  try {
    leftovers.value = await call('app_leftovers', { id: a.id });
    for (const l of leftovers.value) if (!l.caution) ticked.add(l.path);
  } catch (e) {
    toast(errorText(e), 'error');
  } finally {
    loadingLeft.value = false;
  }
});

const leftSize = computed(() => leftovers.value.filter((l) => ticked.has(l.path)).reduce((a, l) => a + l.size, 0));
const needsHelper = computed(() => sel.value && ['pacman', 'apt', 'dnf', 'snap'].includes(sel.value.source) && !store.helper.installed);

async function uninstall() {
  const a = sel.value;
  const cautious = leftovers.value.filter((l) => ticked.has(l.path) && l.caution);
  const ok = await confirm({
    title: `Uninstall ${a.name}?`,
    body: `${bytes(a.size)} will be removed${ticked.size ? `, with ${ticked.size} leftover folders (${bytes(leftSize.value)})` : ''}.${['pacman', 'apt', 'dnf', 'snap'].includes(a.source) ? '\nYou will be asked for your password.' : ''}${cautious.length ? `\n\nSettings and data of the app are ticked (${cautious.map((c) => c.kind).join(', ')}). They may hold your own work.` : ''}`,
    confirmLabel: store.dryRun ? 'Close' : 'Uninstall',
    danger: true,
  });
  if (!ok) return;
  if (store.dryRun) return toast('Dry run is on: nothing was removed.');
  busy.value = true;
  try {
    const rep = await call('app_uninstall', { id: a.id, leftovers: [...ticked] });
    toast(`${a.name} removed${rep.removed_leftovers ? `, ${rep.removed_leftovers} leftovers cleaned` : ''}${rep.errors.length ? `. ${rep.errors.length} problems.` : ''}`, rep.errors.length ? 'warn' : 'info');
    sel.value = null;
    await load();
  } catch (e) {
    toast(errorText(e), 'error', 9000);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <div class="page">
    <PageHead>
      <template #title>Apps</template>
      <template #lead>Installed programs from pacman, apt, dnf, Flatpak, Snap and AppImage. Uninstall one together with the folders it left in your home folder.</template>
      <template #actions><button class="btn" :disabled="loading" @click="load"><Icon name="refresh" />Refresh</button></template>
    </PageHead>
    <div class="bar-row">
      <input type="search" v-model="query" placeholder="Search apps" style="width: 240px" aria-label="Search apps" />
      <button v-for="s in sources" :key="s" class="chip" :class="{ on: source === s }" @click="source = s">{{ s }}</button>
      <span class="grow" />
      <label class="row muted">Sort
        <select v-model="sort"><option value="size">by size</option><option value="name">by name</option></select>
      </label>
    </div>
    <div class="split">
      <div class="list">
        <div v-if="loading" class="empty"><h2>Reading installed apps…</h2></div>
        <button v-for="a in shown" :key="a.id" class="app" :class="{ on: sel?.id === a.id }" @click="sel = a">
          <img v-if="a.icon" :src="a.icon" width="32" height="32" alt="" />
          <span v-else class="ph"><Icon name="apps" /></span>
          <span class="grow ellipsis"><strong>{{ a.name }}</strong><br /><small class="muted">{{ a.package.split('/').pop() }}</small></span>
          <span class="badge">{{ a.source }}</span>
          <span class="muted num">{{ a.size ? bytes(a.size) : '' }}</span>
        </button>
      </div>
      <aside class="detail card" v-if="sel">
        <div class="row">
          <img v-if="sel.icon" :src="sel.icon" width="48" height="48" alt="" />
          <div class="grow"><h2>{{ sel.name }}</h2><p class="muted">{{ sel.source }} · {{ sel.package.split('/').pop() }}<span v-if="sel.version"> · {{ sel.version }}</span></p></div>
        </div>
        <p v-if="sel.comment" class="muted">{{ sel.comment }}</p>
        <p><strong>{{ bytes(sel.size) }}</strong> <span class="muted">installed</span></p>
        <h3 style="margin-top: 6px">Leftovers in your home folder</h3>
        <p v-if="loadingLeft" class="muted">Looking…</p>
        <p v-else-if="!leftovers.length" class="muted">No settings, data or cache folders found.</p>
        <label v-for="l in leftovers" :key="l.path" class="row lf">
          <input type="checkbox" :checked="ticked.has(l.path)" @change="ticked.has(l.path) ? ticked.delete(l.path) : ticked.add(l.path)" />
          <span class="grow ellipsis mono selectable" :title="l.path">{{ shortPath(l.path, store.home) }}</span>
          <span class="badge" :class="l.caution ? 'medium' : 'low'">{{ l.kind }}</span>
          <span class="muted num">{{ bytes(l.size) }}</span>
          <button class="btn ghost icon" title="Show in the file manager" @click.prevent="reveal(l.path)"><Icon name="reveal" /></button>
        </label>
        <p v-if="leftovers.some((l) => l.caution)" class="muted" style="font-size: 12px">Orange items are settings or data that may be yours. They stay unticked unless you tick them.</p>
        <div v-if="needsHelper" class="notice">The administrator helper is not installed, so this app cannot be removed from here. Run <span class="mono">scripts/install-helper.sh</span> once.</div>
        <div class="row" style="justify-content: flex-end; margin-top: 4px">
          <button class="btn danger" :disabled="busy || needsHelper" @click="uninstall"><Icon name="trash" />Uninstall{{ ticked.size ? ` + ${ticked.size} leftovers` : '' }}</button>
        </div>
      </aside>
      <aside class="detail empty" v-else><p>Pick an app to see what it left behind.</p></aside>
    </div>
  </div>
</template>

<style scoped>
.bar-row { display: flex; gap: 10px; align-items: center; padding: 0 28px 12px; flex-wrap: wrap; }
.split { flex: 1; min-height: 0; display: grid; grid-template-columns: minmax(300px, 1fr) minmax(340px, 440px); gap: 16px; padding: 0 28px 20px; }
.list { overflow: auto; min-height: 0; }
.app { display: flex; align-items: center; gap: 12px; width: 100%; padding: 8px 10px; border: 0; border-radius: 8px; background: transparent; color: var(--text); font: inherit; text-align: left; cursor: pointer; }
.app:hover { background: var(--panel-2); }
.app.on { background: var(--accent-soft); }
.ph { width: 32px; height: 32px; display: grid; place-items: center; background: var(--panel-2); border-radius: 8px; color: var(--muted); flex: none; }
.num { white-space: nowrap; min-width: 62px; text-align: right; }
.detail { overflow: auto; min-height: 0; display: flex; flex-direction: column; gap: 10px; align-self: start; max-height: 100%; }
.lf { padding: 4px 0; cursor: pointer; }
</style>
