<script setup>
import { computed, onMounted, onUnmounted, ref, watch } from 'vue';
import { call, on, pickFolder, reveal } from '../api.js';
import { ago, bytes } from '../format.js';
import { confirm, errorText, store, toast } from '../store.js';
import Icon from '../components/Icon.vue';
import PageHead from '../components/PageHead.vue';
import Treemap from '../components/Treemap.vue';
import Sunburst from '../components/Sunburst.vue';

defineProps({ active: Boolean });

const path = ref('');
const scanning = ref(false);
const progress = ref({ files: 0, bytes: 0, dirs: 0 });
const summary = ref(null);
const listing = ref(null);
const stack = ref([]); // [{id, name}] from the root down to the current folder
const view = ref('list');
const depth = ref(3);
const tree = ref(null);
const mounts = ref([]);
let off = null;

onMounted(async () => {
  off = on('scan-progress', (p) => (progress.value = p));
  path.value = store.home || (await call('places')).home;
  try {
    mounts.value = (await call('system_snapshot')).disks.mounts.map((d) => d.path);
  } catch {
    mounts.value = [];
  }
});
onUnmounted(() => off && off());

async function scan(p) {
  if (p) path.value = p;
  if (!path.value || scanning.value) return;
  scanning.value = true;
  progress.value = { files: 0, bytes: 0, dirs: 0 };
  try {
    summary.value = await call('scan_start', { purpose: 'analyze', path: path.value, minFile: null });
    stack.value = [];
    await open(0, summary.value.root);
  } catch (e) {
    toast(errorText(e), 'error');
  } finally {
    scanning.value = false;
  }
}

async function open(id, name) {
  const l = await call('scan_rows', { purpose: 'analyze', id });
  listing.value = l;
  const i = stack.value.findIndex((s) => s.id === id);
  if (i >= 0) stack.value = stack.value.slice(0, i + 1);
  else stack.value.push({ id, name: name ?? l.path });
  await loadTree();
}

async function up() {
  if (stack.value.length > 1) await open(stack.value[stack.value.length - 2].id);
}

async function loadTree() {
  if (view.value === 'list' || !listing.value) return;
  tree.value = await call('scan_view', { purpose: 'analyze', id: listing.value.id, depth: view.value === 'treemap' ? 3 : depth.value, perLevel: view.value === 'treemap' ? 60 : 40 });
}
watch([view, depth], loadTree);

async function choose() {
  const p = await pickFolder('Folder to analyze');
  if (p) scan(p);
}

function pct(r) {
  return listing.value?.size ? (r.size / listing.value.size) * 100 : 0;
}

const crumbs = computed(() => stack.value.map((s, i) => ({ ...s, label: i === 0 ? s.name : s.name })));

async function trashRow(r) {
  const full = `${listing.value.path.replace(/\/$/, '')}/${r.name}`;
  const to = store.settings.use_trash;
  const ok = await confirm({
    title: `${to ? 'Move to the Trash' : 'Delete'}: ${r.name}?`,
    body: `${bytes(r.size)} · ${full}`,
    confirmLabel: to ? 'Move to Trash' : 'Delete',
    danger: !to,
  });
  if (!ok) return;
  try {
    const rep = await call('remove_picked', { purpose: 'analyze', paths: [full], dryRun: store.dryRun });
    if (rep.failed) toast(rep.errors[0] || 'Could not remove it', 'error');
    else {
      toast(store.dryRun ? `Would remove ${bytes(rep.freed)}` : `${to ? 'Moved to the Trash' : 'Deleted'}: ${bytes(rep.freed)}. Scan again to update the sizes.`);
      if (!store.dryRun) listing.value.rows = listing.value.rows.filter((x) => x !== r);
    }
  } catch (e) {
    toast(errorText(e), 'error');
  }
}
async function revealRow(r) {
  reveal(`${listing.value.path.replace(/\/$/, '')}/${r.name}`);
}
</script>

