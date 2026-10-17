<script setup>
defineProps({
  label: String,
  value: String,
  frac: { type: Number, default: 0 },
  level: { type: String, default: 'ok' },
  color: String,
  labelWidth: { type: Number, default: 150 },
});
</script>

<template>
  <div class="m">
    <span class="l ellipsis muted" :style="{ width: labelWidth + 'px' }" :title="label">{{ label }}</span>
    <div class="bar"><i :style="{ width: Math.min(100, Math.max(1, frac * 100)) + '%', background: level === 'crit' ? 'var(--danger)' : level === 'warn' ? 'var(--warn)' : color || 'var(--accent)' }" /></div>
    <span class="v" :class="level">{{ value }}</span>
  </div>
</template>

<style scoped>
.m { display: flex; align-items: center; gap: 10px; min-height: 22px; }
.l { flex: none; font-size: 13px; }
.bar { flex: 1; }
.v { flex: none; min-width: 84px; text-align: right; font-weight: 600; font-variant-numeric: tabular-nums; font-size: 13px; }
.v.warn { color: var(--warn); }
.v.crit { color: var(--danger); }
</style>
