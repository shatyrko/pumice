<script setup>
import { computed, onMounted, reactive, ref } from 'vue';
import { call, reveal } from '../api.js';
import { bytes, shortPath } from '../format.js';
import { confirm, errorText, store, toast } from '../store.js';
import Icon from '../components/Icon.vue';
import PageHead from '../components/PageHead.vue';

defineProps({ active: Boolean });

const categories = {
  caches: { title: 'Caches and trash', icon: 'clean' },
  browsers: { title: 'Browsers', icon: 'search' },
  developer: { title: 'Developer tools', icon: 'purge' },
  packages: { title: 'Packages', icon: 'archive' },
  ai: { title: 'AI models', icon: 'cpu' },
  logs: { title: 'Logs and crash reports', icon: 'file' },
  privacy: { title: 'Privacy', icon: 'lock' },
};

const results = ref([]);
const busy = ref(false);
const scanned = ref(false);
const applying = ref(false);
const expanded = reactive({});
// rule id -> Set of selected paths
const picked = reactive({});
const last = ref(null);

async function scan() {
  busy.value = true;
  last.value = null;
  try {
    results.value = await call('clean_scan', { only: null });
    for (const k of Object.keys(picked)) delete picked[k];
    for (const r of results.value) {
      picked[r.id] = new Set(r.selected && !r.blocked ? r.items.map((i) => i.path) : []);
    }
    scanned.value = true;
  } catch (e) {
    toast(errorText(e), 'error');
  } finally {
    busy.value = false;
  }
}

onMounted(scan);

const groups = computed(() => {
  const by = {};
  for (const r of results.value) (by[r.category] ||= []).push(r);
  return Object.entries(by)
    .map(([key, rules]) => ({ key, ...(categories[key] || { title: key, icon: 'folder' }), rules, total: rules.reduce((a, r) => a + r.total, 0) }))
    .sort((a, b) => b.total - a.total);
});

function ruleSelectedSize(r) {
  const s = picked[r.id];
  if (!s) return 0;
  return r.items.filter((i) => s.has(i.path)).reduce((a, i) => a + i.size, 0);
}
const totalSelected = computed(() => results.value.reduce((a, r) => a + ruleSelectedSize(r), 0));
const countSelected = computed(() => results.value.reduce((a, r) => a + (picked[r.id]?.size || 0), 0));

function ruleState(r) {
  const n = picked[r.id]?.size || 0;
  return n === 0 ? 'none' : n === r.items.length ? 'all' : 'some';
}
function toggleRule(r) {
  if (r.blocked) return;
  picked[r.id] = ruleState(r) === 'all' ? new Set() : new Set(r.items.map((i) => i.path));
}
function toggleItem(r, path) {
  const s = new Set(picked[r.id]);
  s.has(path) ? s.delete(path) : s.add(path);
  picked[r.id] = s;
}
function groupSelect(g, on) {
  for (const r of g.rules) if (!r.blocked) picked[r.id] = on ? new Set(r.items.map((i) => i.path)) : new Set();
}

async function apply() {
  const requests = results.value.filter((r) => picked[r.id]?.size).map((r) => ({ rule_id: r.id, paths: [...picked[r.id]] }));
  if (!requests.length) return;
  const risky = results.value.filter((r) => picked[r.id]?.size && r.risk !== 'low');
  const dry = store.dryRun;
  const ok = await confirm({
    title: dry ? 'Preview this cleanup?' : `Remove ${bytes(totalSelected.value)}?`,
    body: dry
      ? 'Dry run is on: nothing will be removed, you will only see what would be freed.'
      : `${countSelected.value} items from ${requests.length} groups will be deleted for good. Files that were in use are skipped.${risky.length ? '\n\nSome selected groups need review: ' + risky.map((r) => r.name).join(', ') + '.' : ''}`,
    confirmLabel: dry ? 'Preview' : 'Clean',
    danger: !dry,
  });
  if (!ok) return;
  applying.value = true;
  try {
    const rep = await call('clean_apply', { requests, dryRun: dry });
    last.value = rep;
    toast(dry ? `Would free ${bytes(rep.freed)} (${rep.removed} items)` : `Freed ${bytes(rep.freed)}${rep.failed ? `, ${rep.failed} could not be removed` : ''}`, rep.failed ? 'warn' : 'info');
    if (!dry) await scan();
  } catch (e) {
    toast(errorText(e), 'error');
  } finally {
    applying.value = false;
  }
}
</script>

