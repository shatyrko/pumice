<script setup>
import { onMounted, onUnmounted, ref } from 'vue';
import { store } from '../store.js';
import Icon from './Icon.vue';

const okButton = ref(null);

function close(result) {
  const d = store.dialog;
  store.dialog = null;
  d?.resolve(result);
}

function onKey(e) {
  if (!store.dialog) return;
  if (e.key === 'Escape') close(false);
}

onMounted(() => {
  window.addEventListener('keydown', onKey);
  okButton.value?.focus();
});
onUnmounted(() => window.removeEventListener('keydown', onKey));
</script>

<template>
  <div class="scrim" @mousedown.self="close(false)">
    <div class="dialog" role="dialog" aria-modal="true">
      <div class="row">
        <Icon :name="store.dialog.danger ? 'warn' : 'check'" :style="{ color: store.dialog.danger ? 'var(--danger)' : 'var(--accent)' }" />
        <h2>{{ store.dialog.title }}</h2>
      </div>
      <div v-if="store.dialog.body" class="muted" style="white-space: pre-line">{{ store.dialog.body }}</div>
      <ul v-if="store.dialog.lines?.length" class="muted" style="margin: 0; padding-left: 18px; max-height: 180px; overflow: auto">
        <li v-for="(l, i) in store.dialog.lines" :key="i" class="ellipsis">{{ l }}</li>
      </ul>
      <label v-if="store.dialog.extra" class="row" style="gap: 8px">
        <input type="checkbox" v-model="store.dialog.extra.value" />
        <span>{{ store.dialog.extra.label }}</span>
      </label>
      <div class="row" style="justify-content: flex-end">
        <button class="btn" @click="close(false)">Cancel</button>
        <button ref="okButton" class="btn" :class="store.dialog.danger ? 'danger' : 'primary'" @click="close(true)">
          {{ store.dialog.confirmLabel || 'Continue' }}
        </button>
      </div>
    </div>
  </div>
</template>
