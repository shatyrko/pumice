<script setup>
import { ref } from 'vue';
import { call } from '../api.js';
import { errorText, go, toast } from '../store.js';
import Icon from './Icon.vue';

// A small "Explain" button: asks the connected AI service about one item and shows the answer under it.
const props = defineProps({ topic: String, facts: String });
const text = ref('');
const busy = ref(false);
const failed = ref(false);
let lastFacts = null;

async function ask() {
  if (busy.value) return;
  if (text.value && lastFacts === props.facts) {
    text.value = '';
    return;
  }
  const st = await call('ai_status');
  if (!st.configured) {
    toast('Set up AI explanations in Settings first.', 'warn');
    go('settings');
    return;
  }
  busy.value = true;
  failed.value = false;
  try {
    text.value = await call('ai_explain', { topic: props.topic, facts: props.facts });
    lastFacts = props.facts;
  } catch (e) {
    failed.value = true;
    text.value = errorText(e);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <span class="ex">
    <button class="btn ghost small" :disabled="busy" title="Ask the connected AI service what this is" @click.stop.prevent="ask">
      <Icon name="clean" style="width: 15px; height: 15px" />{{ busy ? 'Asking…' : 'Explain' }}
    </button>
    <span v-if="text" class="answer selectable" :class="{ bad: failed }" @click.stop>{{ text }}</span>
  </span>
</template>

<style scoped>
.ex { display: inline-flex; flex-direction: column; align-items: flex-start; gap: 4px; max-width: 100%; }
.answer { display: block; padding: 8px 10px; border-radius: 8px; background: var(--accent-soft); font-size: 13px; line-height: 1.45; max-width: 62ch; user-select: text; white-space: normal; }
.answer.bad { background: var(--danger-soft); }
</style>