<template>
  <div class="page">
    <PageHead>
      <template #title>Clean</template>
      <template #lead>Caches, logs, package and tool downloads, and privacy leftovers. Nothing is removed until you press Clean, and rules never touch your own folders.</template>
      <template #actions>
        <button class="btn" :disabled="busy" @click="scan"><Icon name="refresh" />{{ busy ? 'Looking…' : 'Scan again' }}</button>
      </template>
    </PageHead>

    <div class="page-body">
      <div v-if="busy && !scanned" class="empty"><h2>Looking for things to clean…</h2></div>
      <div v-else-if="scanned && !groups.length" class="empty"><h2>Nothing to clean</h2><p>Your caches are already small.</p></div>

      <div v-for="g in groups" :key="g.key" class="card group">
        <div class="row ghead">
          <Icon :name="g.icon" />
          <h2 class="grow">{{ g.title }}</h2>
          <span class="muted">{{ bytes(g.total) }}</span>
          <button class="btn ghost small" @click="groupSelect(g, true)">All</button>
          <button class="btn ghost small" @click="groupSelect(g, false)">None</button>
        </div>
        <div v-for="r in g.rules" :key="r.id" class="rule" :class="{ blocked: r.blocked }">
          <div class="row">
            <input type="checkbox" :checked="ruleState(r) === 'all'" :indeterminate.prop="ruleState(r) === 'some'" :disabled="!!r.blocked" @change="toggleRule(r)" :aria-label="r.name" />
            <button class="linklike grow" @click="expanded[r.id] = !expanded[r.id]">
              <Icon name="chevron" :style="{ transform: expanded[r.id] ? 'rotate(90deg)' : 'none' }" />
              <span class="name">{{ r.name }}</span>
              <span class="badge" :class="r.risk">{{ r.risk === 'low' ? 'safe' : r.risk === 'medium' ? 'review' : 'careful' }}</span>
            </button>
            <span class="muted num">{{ r.items.length }} items</span>
            <strong class="num">{{ bytes(r.total) }}</strong>
          </div>
          <p class="muted desc">{{ r.description }}<span v-if="r.note"> {{ r.note }}</span></p>
          <p v-if="r.blocked" class="blocked-note"><Icon name="warn" /> {{ r.blocked }}</p>
          <div v-if="expanded[r.id]" class="items">
            <label v-for="i in r.items.slice(0, 200)" :key="i.path" class="item row">
              <input type="checkbox" :checked="picked[r.id]?.has(i.path)" :disabled="!!r.blocked" @change="toggleItem(r, i.path)" />
              <span class="mono ellipsis grow selectable" :title="i.path">{{ shortPath(i.path, store.home) }}</span>
              <button class="btn ghost icon" title="Show in the file manager" @click.prevent="reveal(i.path)"><Icon name="reveal" /></button>
              <span class="muted num">{{ bytes(i.size) }}</span>
            </label>
            <p v-if="r.items.length > 200" class="muted">and {{ r.items.length - 200 }} more (they follow the group's checkbox).</p>
          </div>
        </div>
      </div>

      <div v-if="last" class="card" style="margin-top: 14px">
        <h2>{{ last.dry_run ? 'Preview' : 'Result' }}: {{ bytes(last.freed) }} {{ last.dry_run ? 'would be freed' : 'freed' }}</h2>
        <p class="muted">{{ last.removed }} items {{ last.dry_run ? 'checked' : 'removed' }}, {{ last.failed }} skipped.</p>
        <ul v-if="last.outcomes.some((o) => o.error)" class="muted" style="max-height: 140px; overflow: auto">
          <li v-for="o in last.outcomes.filter((o) => o.error).slice(0, 30)" :key="o.path" class="selectable"><span class="mono">{{ shortPath(o.path, store.home) }}</span>: {{ o.error }}</li>
        </ul>
      </div>
    </div>

    <footer class="page-foot">
      <div class="grow">
        <strong>{{ bytes(totalSelected) }}</strong>
        <span class="muted"> selected · {{ countSelected }} items</span>
      </div>
      <span v-if="store.dryRun" class="badge ok">Dry run</span>
      <button class="btn primary" :disabled="!countSelected || applying" @click="apply"><Icon name="trash" />{{ applying ? 'Cleaning…' : store.dryRun ? 'Preview cleanup' : 'Clean selected' }}</button>
    </footer>
  </div>
</template>

<style scoped>
.group { margin-bottom: 14px; padding: 6px 16px 10px; }
.ghead { padding: 10px 0 6px; border-bottom: 1px solid var(--line); }
.rule { padding: 10px 0; border-top: 1px solid var(--line); }
.rule:first-of-type { border-top: 0; }
.rule.blocked { opacity: 0.7; }
.desc { margin: 2px 0 0 26px; font-size: 13px; }
.blocked-note { margin: 4px 0 0 26px; color: var(--warn); display: flex; align-items: center; gap: 6px; }
.linklike { display: flex; align-items: center; gap: 8px; background: none; border: 0; color: var(--text); font: inherit; cursor: pointer; text-align: left; padding: 0; }
.name { font-weight: 600; }
.num { white-space: nowrap; font-variant-numeric: tabular-nums; }
.items { margin: 8px 0 0 26px; border-left: 2px solid var(--line); padding-left: 12px; }
.item { padding: 3px 0; cursor: pointer; }
</style>
