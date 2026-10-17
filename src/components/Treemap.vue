<script setup>
import { computed, onMounted, onUnmounted, ref } from 'vue';
import { bytes } from '../format.js';

const props = defineProps({ root: { type: Object, required: true } });
const emit = defineEmits(['open']);

const box = ref(null);
const size = ref({ w: 800, h: 520 });
const hover = ref(null);
let ro = null;

onMounted(() => {
  ro = new ResizeObserver(([e]) => {
    size.value = { w: Math.max(200, e.contentRect.width), h: Math.max(200, Math.min(640, window.innerHeight - 330)) };
  });
  ro.observe(box.value);
});
onUnmounted(() => ro && ro.disconnect());

function worst(row, sum, side) {
  const max = Math.max(...row);
  const min = Math.min(...row);
  return Math.max((side * side * max) / (sum * sum), (sum * sum) / (side * side * min));
}

// Squarified treemap (Bruls, Huizing, van Wijk).
function squarify(items, x, y, w, h) {
  const total = items.reduce((a, n) => a + n.size, 0);
  if (total <= 0 || w <= 0 || h <= 0) return [];
  const scale = (w * h) / total;
  const out = [];
  let i = 0;
  while (i < items.length) {
    const side = Math.min(w, h);
    let row = [items[i].size * scale];
    let sum = row[0];
    let j = i + 1;
    while (j < items.length) {
      const a = items[j].size * scale;
      if (worst([...row, a], sum + a, side) <= worst(row, sum, side)) {
        row.push(a);
        sum += a;
        j++;
      } else break;
    }
    const thick = sum / side;
    let off = 0;
    for (let k = 0; k < row.length; k++) {
      const len = row[k] / thick;
      if (w >= h) out.push({ n: items[i + k], x, y: y + off, w: thick, h: len });
      else out.push({ n: items[i + k], x: x + off, y, w: len, h: thick });
      off += len;
    }
    if (w >= h) {
      x += thick;
      w -= thick;
    } else {
      y += thick;
      h -= thick;
    }
    i = j;
  }
  return out;
}

function hue(name) {
  let h = 0;
  for (let i = 0; i < name.length; i++) h = (h * 31 + name.charCodeAt(i)) >>> 0;
  return h % 360;
}

const dark = window.matchMedia('(prefers-color-scheme: dark)').matches;

const cells = computed(() => {
  const out = [];
  const walk = (node, x, y, w, h, depth) => {
    const kids = (node.children || []).filter((c) => c.size > 0);
    const rects = squarify(kids, x, y, w, h);
    for (const r of rects) {
      out.push({ ...r, depth });
      const c = r.n;
      if (c.children?.length && r.w > 64 && r.h > 46) walk(c, r.x + 2, r.y + 17, r.w - 4, r.h - 19, depth + 1);
    }
  };
  walk(props.root, 0, 0, size.value.w, size.value.h, 0);
  return out;
});

function style(c) {
  const n = c.n;
  const gray = n.id == null;
  const l = dark ? 30 + c.depth * 7 : 66 - c.depth * 6;
  return {
    left: c.x + 'px',
    top: c.y + 'px',
    width: Math.max(0, c.w - 1) + 'px',
    height: Math.max(0, c.h - 1) + 'px',
    background: gray ? (dark ? '#3a4448' : '#c9d0d1') : n.is_dir ? `hsl(${hue(n.name)} 34% ${l}%)` : `hsl(${hue(n.name)} 18% ${dark ? 24 : 78}%)`,
    cursor: n.id != null && n.is_dir ? 'pointer' : 'default',
  };
}
</script>

<template>
  <div>
    <div ref="box" class="tm" :style="{ height: size.h + 'px' }" @mouseleave="hover = null">
      <div
        v-for="(c, i) in cells"
        :key="i"
        class="cell"
        :style="style(c)"
        @mouseenter="hover = c.n"
        @click.stop="emit('open', c.n)"
      >
        <span v-if="c.w > 56 && c.h > 20" class="label">{{ c.n.name }}<small v-if="c.h > 36"> {{ bytes(c.n.size) }}</small></span>
      </div>
    </div>
    <p class="muted info">{{ hover ? `${hover.name} · ${bytes(hover.size)}` : 'Click a folder to open it.' }}</p>
  </div>
</template>

<style scoped>
.tm { position: relative; width: 100%; background: var(--panel-2); border-radius: 8px; overflow: hidden; }
.cell { position: absolute; overflow: hidden; border-radius: 3px; box-sizing: border-box; }
.cell:hover { outline: 2px solid var(--accent); outline-offset: -2px; z-index: 2; }
.label { display: block; padding: 2px 5px; font-size: 12px; line-height: 14px; color: var(--text); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; pointer-events: none; }
.label small { opacity: 0.7; }
.info { margin-top: 8px; min-height: 20px; }
</style>
