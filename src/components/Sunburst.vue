<script setup>
import { computed, ref } from 'vue';
import { bytes } from '../format.js';

const props = defineProps({ root: { type: Object, required: true }, depth: { type: Number, default: 3 } });
const emit = defineEmits(['open']);
const hover = ref(null);

const R0 = 62;
const RING = computed(() => (250 - R0) / props.depth);

function hue(name) {
  let h = 0;
  for (let i = 0; i < name.length; i++) h = (h * 31 + name.charCodeAt(i)) >>> 0;
  return h % 360;
}

function point(r, a) {
  return [Math.cos(a - Math.PI / 2) * r, Math.sin(a - Math.PI / 2) * r];
}

function arc(r1, r2, a1, a2) {
  const large = a2 - a1 > Math.PI ? 1 : 0;
  const [x1, y1] = point(r2, a1);
  const [x2, y2] = point(r2, a2);
  const [x3, y3] = point(r1, a2);
  const [x4, y4] = point(r1, a1);
  return `M${x1},${y1} A${r2},${r2} 0 ${large} 1 ${x2},${y2} L${x3},${y3} A${r1},${r1} 0 ${large} 0 ${x4},${y4} Z`;
}

const arcs = computed(() => {
  const out = [];
  const walk = (node, a1, a2, level, baseHue) => {
    const kids = (node.children || []).filter((c) => c.size > 0);
    const total = kids.reduce((a, c) => a + c.size, 0);
    if (!total || level >= props.depth) return;
    let a = a1;
    kids.forEach((c, i) => {
      const span = ((a2 - a1) * c.size) / total;
      const h = level === 0 ? hue(c.name) : baseHue;
      if (span > 0.006) {
        out.push({
          n: c,
          d: arc(R0 + level * RING.value + 1, R0 + (level + 1) * RING.value - 1, a, a + span - 0.002),
          fill: c.id == null ? 'var(--line)' : `hsl(${h} 38% ${58 - level * 7}%)`,
        });
        walk(c, a, a + span, level + 1, h);
      }
      a += span;
    });
  };
  walk(props.root, 0, Math.PI * 2 - 0.0001, 0, 0);
  return out;
});

const centre = computed(() => hover.value || props.root);
</script>

<template>
  <div class="sb">
    <svg viewBox="-260 -260 520 520" role="img" aria-label="Sunburst chart of folder sizes" @mouseleave="hover = null">
      <path v-for="(a, i) in arcs" :key="i" :d="a.d" :fill="a.fill" class="arc" :class="{ click: a.n.id != null && a.n.is_dir }" @mouseenter="hover = a.n" @click="emit('open', a.n)" />
      <text y="-4" text-anchor="middle" class="t1">{{ bytes(centre.size) }}</text>
      <text y="14" text-anchor="middle" class="t2">{{ centre.name.length > 16 ? centre.name.slice(0, 15) + '…' : centre.name }}</text>
    </svg>
    <p class="muted info">{{ hover ? `${hover.name} · ${bytes(hover.size)}` : 'Click a ring segment to open that folder.' }}</p>
  </div>
</template>

<style scoped>
.sb { display: flex; flex-direction: column; align-items: center; }
svg { width: min(100%, 520px); max-height: 62vh; height: auto; }
.arc { stroke: var(--bg); stroke-width: 0.6; }
.arc.click { cursor: pointer; }
.arc:hover { filter: brightness(1.12); }
.t1 { font-size: 15px; font-weight: 700; fill: var(--text); }
.t2 { font-size: 11px; fill: var(--muted); }
.info { min-height: 20px; margin-top: 4px; }
</style>
