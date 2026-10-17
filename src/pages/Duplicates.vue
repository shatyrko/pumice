<script setup>
import { computed, onMounted, onUnmounted, reactive, ref } from 'vue';
import { call, on, pickFolder, reveal } from '../api.js';
import { bytes, dirName, baseName, shortPath, ago } from '../format.js';
import { confirm, errorText, store, toast } from '../store.js';
import Icon from '../components/Icon.vue';
import PageHead from '../components/PageHead.vue';

defineProps({ active: Boolean });

const roots = ref([]);
const minSize = ref(1_000_000);
const skipHidden = ref(true);
const scanning = ref(false);
const progress = ref({ seen: 0, hashed: 0 });
const groups = ref([]);
const searched = ref(false);
// hash -> Set of paths marked to remove
const remove = reactive({});
const busy = ref(false);
let off = null;

onMounted(() => {
  off = on('dup-progress', (p) => (progress.value = p));
  roots.value = ['~'];
  if (import.meta.env.DEV && new URLSearchParams(location.search).get('scan')) find();
});
onUnmounted(() => off && off());

async function add() {
  const p = await pickFolder('Folder to search');
  if (p && !roots.value.includes(p)) roots.value.push(p);
}

async function find() {
  if (!roots.value.length) return;
  scanning.value = true;
  searched.value = false;
  try {
    groups.value = await call('dup_find', { roots: roots.value, minSize: minSize.value, skipHidden: skipHidden.value });
    for (const k of Object.keys(remove)) delete remove[k];
    for (const g of groups.value) remove[g.hash] = new Set(g.files.slice(1).map((f) => f.path));
    searched.value = true;
  } catch (e) {
    toast(errorText(e), 'error');
  } finally {
    scanning.value = false;
  }
}

function toggle(g, path) {
  const s = new Set(remove[g.hash]);
  if (s.has(path)) s.delete(path);
  else {
    if (s.size >= g.files.length - 1) return toast('Keep at least one copy.', 'warn');
    s.add(path);
  }
  remove[g.hash] = s;
}

function auto(mode) {
  for (const g of groups.value) {
    const list = [...g.files];
    if (mode === 'newest') list.reverse();
    if (mode === 'short') list.sort((a, b) => a.path.length - b.path.length);
    remove[g.hash] = new Set(list.slice(1).map((f) => f.path));
  }
}

const wasted = computed(() => groups.value.reduce((a, g) => a + g.size * (g.files.length - 1), 0));
const selectedCount = computed(() => groups.value.reduce((a, g) => a + (remove[g.hash]?.size || 0), 0));
const selectedBytes = computed(() => groups.value.reduce((a, g) => a + g.size * (remove[g.hash]?.size || 0), 0));