<template>
  <div class="page">
    <PageHead>
      <template #title>Analyze</template>
      <template #lead>See what fills a disk or a folder. Pick a place, scan it, then dig down by clicking folders.</template>
    </PageHead>

    <div class="bar-row">
      <input type="text" v-model="path" class="grow" placeholder="/path/to/folder" @keydown.enter="scan()" aria-label="Folder to scan" />
      <button class="btn" @click="choose"><Icon name="folder" />Choose…</button>
      <button class="btn primary" :disabled="scanning" @click="scan()"><Icon name="play" />Scan</button>
      <button v-if="scanning" class="btn" @click="call('scan_cancel')"><Icon name="stop" />Stop</button>
    </div>
    <div class="bar-row" style="padding-top: 0">
      <button class="chip" @click="scan(store.home)">Home</button>
      <button v-for="m in mounts" :key="m" class="chip" @click="scan(m)">{{ m }}</button>
      <span class="grow" />
      <template v-if="listing">
        <button class="chip" :class="{ on: view === 'list' }" @click="view = 'list'"><Icon name="list" />List</button>
        <button class="chip" :class="{ on: view === 'treemap' }" @click="view = 'treemap'"><Icon name="treemap" />Treemap</button>
        <button class="chip" :class="{ on: view === 'sunburst' }" @click="view = 'sunburst'"><Icon name="sunburst" />Sunburst</button>
        <label v-if="view === 'sunburst'" class="row muted" style="gap: 6px">Rings <input type="range" min="1" max="5" v-model.number="depth" /></label>
      </template>
    </div>

    <div class="page-body">
      <div v-if="scanning" class="empty">
        <h2>Scanning…</h2>
        <p>{{ progress.files.toLocaleString() }} files · {{ bytes(progress.bytes) }} · {{ progress.dirs.toLocaleString() }} folders</p>
      </div>
      <div v-else-if="!listing" class="empty"><h2>Nothing scanned yet</h2><p>Choose Home, a disk, or any folder above.</p></div>
      <template v-else>
        <div class="crumbs row wrap">
          <button class="btn ghost small" :disabled="stack.length < 2" @click="up"><Icon name="up" />Up</button>
          <template v-for="(c, i) in crumbs" :key="c.id">
            <Icon v-if="i" name="chevron" />
            <button class="crumb" @click="open(c.id)">{{ c.name.split('/').filter(Boolean).pop() || '/' }}</button>
          </template>
          <span class="grow" />
          <span class="muted">{{ bytes(listing.size) }} · {{ listing.files.toLocaleString() }} files<template v-if="summary"> · scanned in {{ summary.millis }} ms</template></span>
        </div>
        <p v-if="summary?.errors" class="muted" style="margin: 2px 0 8px">{{ summary.errors }} folders could not be read (permissions).</p>

        <table v-if="view === 'list'" class="table selectable">
          <thead><tr><th>Name</th><th style="width: 22%">Share</th><th class="num">Size</th><th class="num">Files</th><th class="num">Modified</th><th style="width: 74px" /></tr></thead>
          <tbody>
            <tr v-for="r in listing.rows" :key="r.name + r.id">
              <td>
                <button v-if="r.is_dir" class="namebtn" @click="open(r.id, r.name)"><Icon name="folder" style="color: var(--accent)" />{{ r.name }}</button>
                <span v-else class="row" style="gap: 7px"><Icon name="file" class="muted" /><span class="ellipsis">{{ r.name }}</span></span>
              </td>
              <td><div class="bar"><i :style="{ width: Math.max(1, pct(r)) + '%' }" /></div></td>
              <td class="num">{{ bytes(r.size) }}</td>
              <td class="num muted">{{ r.files ? r.files.toLocaleString() : '' }}</td>
              <td class="num muted">{{ r.mtime ? ago(r.mtime) : '' }}</td>
              <td class="num">
                <template v-if="r.id !== null">
                  <button class="btn ghost icon" title="Show in the file manager" @click="revealRow(r)"><Icon name="reveal" /></button>
                  <button class="btn ghost icon" title="Remove" @click="trashRow(r)"><Icon name="trash" /></button>
                </template>
              </td>
            </tr>
          </tbody>
        </table>
        <Treemap v-else-if="view === 'treemap' && tree" :root="tree" @open="(n) => n.id != null && n.is_dir && open(n.id, n.name)" />
        <Sunburst v-else-if="view === 'sunburst' && tree" :root="tree" :depth="depth" @open="(n) => n.id != null && n.is_dir && open(n.id, n.name)" />
      </template>
    </div>
  </div>
</template>

<style scoped>
.bar-row { display: flex; gap: 10px; align-items: center; padding: 0 28px 12px; flex-wrap: wrap; }
.crumbs { padding: 4px 0 10px; }
.crumb { background: none; border: 0; color: var(--text); font: inherit; font-weight: 600; cursor: pointer; padding: 2px 4px; border-radius: 4px; }
.crumb:hover { background: var(--panel-2); }
.namebtn { display: inline-flex; align-items: center; gap: 7px; background: none; border: 0; color: var(--text); font: inherit; cursor: pointer; padding: 0; }
.namebtn:hover { color: var(--accent); }
</style>
