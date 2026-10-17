<script setup>
import { computed } from 'vue';

// Two optional series, each already scaled to 0..1.
const props = defineProps({
  a: { type: Array, default: () => [] },
  b: { type: Array, default: () => [] },
  colorA: { type: String, default: 'var(--accent)' },
  colorB: { type: String, default: 'var(--warn)' },
  caption: { type: String, default: '' },
  points: { type: Number, default: 90 },
});

function poly(values) {
  if (values.length < 2) return '';
  const n = props.points - 1;
  const off = props.points - values.length;
  return values.map((v, i) => `${(((i + off) / n) * 100).toFixed(2)},${(31 - Math.min(1, Math.max(0, v)) * 29).toFixed(2)}`).join(' ');
}
const pa = computed(() => poly(props.a));
const pb = computed(() => poly(props.b));
const area = computed(() => (pa.value ? `${pa.value} 100,31 ${((props.points - props.a.length) / (props.points - 1)) * 100},31` : ''));
</script>

<template>
  <figure class="g">
    <svg viewBox="0 0 100 32" preserveAspectRatio="none" role="img" :aria-label="caption">
      <line x1="0" x2="100" y1="16" y2="16" stroke="var(--line)" stroke-width="1" vector-effect="non-scaling-stroke" stroke-dasharray="3 4" />
      <polygon v-if="area" :points="area" :fill="colorA" fill-opacity="0.16" />
      <polyline v-if="pa" :points="pa" fill="none" :stroke="colorA" stroke-width="1.6" vector-effect="non-scaling-stroke" stroke-linejoin="round" />
      <polyline v-if="pb" :points="pb" fill="none" :stroke="colorB" stroke-width="1.6" vector-effect="non-scaling-stroke" stroke-linejoin="round" />
    </svg>
    <figcaption v-if="caption" class="muted">{{ caption }}</figcaption>
  </figure>
</template>

<style scoped>
.g { margin: 0; }
svg { display: block; width: 100%; height: 64px; background: var(--panel-2); border-radius: 8px; }
figcaption { font-size: 12px; margin-top: 3px; }
</style>