async function apply() {
  const to = store.settings.use_trash;
  const ok = await confirm({
    title: `${to ? 'Move' : 'Delete'} ${selectedCount.value} duplicate files?`,
    body: `One copy of each file stays. ${bytes(selectedBytes.value)} ${to ? 'goes to the Trash.' : 'is deleted for good.'}`,
    confirmLabel: to ? 'Move to Trash' : 'Delete',
    danger: !to,
  });
  if (!ok) return;
  busy.value = true;
  try {
    const choices = groups.value
      .filter((g) => remove[g.hash]?.size)
      .map((g) => ({ hash: g.hash, keep: g.files.filter((f) => !remove[g.hash].has(f.path)).map((f) => f.path), remove: [...remove[g.hash]] }));
    const rep = await call('dup_apply', { choices, dryRun: store.dryRun });
    toast(store.dryRun ? `Would remove ${rep.removed} files (${bytes(rep.freed)})` : `${rep.removed} duplicates ${to ? 'moved to the Trash' : 'deleted'} (${bytes(rep.freed)})${rep.failed ? `, ${rep.failed} skipped` : ''}`, rep.failed ? 'warn' : 'info');
    if (!store.dryRun) await find();
  } catch (e) {
    toast(errorText(e), 'error');
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <div class="page">
    <PageHead>
      <template #title>Duplicates</template>
      <template #lead>Files with exactly the same content, found by comparing the data, not the names. One copy of each file is always kept.</template>
    </PageHead>
    <div class="bar-row">
      <span v-for="r in roots" :key="r" class="chip on">{{ r === '~' ? 'Home folder' : r }}<button class="x" aria-label="Remove folder" @click="roots = roots.filter((x) => x !== r)"><Icon name="x" style="width: 14px; height: 14px" /></button></span>
      <button class="btn small" @click="add"><Icon name="plus" />Add folder</button>
      <span class="grow" />
      <label class="row muted">Bigger than
        <select v-model.number="minSize"><option :value="100000">100 KB</option><option :value="1000000">1 MB</option><option :value="10000000">10 MB</option><option :value="100000000">100 MB</option></select>
      </label>
      <label class="row muted"><input type="checkbox" v-model="skipHidden" />Skip hidden folders</label>
      <button class="btn primary" :disabled="scanning || !roots.length" @click="find"><Icon name="search" />Find duplicates</button>
      <button v-if="scanning" class="btn" @click="call('dup_cancel')"><Icon name="stop" />Stop</button>
    </div>

    <div class="page-body">
      <div v-if="scanning" class="empty"><h2>Comparing files…</h2><p>{{ progress.seen.toLocaleString() }} files read · {{ progress.hashed.toLocaleString() }} compared in full</p></div>
      <div v-else-if="!searched" class="empty"><h2>Pick folders and search</h2><p>Hard links to the same file are not counted as duplicates.</p></div>
      <div v-else-if="!groups.length" class="empty"><h2>No duplicates found</h2></div>
      <template v-else>
        <div class="row" style="margin-bottom: 10px">
          <span class="muted grow">{{ groups.length }} groups · {{ bytes(wasted) }} are copies</span>
          <span class="muted">Keep:</span>
          <button class="btn small" @click="auto('oldest')">oldest</button>
          <button class="btn small" @click="auto('newest')">newest</button>
          <button class="btn small" @click="auto('short')">shortest path</button>
        </div>
        <div v-for="g in groups.slice(0, 300)" :key="g.hash" class="card g">
          <div class="row"><strong class="grow">{{ baseName(g.files[0].path) }}</strong><span class="muted">{{ g.files.length }} copies × {{ bytes(g.size) }}</span></div>
          <label v-for="f in g.files" :key="f.path" class="row f">
            <input type="checkbox" :checked="remove[g.hash]?.has(f.path)" @change="toggle(g, f.path)" :aria-label="'Remove ' + f.path" />
            <span class="badge" :class="remove[g.hash]?.has(f.path) ? 'high' : 'ok'">{{ remove[g.hash]?.has(f.path) ? 'remove' : 'keep' }}</span>
            <span class="mono ellipsis grow selectable" :title="f.path">{{ shortPath(f.path, store.home) }}</span>
            <span class="muted">{{ ago(f.mtime) }}</span>
            <button class="btn ghost icon" title="Show in the file manager" @click.prevent="reveal(f.path)"><Icon name="reveal" /></button>
          </label>
        </div>
        <p v-if="groups.length > 300" class="muted">The 300 largest groups are shown.</p>
      </template>
    </div>

    <footer class="page-foot">
      <div class="grow"><strong>{{ bytes(selectedBytes) }}</strong><span class="muted"> to remove · {{ selectedCount }} files</span></div>
      <button class="btn primary" :disabled="!selectedCount || busy" @click="apply"><Icon name="trash" />{{ store.dryRun ? 'Preview' : store.settings.use_trash ? 'Move to Trash' : 'Delete' }}</button>
    </footer>
  </div>
</template>

<style scoped>
.bar-row { display: flex; gap: 10px; align-items: center; padding: 0 28px 12px; flex-wrap: wrap; }
.x { background: none; border: 0; color: inherit; cursor: pointer; padding: 0; display: inline-flex; }
.g { margin-bottom: 10px; padding: 12px 14px; }
.f { padding: 5px 0; cursor: pointer; border-top: 1px solid var(--line); margin-top: 5px; }
</style>
