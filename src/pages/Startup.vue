<script setup>
import { computed, onMounted, ref } from 'vue';
import { call } from '../api.js';
import { errorText, toast } from '../store.js';
import Icon from '../components/Icon.vue';
import Explain from '../components/Explain.vue';
import PageHead from '../components/PageHead.vue';

defineProps({ active: Boolean });

const entries = ref([]);
const boot = ref(null);
const loading = ref(true);
const tab = ref('autostart');

async function load() {
  loading.value = true;
  try {
    [entries.value, boot.value] = await Promise.all([call('startup_list'), call('boot_info')]);
  } catch (e) {
    toast(errorText(e), 'error');
  } finally {
    loading.value = false;
  }
}
onMounted(load);

const shown = computed(() => entries.value.filter((e) => (tab.value === 'autostart' ? e.kind === 'autostart' : e.kind !== 'autostart')));
const maxMs = computed(() => Math.max(1, ...(boot.value?.slowest || []).map((b) => b.millis)));

async function flip(e) {
  const next = !e.enabled;
  try {
    await call('startup_set', { id: e.id, enabled: next });
    e.enabled = next;
    toast(`${e.name} ${next ? 'will start' : 'will not start'} with your session.`);
  } catch (err) {
    toast(errorText(err), 'error');
  }
}
</script>

<template>
  <div class="page">
    <PageHead>
      <template #title>Startup</template>
      <template #lead>Programs and services that start when you log in. Switching one off is reversible: nothing is deleted.</template>
      <template #actions><button class="btn" :disabled="loading" @click="load"><Icon name="refresh" />Refresh</button></template>
    </PageHead>
    <div class="bar-row">
      <button class="chip" :class="{ on: tab === 'autostart' }" @click="tab = 'autostart'">Programs</button>
      <button class="chip" :class="{ on: tab === 'services' }" @click="tab = 'services'">User services</button>
      <button class="chip" :class="{ on: tab === 'boot' }" @click="tab = 'boot'">Boot time</button>
    </div>
    <div class="page-body">
      <template v-if="tab !== 'boot'">
        <div v-if="!loading && !shown.length" class="empty"><h2>Nothing here</h2></div>
        <div v-for="e in shown" :key="e.id" class="card item row">
          <label class="switch"><input type="checkbox" :checked="e.enabled" @change="flip(e)" :aria-label="e.name" /><span /></label>
          <div class="grow" style="min-width: 0">
            <strong>{{ e.name }}</strong> <span class="badge">{{ e.scope }}</span> <span v-if="e.kind === 'timer'" class="badge">timer</span>
            <p class="muted ellipsis">{{ e.comment || e.command }}</p>
            <p v-if="e.comment && e.command" class="muted mono ellipsis selectable">{{ e.command }}</p>
            <Explain topic="program or service that starts with the desktop session" :facts="`Name: ${e.name}\nKind: ${e.kind}\nDescription: ${e.comment}\nCommand: ${e.command}`" />
          </div>
        </div>
      </template>
      <template v-else>
        <div v-if="!boot?.summary" class="empty"><h2>No boot data</h2><p>systemd-analyze is not available.</p></div>
        <template v-else>
          <div class="card" style="margin-bottom: 14px"><h2>{{ boot.summary }}</h2></div>
          <div class="card">
            <h2 style="margin-bottom: 8px">Slowest units at boot</h2>
            <div v-for="b in boot.slowest" :key="b.unit" class="b row">
              <span class="mono ellipsis" style="width: 280px">{{ b.unit }}</span>
              <div class="bar grow"><i :style="{ width: (b.millis / maxMs) * 100 + '%' }" /></div>
              <span class="muted num">{{ (b.millis / 1000).toFixed(2) }} s</span>
            </div>
            <p class="muted" style="margin-top: 8px">Many units start in parallel, so these times do not add up to the total.</p>
          </div>
        </template>
      </template>
    </div>
  </div>
</template>

<style scoped>
.bar-row { display: flex; gap: 10px; align-items: center; padding: 0 28px 12px; }
.item { margin-bottom: 8px; padding: 12px 16px; }
.b { padding: 4px 0; }
.num { width: 64px; text-align: right; font-variant-numeric: tabular-nums; }
</style>
