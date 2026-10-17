<script setup>
import { computed, onMounted, onUnmounted, reactive, ref } from 'vue';
import { call, on, reveal } from '../api.js';
import { ago, baseName, bytes, dirName, shortPath } from '../format.js';
import { confirm, errorText, store, toast } from '../store.js';
import Icon from '../components/Icon.vue';
import PageHead from '../components/PageHead.vue';

defineProps({ active: Boolean });

const tab = ref('large');
const scanning = ref(false);
const scanned = ref(false);
const progress = ref({ files: 0, bytes: 0 });
const files = ref([]);
const minSize = ref(100_000_000);
const age = ref(0);
const kind = ref('all');
const picked = reactive(new Set());
const busy = ref(false);
let off = null;

onMounted(() => (off = on('scan-progress', (p) => (progress.value = p))));
onUnmounted(() => off && off());

async function scanHome() {
  scanning.value = true;
  try {
    await call('scan_start', { purpose: 'home', path: store.home, minFile: 1 << 20 });
    scanned.value = true;
    await load();
  } catch (e) {
    toast(errorText(e), 'error');
  } finally {
    scanning.value = false;
  }
}

async function load() {
  picked.clear();
  files.value = await call('large_files', {
    limit: 500,
    minSize: tab.value === 'installers' ? 1 << 20 : minSize.value,
    olderThanDays: age.value,
    onlyInstallers: tab.value === 'installers',
  });
}

function setTab(t) {
  tab.value = t;
  if (scanned.value) load();
}

const kinds = computed(() => ['all', ...new Set(files.value.map((f) => f.kind))]);
const shown = computed(() => files.value.filter((f) => kind.value === 'all' || f.kind === kind.value));
const total = computed(() => shown.value.filter((f) => picked.has(f.path)).reduce((a, f) => a + f.size, 0));

function toggle(p) {
  picked.has(p) ? picked.delete(p) : picked.add(p);
}
const icons = { video: 'video', image: 'image', archive: 'archive', 'disk image': 'disc', installer: 'purge', audio: 'file', document: 'file', log: 'file', other: 'file' };

async function remove() {
  const paths = [...picked];
  const to = store.settings.use_trash;
  const ok = await confirm({
    title: `${to ? 'Move' : 'Delete'} ${paths.length} files${to ? ' to the Trash' : ''}?`,
    body: `${bytes(total.value)} in total.${to ? ' The space is freed when you empty the Trash.' : ' This cannot be undone.'}`,
    lines: paths.slice(0, 8).map((p) => shortPath(p, store.home)),
    confirmLabel: to ? 'Move to Trash' : 'Delete',
    danger: !to,
  });
  if (!ok) return;
  busy.value = true;
  try {
    const rep = await call('remove_picked', { purpose: 'home', paths, dryRun: store.dryRun });
    toast(store.dryRun ? `Would remove ${rep.removed} files (${bytes(rep.freed)})` : `${rep.removed} files ${to ? 'moved to the Trash' : 'deleted'} (${bytes(rep.freed)})${rep.failed ? `, ${rep.failed} failed` : ''}`, rep.failed ? 'warn' : 'info');
    if (!store.dryRun) {
      const gone = new Set(paths.filter((p) => !rep.errors.some((e) => e.startsWith(p))));
      files.value = files.value.filter((f) => !gone.has(f.path));
      picked.clear();
    }
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
      <template #title>Large files</template>
      <template #lead>The biggest files in your home folder, and installers you no longer need. Files go to the Trash unless you turn that off in Settings.</template>
      <template #actions>
        <button class="btn primary" :disabled="scanning" @click="scanHome"><Icon name="search" />{{ scanned ? 'Scan again' : 'Scan home folder' }}</button>
      </template>
    </PageHead>

    <div v-if="scanned" class="bar-row">
      <button class="chip" :class="{ on: tab === 'large' }" @click="setTab('large')">Large files</button>
      <button class="chip" :class="{ on: tab === 'installers' }" @click="setTab('installers')">Installers and images</button>
      <span class="grow" />
      <template v-if="tab === 'large'">
        <label class="row muted">Bigger than
          <select v-model.number="minSize" @change="load">
            <option :value="20_000_000">20 MB</option><option :value="100_000_000">100 MB</option><option :value="500_000_000">500 MB</option><option :value="1_000_000_000">1 GB</option><option :value="5_000_000_000">5 GB</option>
          </select>
        </label>
      </template>
      <label class="row muted">Not used for
        <select v-model.number="age" @change="load">
          <option :value="0">any time</option><option :value="30">30 days</option><option :value="90">3 months</option><option :value="365">1 year</option>
        </select>
      </label>
    </div>
    <div v-if="scanned && kinds.length > 2" class="bar-row" style="padding-top: 0">
      <button v-for="k in kinds" :key="k" class="chip" :class="{ on: kind === k }" @click="kind = k">{{ k }}</button>
    </div>

    <div class="page-body">
      <div v-if="scanning" class="empty"><h2>Scanning your home folder…</h2><p>{{ progress.files.toLocaleString() }} files · {{ bytes(progress.bytes) }}</p></div>
      <div v-else-if="!scanned" class="empty"><h2>Find the files that take the most space</h2><p>The scan reads names and sizes only. It takes a few seconds.</p></div>
      <div v-else-if="!shown.length" class="empty"><h2>No files match</h2><p>Try a smaller size or a shorter time.</p></div>
      <table v-else class="table selectable">
        <thead><tr><th style="width: 34px" /><th>Name</th><th>Folder</th><th class="num">Size</th><th class="num">Last used</th><th style="width: 40px" /></tr></thead>
        <tbody>
          <tr v-for="f in shown" :key="f.path" @click="toggle(f.path)" style="cursor: pointer">
            <td><input type="checkbox" :checked="picked.has(f.path)" @click.stop @change="toggle(f.path)" :aria-label="f.name" /></td>
            <td><span class="row" style="gap: 8px"><Icon :name="icons[f.kind] || 'file'" class="muted" /><span class="ellipsis" :title="f.name">{{ f.name }}</span></span></td>
            <td class="muted mono ellipsis" style="max-width: 360px" :title="dirName(f.path)">{{ shortPath(dirName(f.path), store.home) }}</td>
            <td class="num"><strong>{{ bytes(f.size) }}</strong></td>
            <td class="num muted">{{ ago(Math.max(f.mtime, f.atime)) }}</td>
            <td><button class="btn ghost icon" title="Show in the file manager" @click.stop="reveal(f.path)"><Icon name="reveal" /></button></td>
          </tr>
        </tbody>
      </table>
    </div>

    <footer class="page-foot">
      <div class="grow"><strong>{{ bytes(total) }}</strong><span class="muted"> selected · {{ picked.size }} files</span></div>
      <button class="btn" :disabled="!picked.size" @click="picked.clear()">Clear</button>
      <button class="btn primary" :disabled="!picked.size || busy" @click="remove"><Icon name="trash" />{{ store.dryRun ? 'Preview' : store.settings.use_trash ? 'Move to Trash' : 'Delete' }}</button>
    </footer>
  </div>
</template>

<style scoped>
.bar-row { display: flex; gap: 10px; align-items: center; padding: 0 28px 12px; flex-wrap: wrap; }
</style>
