<script setup>
import { computed, onMounted, ref, watch } from 'vue';
import { call } from '../api.js';
import { bytes, dateTime } from '../format.js';
import { confirm, errorText, toast } from '../store.js';
import Icon from '../components/Icon.vue';
import PageHead from '../components/PageHead.vue';

const props = defineProps({ active: Boolean });
const entries = ref([]);

async function load() {
  try {
    entries.value = await call('history_list', { limit: 500 });
  } catch (e) {
    toast(errorText(e), 'error');
  }
}
onMounted(load);
watch(() => props.active, (a) => a && load());

const freed = computed(() => entries.value.filter((e) => !e.dry_run).reduce((a, e) => a + e.freed, 0));
const icons = { clean: 'clean', purge: 'purge', duplicates: 'copy', 'large-files': 'large', apps: 'apps', startup: 'startup', maintenance: 'wrench' };

async function clear() {
  if (!(await confirm({ title: 'Clear the history?', body: 'Only the list is erased. Nothing on disk changes.', confirmLabel: 'Clear' }))) return;
  await call('history_clear');
  entries.value = [];
}
</script>

<template>
  <div class="page">
    <PageHead>
      <template #title>History</template>
      <template #lead>Every cleanup and system change Pumice made, with the space it freed.</template>
      <template #actions><button class="btn" :disabled="!entries.length" @click="clear">Clear history</button></template>
    </PageHead>
    <div class="page-body">
      <div v-if="!entries.length" class="empty"><h2>Nothing yet</h2><p>Cleanups you run will be listed here.</p></div>
      <template v-else>
        <div class="card" style="margin-bottom: 14px"><span class="muted">Freed so far</span> <strong style="font-size: 22px; margin-left: 8px">{{ bytes(freed) }}</strong></div>
        <div v-for="(e, i) in entries" :key="i" class="card h row">
          <Icon :name="icons[e.kind] || 'file'" class="muted" />
          <div class="grow" style="min-width: 0">
            <strong>{{ e.title }}</strong> <span v-if="e.dry_run" class="badge ok">dry run</span>
            <p class="muted">{{ dateTime(e.ts) }} · {{ e.ok }} done<span v-if="e.failed">, {{ e.failed }} failed</span></p>
            <p v-for="d in e.details.slice(0, 3)" :key="d" class="muted mono ellipsis selectable">{{ d }}</p>
          </div>
          <strong v-if="e.freed">{{ bytes(e.freed) }}</strong>
        </div>
      </template>
    </div>
  </div>
</template>

<style scoped>
.h { margin-bottom: 8px; padding: 12px 16px; }
</style>
