<script setup>
import { computed, onMounted, reactive, ref } from 'vue';
import { call, pickFolder, reveal } from '../api.js';
import { bytes, shortPath } from '../format.js';
import { confirm, errorText, store, toast } from '../store.js';
import Icon from '../components/Icon.vue';
import PageHead from '../components/PageHead.vue';

defineProps({ active: Boolean });

const items = ref([]);
const busy = ref(false);
const scanned = ref(false);
const extra = ref([]);
const picked = reactive(new Set());

async function scan() {
  busy.value = true;
  try {
    items.value = await call('purge_scan', { extraRoots: extra.value });
    picked.clear();
    for (const a of items.value) if (a.age_days >= store.settings.purge_idle_days) picked.add(a.path);
    scanned.value = true;
  } catch (e) {
    toast(errorText(e), 'error');
  } finally {
    busy.value = false;
  }
}
onMounted(scan);

async function add() {
  const p = await pickFolder('Folder with your projects');
  if (p && !extra.value.includes(p)) {
    extra.value.push(p);
    scan();
  }
}

const total = computed(() => items.value.filter((a) => picked.has(a.path)).reduce((s, a) => s + a.size, 0));
const all = computed(() => items.value.reduce((s, a) => s + a.size, 0));

function toggle(p) {
  picked.has(p) ? picked.delete(p) : picked.add(p);
}

async function apply() {
  const paths = [...picked];
  const fresh = items.value.filter((a) => picked.has(a.path) && a.age_days < 7).length;
  const ok = await confirm({
    title: `Delete ${paths.length} build folders?`,
    body: `${bytes(total.value)} will be removed for good. They can be rebuilt with the project's own tools (npm install, cargo build, …).${fresh ? `\n\n${fresh} of them belong to projects changed in the last week.` : ''}`,
    lines: paths.slice(0, 8).map((p) => shortPath(p, store.home)),
    confirmLabel: 'Delete',
    danger: true,
  });
  if (!ok) return;
  busy.value = true;
  try {
    const rep = await call('purge_apply', { paths, dryRun: store.dryRun });
    toast(store.dryRun ? `Would free ${bytes(rep.freed)}` : `Freed ${bytes(rep.freed)}${rep.failed ? `, ${rep.failed} failed` : ''}`, rep.failed ? 'warn' : 'info');
    if (!store.dryRun) await scan();
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
      <template #title>Projects</template>
      <template #lead>Build folders you can make again: node_modules, Rust target, Python virtual environments and more. A folder only counts when the project's own file (package.json, Cargo.toml, …) is next to it.</template>
      <template #actions>
        <button class="btn" @click="add"><Icon name="plus" />Add folder</button>
        <button class="btn" :disabled="busy" @click="scan"><Icon name="refresh" />Scan again</button>
      </template>
    </PageHead>
    <div class="page-body">
      <div v-if="busy && !scanned" class="empty"><h2>Looking through your projects…</h2></div>
      <div v-else-if="!items.length" class="empty"><h2>No build folders found</h2><p>Looked in your usual project folders. Use “Add folder” to search another one.</p></div>
      <template v-else>
        <p class="muted" style="margin-bottom: 8px">{{ items.length }} folders · {{ bytes(all) }}. Folders of projects idle for {{ store.settings.purge_idle_days }}+ days are ticked.</p>
        <table class="table selectable">
          <thead><tr><th style="width: 34px" /><th>Project</th><th>Kind</th><th class="num">Idle</th><th class="num">Size</th><th style="width: 40px" /></tr></thead>
          <tbody>
            <tr v-for="a in items" :key="a.path" style="cursor: pointer" @click="toggle(a.path)">
              <td><input type="checkbox" :checked="picked.has(a.path)" @click.stop @change="toggle(a.path)" :aria-label="a.path" /></td>
              <td class="mono ellipsis" style="max-width: 420px" :title="a.path">{{ shortPath(a.project, store.home) }}<span class="muted">/{{ a.path.split('/').pop() }}</span></td>
              <td><span class="badge">{{ a.kind }}</span></td>
              <td class="num muted">{{ a.age_days }} d</td>
              <td class="num"><strong>{{ bytes(a.size) }}</strong></td>
              <td><button class="btn ghost icon" title="Show in the file manager" @click.stop="reveal(a.path)"><Icon name="reveal" /></button></td>
            </tr>
          </tbody>
        </table>
      </template>
    </div>
    <footer class="page-foot">
      <div class="grow"><strong>{{ bytes(total) }}</strong><span class="muted"> selected · {{ picked.size }} folders</span></div>
      <button class="btn" @click="picked.clear()" :disabled="!picked.size">Clear</button>
      <button class="btn danger" :disabled="!picked.size || busy" @click="apply"><Icon name="trash" />{{ store.dryRun ? 'Preview' : 'Delete selected' }}</button>
    </footer>
  </div>
</template>
