<script setup>
import { onMounted, reactive, ref } from 'vue';
import { call } from '../api.js';
import { bytes } from '../format.js';
import { confirm, errorText, store, toast } from '../store.js';
import Icon from '../components/Icon.vue';
import Explain from '../components/Explain.vue';
import PageHead from '../components/PageHead.vue';

defineProps({ active: Boolean });

const checks = ref([]);
const loading = ref(true);
const output = ref('');
const running = ref('');
const picked = reactive({});

async function load() {
  loading.value = true;
  try {
    await call('helper_status').then((h) => (store.helper = h));
    checks.value = await call('maintenance_checks');
    for (const c of checks.value) if (c.id === 'orphans') picked[c.id] = new Set(c.items);
  } catch (e) {
    toast(errorText(e), 'error');
  } finally {
    loading.value = false;
  }
}
onMounted(load);

async function run(c, a) {
  const args = c.id === 'orphans' ? [...(picked[c.id] || [])] : c.id === 'smart' ? c.items : [];
  const ok = await confirm({
    title: a.label + '?',
    body: a.confirm + (a.privileged ? '\nYou will be asked for your password.' : ''),
    lines: c.id === 'orphans' ? args : undefined,
    confirmLabel: store.dryRun ? 'Close' : 'Run',
    danger: a.id !== 'smart' && a.id !== 'fc-cache' && a.id !== 'desktop-db',
  });
  if (!ok) return;
  if (store.dryRun) return toast('Dry run is on: nothing was changed.');
  running.value = a.id;
  try {
    output.value = await call('maintenance_run', { id: a.id, args });
    toast(`${a.label}: done`);
    if (!['smart'].includes(a.id)) await load();
  } catch (e) {
    output.value = errorText(e);
    toast(errorText(e), 'error', 9000);
  } finally {
    running.value = '';
  }
}

const dot = { ok: 'var(--ok)', warn: 'var(--warn)', info: 'var(--muted)', na: 'var(--line)' };
</script>

<template>
  <div class="page">
    <PageHead>
      <template #title>Maintenance</template>
      <template #lead>Health checks and safe repairs for the system. Checks only read. Actions that need administrator rights ask for your password through polkit.</template>
      <template #actions><button class="btn" :disabled="loading" @click="load"><Icon name="refresh" />Check again</button></template>
    </PageHead>
    <div class="page-body">
      <div v-if="!store.helper.installed" class="notice" style="margin-bottom: 14px"><Icon name="lock" style="vertical-align: -3px" /> The administrator helper is not installed. Checks work, but actions marked with a lock cannot run yet. Run <span class="mono">scripts/install-helper.sh</span> once (it asks for sudo).</div>
      <div v-for="c in checks" :key="c.id" class="card chk">
        <div class="row">
          <span class="dot" :style="{ background: dot[c.status] }" />
          <h2 class="grow">{{ c.title }}</h2>
          <strong v-if="c.size != null">{{ bytes(c.size) }}</strong>
        </div>
        <p class="muted">{{ c.detail }}</p>
        <div><Explain topic="system maintenance check" :facts="`Check: ${c.title}\nResult: ${c.detail}\nExamples: ${c.items.slice(0, 6).join(', ')}`" /></div>
        <div v-if="c.items.length" class="items selectable">
          <template v-if="c.id === 'orphans'">
            <label v-for="i in c.items" :key="i" class="row"><input type="checkbox" :checked="picked[c.id]?.has(i)" @change="picked[c.id]?.has(i) ? picked[c.id].delete(i) : picked[c.id].add(i)" /><span class="mono">{{ i }}</span></label>
          </template>
          <div v-else v-for="i in c.items" :key="i" class="mono">{{ i }}</div>
        </div>
        <div v-if="c.actions.length" class="row wrap">
          <button v-for="a in c.actions" :key="a.id" class="btn small" :disabled="!!running || (a.privileged && !store.helper.installed) || (c.id === 'orphans' && !picked[c.id]?.size)" @click="run(c, a)">
            <Icon :name="a.privileged ? 'lock' : 'wrench'" style="width: 15px; height: 15px" />{{ running === a.id ? 'Working…' : a.label }}
          </button>
        </div>
      </div>
      <div v-if="output" class="card" style="margin-top: 14px">
        <div class="row"><h2 class="grow">Output</h2><button class="btn ghost small" @click="output = ''">Clear</button></div>
        <pre class="mono selectable" style="white-space: pre-wrap; margin: 8px 0 0; max-height: 260px; overflow: auto">{{ output }}</pre>
      </div>
    </div>
  </div>
</template>

<style scoped>
.chk { margin-bottom: 12px; display: flex; flex-direction: column; gap: 8px; }
.dot { width: 10px; height: 10px; border-radius: 50%; flex: none; }
.items { max-height: 180px; overflow: auto; padding: 8px 10px; background: var(--panel-2); border-radius: 8px; display: flex; flex-direction: column; gap: 3px; }
</style>
