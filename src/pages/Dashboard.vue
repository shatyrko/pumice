<script setup>
import { computed, onMounted, onUnmounted, reactive, ref, watch } from 'vue';
import { call } from '../api.js';
import { bytes, dur, hours, level as lvl, mb, mhz, pct, rate, temp, watts } from '../format.js';
import { go, store } from '../store.js';
import Graph from '../components/Graph.vue';
import Icon from '../components/Icon.vue';
import Meter from '../components/Meter.vue';
import PageHead from '../components/PageHead.vue';

const props = defineProps({ active: Boolean });
const s = ref(null);
const reclaim = ref(null);
const reclaimBusy = ref(false);
const procSort = ref('cpu');
let timer = null;

function loadFolded() {
  try {
    return JSON.parse(localStorage.getItem('pumice.folded') || '{}');
  } catch {
    return {};
  }
}
const folded = reactive(loadFolded());
function fold(key) {
  folded[key] = !folded[key];
  try {
    localStorage.setItem('pumice.folded', JSON.stringify(folded));
  } catch {
    // Not remembered in a private window.
  }
}

async function tick() {
  try {
    const v = await call('system_snapshot');
    if (v.ok) s.value = v;
  } catch {
    // The next tick tries again.
  }
}

async function findReclaimable() {
  reclaimBusy.value = true;
  try {
    const res = await call('clean_scan', { only: null });
    reclaim.value = {
      total: res.filter((r) => r.risk === 'low' && !r.blocked).reduce((a, r) => a + r.total, 0),
      all: res.reduce((a, r) => a + r.total, 0),
    };
  } finally {
    reclaimBusy.value = false;
  }
}

function start() {
  if (timer) return;
  call('set_dashboard_open', { open: true });
  tick();
  timer = setInterval(tick, 2000);
}
function stop() {
  clearInterval(timer);
  timer = null;
  call('set_dashboard_open', { open: false });
}
watch(() => props.active, (a) => (a ? start() : stop()), { immediate: true });
onMounted(findReclaimable);
onUnmounted(stop);

const c = computed(() => store.settings);
const unit = computed(() => c.value.temp_unit);
const T = (v, withUnit = true) => temp(v, unit.value, withUnit);

// ---- series helpers (values scaled to 0..1 for the graphs) ----
const pctSeries = (a) => (a || []).map((v) => v / 100);
const scaled = (a, floor = 1) => {
  const arr = a || [];
  const peak = Math.max(floor, ...arr);
  return arr.map((v) => v / peak);
};
const tempSeries = (a) => (a || []).map((v) => Math.min(1, Math.max(0, (v - 30) / 70)));
const peak = (a) => Math.max(0, ...(a || []));

const ring = computed(() => {
  const h = s.value?.health ?? 0;
  const len = 2 * Math.PI * 44;
  return { dash: `${(h / 100) * len} ${len}`, color: h >= 85 ? 'var(--ok)' : h >= 65 ? 'var(--warn)' : 'var(--danger)' };
});

// P-cores / E-cores in groups
const coreGroups = computed(() => {
  const x = s.value;
  if (!x) return [];
  const groups = [];
  let cur = null;
  x.cpu.cores.forEach((u, i) => {
    const kind = x.system.core_kinds[i] || '';
    if (!cur || cur.kind !== kind) {
      cur = { kind, items: [] };
      groups.push(cur);
    }
    cur.items.push({ i, u });
  });
  return groups.map((g) => ({ ...g, name: g.kind === 'P' ? `${g.items.length} performance cores` : g.kind === 'E' ? `${g.items.length} efficiency cores` : `${g.items.length} cores` }));
});

const temps = computed(() => {
  const x = s.value;
  if (!x) return [];
  const list = [...x.temps];
  if (x.cpu.temp != null) list.push({ label: 'CPU package', c: x.cpu.temp, kind: 'cpu' });
  return list.sort((a, b) => b.c - a.c);
});
const hottest = computed(() => temps.value[0]);
const topRpm = computed(() => Math.max(6000, peak(s.value?.hist.fan)));
const fullest = computed(() => (s.value?.disks.mounts || []).reduce((a, m) => (!a || m.pct > a.pct ? m : a), null));
const procs = computed(() => (s.value ? s.value.procs[procSort.value] : []));
const nv = computed(() => s.value?.gpu.nvidia);
const bat = computed(() => s.value?.power.battery);

function band(f) {
  if (f == null) return '';
  return f >= 5925 ? '6 GHz' : f >= 4900 ? '5 GHz' : '2.4 GHz';
}
function quality(dbm) {
  return dbm == null ? 0 : Math.min(100, Math.max(0, 2 * (dbm + 100)));
}
</script>

<template>
  <div class="page">
    <PageHead>
      <template #title>Dashboard</template>
      <template #lead>{{ s ? `${s.system.host} · ${s.system.os} · kernel ${s.system.kernel} · up ${dur(s.system.uptime_secs)}` : 'Reading the system…' }}</template>
    </PageHead>
    <div v-if="s" class="page-body">
      <div class="top">
        <div class="card health">
          <svg viewBox="0 0 100 100" width="104" height="104" aria-hidden="true">
            <circle cx="50" cy="50" r="44" fill="none" stroke="var(--panel-2)" stroke-width="9" />
            <circle cx="50" cy="50" r="44" fill="none" :stroke="ring.color" stroke-width="9" stroke-linecap="round" :stroke-dasharray="ring.dash" transform="rotate(-90 50 50)" />
            <text x="50" y="58" text-anchor="middle" font-size="28" font-weight="700" fill="currentColor">{{ s.health }}</text>
          </svg>
          <div class="col" style="gap: 4px">
            <h2>System health</h2>
            <ul class="notes"><li v-if="!s.health_notes.length" class="muted">Nothing needs attention.</li><li v-for="n in s.health_notes" :key="n">{{ n }}</li></ul>
          </div>
        </div>
        <div class="card reclaim">
          <h2>Space you can free</h2>
          <div class="big">{{ reclaim ? bytes(reclaim.total) : '…' }}</div>
          <p class="muted" v-if="reclaim">safe to clean now{{ reclaim.all > reclaim.total ? `, ${bytes(reclaim.all)} with items that need review` : '' }}</p>
          <div class="row">
            <button class="btn primary" @click="go('clean')"><Icon name="clean" />Review and clean</button>
            <button class="btn ghost icon" :disabled="reclaimBusy" title="Look again" @click="findReclaimable"><Icon name="refresh" /></button>
          </div>
        </div>
      </div>

      <div class="sections">
        <!-- CPU -->
        <section class="card">
          <button class="head" @click="fold('cpu')">
            <Icon name="cpu" /><h2 class="grow">CPU</h2>
            <span :class="['sum', lvl(s.cpu.usage, c.cpu_warn, c.cpu_crit)]">{{ pct(s.cpu.usage) }} · {{ T(s.cpu.temp, false) }}</span>
            <Icon name="chevrondown" :class="{ turned: folded.cpu }" />
          </button>
          <div v-show="!folded.cpu" class="body">
            <Graph :a="pctSeries(s.hist.cpu)" :b="tempSeries(s.hist.cpu_temp)" color-b="var(--warn)" caption="usage 0–100 %  ·  temperature 30–100 °C" />
            <div class="tiles">
              <div class="tile"><span class="muted">usage</span><strong :class="lvl(s.cpu.usage, c.cpu_warn, c.cpu_crit)">{{ pct(s.cpu.usage) }}</strong></div>
              <div class="tile"><span class="muted">package</span><strong :class="lvl(s.cpu.temp, c.temp_warn, c.temp_crit)">{{ T(s.cpu.temp) }}</strong></div>
              <div class="tile"><span class="muted">fastest core</span><strong>{{ mhz(s.cpu.freq_max) }}</strong></div>
              <div class="tile"><span class="muted">power</span><strong>{{ watts(s.power.cpu_w) }}</strong></div>
            </div>
            <div class="cores">
              <div v-for="(g, gi) in coreGroups" :key="gi" class="cg" :style="{ flex: g.items.length }">
                <div class="bars">
                  <i v-for="it in g.items" :key="it.i" :title="`CPU ${it.i}${g.kind ? ' (' + g.kind + '-core)' : ''}: ${pct(it.u)}`"
                     :style="{ height: Math.max(2, it.u * 0.36) + 'px', background: lvl(it.u, c.cpu_warn, c.cpu_crit) === 'ok' ? 'var(--accent)' : lvl(it.u, c.cpu_warn, c.cpu_crit) === 'warn' ? 'var(--warn)' : 'var(--danger)' }" />
                </div>
                <div class="muted cn">{{ g.name }}</div>
              </div>
            </div>
            <div class="kv"><span class="muted">{{ s.system.model }}</span></div>
            <div v-if="s.system.hybrid" class="kv"><span class="muted">Clock, performance cores (avg / max)</span><b>{{ mhz(s.cpu.freq_p) }} / {{ mhz(s.system.max_freq) }}</b></div>
            <div v-if="s.system.hybrid" class="kv"><span class="muted">Clock, efficiency cores (avg / max)</span><b>{{ mhz(s.cpu.freq_e) }} / {{ mhz(s.system.e_max_freq) }}</b></div>
            <div v-else class="kv"><span class="muted">Clock (avg / max)</span><b>{{ mhz(s.cpu.freq_p) }} / {{ mhz(s.system.max_freq) }}</b></div>
            <div class="kv"><span class="muted">Hottest core</span><b :class="lvl(s.cpu.hottest, c.temp_warn, c.temp_crit)">{{ T(s.cpu.hottest) }}</b></div>
            <div class="kv"><span class="muted">Load 1 / 5 / 15 min ({{ s.system.ncpu }} threads)</span><b :class="lvl(s.cpu.load[0], s.system.ncpu * 0.7, s.system.ncpu)">{{ s.cpu.load.map((l) => l.toFixed(2)).join(' / ') }}</b></div>
            <div class="kv"><span class="muted">Tasks waiting for a CPU (last 10 s)</span><b :class="lvl(s.cpu.psi, 10, 40)">{{ pct(s.cpu.psi, 1) }}</b></div>
          </div>
        </section>

        <!-- Memory -->
        <section class="card">
          <button class="head" @click="fold('mem')">
            <Icon name="memory" /><h2 class="grow">Memory</h2>
            <span :class="['sum', lvl(s.mem.used_pct, c.ram_warn, c.ram_crit)]">{{ mb(s.mem.used_mb) }} of {{ mb(s.mem.total_mb) }}</span>
            <Icon name="chevrondown" :class="{ turned: folded.mem }" />
          </button>
          <div v-show="!folded.mem" class="body">
            <Graph :a="pctSeries(s.hist.ram)" :b="pctSeries(s.hist.swap)" color-b="var(--accent-2, #5aa0e6)" caption="memory in use  ·  swap in use (0–100 %)" />
            <Meter label="Memory" :frac="s.mem.used_pct / 100" :level="lvl(s.mem.used_pct, c.ram_warn, c.ram_crit)" :value="`${mb(s.mem.used_mb)} / ${mb(s.mem.total_mb)}`" />
            <Meter v-if="s.mem.swap_total_mb" label="Swap" :frac="s.mem.swap_pct / 100" color="#5aa0e6" :value="`${mb(s.mem.swap_used_mb)} / ${mb(s.mem.swap_total_mb)}`" />
            <div class="kv"><span class="muted">Available for apps</span><b>{{ mb(s.mem.avail_mb) }}</b></div>
            <div class="kv"><span class="muted">File cache (freed when needed)</span><b>{{ mb(s.mem.cached_mb) }}</b></div>
            <div class="kv"><span class="muted">Shared / tmpfs</span><b>{{ mb(s.mem.shared_mb) }}</b></div>
            <div class="kv"><span class="muted">Waiting to be written to disk</span><b>{{ mb(s.mem.dirty_mb) }}</b></div>
            <div class="kv"><span class="muted">Tasks waiting for memory (last 10 s)</span><b :class="lvl(s.mem.psi, 5, 20)">{{ pct(s.mem.psi, 1) }}</b></div>
            <div class="kv"><span class="muted">Tasks waiting for disk I/O (last 10 s)</span><b :class="lvl(s.mem.io_psi, 10, 40)">{{ pct(s.mem.io_psi, 1) }}</b></div>
          </div>
        </section>

        <!-- GPU -->
        <section class="card" v-if="s.gpu.igpu_freq != null || nv.present">
          <button class="head" @click="fold('gpu')">
            <Icon name="treemap" /><h2 class="grow">GPU</h2>
            <span class="sum">{{ nv.present && nv.util != null ? `NVIDIA ${pct(nv.util)} · ${T(nv.temp, false)}` : s.gpu.igpu_freq != null ? (s.gpu.igpu_freq ? `Intel ${s.gpu.igpu_freq} MHz` : 'Intel idle') : '' }}</span>
            <Icon name="chevrondown" :class="{ turned: folded.gpu }" />
          </button>
          <div v-show="!folded.gpu" class="body">
            <Meter v-if="s.gpu.igpu_freq != null" label="Intel GPU clock" :frac="s.gpu.igpu_max ? s.gpu.igpu_freq / s.gpu.igpu_max : 0" :value="s.gpu.igpu_freq ? `${s.gpu.igpu_freq} / ${s.gpu.igpu_max || 0} MHz` : 'idle'" />
            <div v-if="!nv.present" class="kv"><span class="muted">NVIDIA GPU</span><b class="muted">not present or switched off</b></div>
            <template v-else-if="!nv.awake">
              <div class="kv"><span class="muted">NVIDIA GPU</span><b style="color: var(--ok)">asleep</b></div>
              <p class="muted small">Not read while asleep: asking it would wake it and use battery.</p>
            </template>
            <div v-else-if="nv.util == null" class="kv"><span class="muted">NVIDIA GPU</span><b>awake, reading…</b></div>
            <template v-else>
              <Graph :a="pctSeries(s.hist.nvidia)" color="var(--ok)" caption="NVIDIA load (0–100 %)" />
              <div class="kv"><span class="muted">NVIDIA GPU</span><b>{{ nv.name }}</b></div>
              <Meter label="Load" :frac="nv.util / 100" :value="pct(nv.util)" color="var(--ok)" />
              <Meter v-if="nv.vram_total_mb" label="Video memory" :frac="(nv.vram_used_mb || 0) / nv.vram_total_mb" color="#5aa0e6" :value="`${mb(nv.vram_used_mb)} / ${mb(nv.vram_total_mb)}`" />
              <div class="kv"><span class="muted">Temperature</span><b :class="lvl(nv.temp, c.temp_warn, c.temp_crit)">{{ T(nv.temp) }}</b></div>
              <div class="kv"><span class="muted">Power</span><b>{{ watts(nv.watts) }}</b></div>
              <div class="kv"><span class="muted">Clock</span><b>{{ mhz(nv.clock) }}</b></div>
            </template>
          </div>
        </section>

        <!-- Temperatures and fans -->
        <section class="card" v-if="temps.length || s.fans.length">
          <button class="head" @click="fold('temps')">
            <Icon name="wrench" /><h2 class="grow">Temperatures & fans</h2>
            <span :class="['sum', hottest ? lvl(hottest.c, c.temp_warn, c.temp_crit) : '']">{{ hottest ? `hottest ${T(hottest.c, false)}` : '' }}</span>
            <Icon name="chevrondown" :class="{ turned: folded.temps }" />
          </button>
          <div v-show="!folded.temps" class="body">
            <Meter v-for="t in temps" :key="t.label" :label="t.label" :frac="(t.c - 30) / 70" :level="lvl(t.c, c.temp_warn, c.temp_crit)" :value="T(t.c)" color="var(--ok)" :label-width="170" />
            <template v-if="s.fans.length">
              <hr />
              <Meter v-for="f in s.fans" :key="f.label" :label="f.label" :frac="f.rpm / topRpm" :value="`${f.rpm.toFixed(0)} rpm`" color="#5aa0e6" :label-width="170" />
              <div v-if="s.power.profile" class="kv"><span class="muted">Power profile (sets the fan curve)</span><b>{{ s.power.profile }}</b></div>
            </template>
          </div>
        </section>

        <!-- Power -->
        <section class="card">
          <button class="head" @click="fold('power')">
            <Icon name="battery" /><h2 class="grow">Power</h2>
            <span class="sum">{{ s.power.cpu_w != null ? `CPU ${watts(s.power.cpu_w)}` : '' }}{{ bat && bat.status === 'Discharging' && bat.watts ? ` · battery ${watts(bat.watts)}` : '' }}</span>
            <Icon name="chevrondown" :class="{ turned: folded.power }" />
          </button>
          <div v-show="!folded.power" class="body">
            <template v-if="s.power.rapl_available">
              <Graph :a="scaled(s.hist.cpu_w, 10)" color="var(--warn)" :caption="`CPU package power, peak ${watts(peak(s.hist.cpu_w))}`" />
              <div class="tiles">
                <div class="tile"><span class="muted">CPU package</span><strong>{{ watts(s.power.rapl['package-0']) }}</strong></div>
                <div class="tile"><span class="muted">cores</span><strong>{{ watts(s.power.rapl['core']) }}</strong></div>
                <div class="tile"><span class="muted">graphics & rest</span><strong>{{ watts(s.power.rapl['uncore']) }}</strong></div>
              </div>
            </template>
            <p v-else class="muted small">CPU watts are not readable: Intel's RAPL counters are root-only by default. A udev rule that makes them readable for your group enables them.</p>
            <template v-if="bat">
              <Meter label="Battery" :frac="(bat.pct || 0) / 100" :level="(bat.pct || 100) < 15 && bat.status !== 'Charging' ? 'crit' : 'ok'" color="var(--ok)" :value="`${pct(bat.pct)} · ${bat.status}`" :label-width="90" />
              <div v-if="bat.watts && bat.watts > 0.3" class="kv"><span class="muted">{{ bat.status === 'Charging' ? 'Charging at' : 'Drawing' }}</span><b>{{ watts(bat.watts) }}</b></div>
              <div v-if="bat.hours_left != null" class="kv"><span class="muted">{{ bat.status === 'Charging' ? 'Until full' : 'Time left' }}</span><b>{{ hours(bat.hours_left) }}</b></div>
              <div class="kv"><span class="muted">Energy now / full</span><b>{{ bat.now_wh.toFixed(1) }} / {{ bat.full_wh.toFixed(1) }} Wh</b></div>
              <div class="kv"><span class="muted">Health (of the design capacity)</span><b :class="lvl(100 - (bat.health || 100), 20, 40)">{{ pct(bat.health, 1) }}</b></div>
              <div v-if="bat.cycles" class="kv"><span class="muted">Charge cycles</span><b>{{ bat.cycles }}</b></div>
            </template>
            <div class="kv"><span class="muted">Charger</span><b>{{ s.power.ac ? 'connected' : 'not connected' }}</b></div>
            <div v-if="s.power.epp" class="kv"><span class="muted">CPU energy preference</span><b>{{ s.power.epp }}</b></div>
          </div>
        </section>

        <!-- Disks -->
        <section class="card">
          <button class="head" @click="fold('disks')">
            <Icon name="large" /><h2 class="grow">Disks</h2>
            <span class="sum">↓ {{ rate(s.disks.read_bps) }} ↑ {{ rate(s.disks.write_bps) }}</span>
            <Icon name="chevrondown" :class="{ turned: folded.disks }" />
          </button>
          <div v-show="!folded.disks" class="body">
            <Graph :a="scaled(s.hist.read, 1048576)" :b="scaled(s.hist.write, 1048576)" :caption="`read  ·  written, peak ${rate(Math.max(peak(s.hist.read), peak(s.hist.write)))}`" />
            <div v-for="d in s.disks.io" :key="d.block" class="kv">
              <span class="grow ellipsis">{{ d.name }}</span><span class="muted" v-if="d.temp != null">{{ T(d.temp) }}</span>
              <span class="muted io">↓ {{ rate(d.read_bps) }}</span><span class="muted io">↑ {{ rate(d.write_bps) }}</span>
            </div>
            <Meter v-for="m in s.disks.mounts" :key="m.source" :label="m.path" :frac="m.pct / 100" :level="lvl(m.pct, 85, 95)" :value="`${bytes(m.used)} / ${bytes(m.total)}`" :label-width="110" />
          </div>
        </section>

        <!-- Network -->
        <section class="card">
          <button class="head" @click="fold('net')">
            <Icon name="net" /><h2 class="grow">Network</h2>
            <span class="sum">↓ {{ rate(s.net.rx_bps) }} ↑ {{ rate(s.net.tx_bps) }}</span>
            <Icon name="chevrondown" :class="{ turned: folded.net }" />
          </button>
          <div v-show="!folded.net" class="body">
            <Graph :a="scaled(s.hist.rx, 102400)" :b="scaled(s.hist.tx, 102400)" :caption="`download  ·  upload, peak ${rate(Math.max(peak(s.hist.rx), peak(s.hist.tx)))}`" />
            <div v-for="i in s.net.ifaces" :key="i.name" class="kv">
              <span class="grow ellipsis"><b>{{ i.name }}</b> <span class="muted">{{ i.wireless ? 'Wi-Fi' : 'wired' }}{{ i.ip ? ' · ' + i.ip : '' }}</span></span>
              <span class="muted io">↓ {{ rate(i.rx_bps) }}</span><span class="muted io">↑ {{ rate(i.tx_bps) }}</span>
            </div>
            <template v-if="s.net.wifi?.connected">
              <Meter label="Wi-Fi signal" :frac="quality(s.net.wifi.signal) / 100" :level="quality(s.net.wifi.signal) < 30 ? 'warn' : 'ok'" :value="`${s.net.wifi.signal} dBm`" />
              <div class="kv"><span class="muted">Network</span><b>{{ s.net.wifi.ssid }}{{ s.net.wifi.freq ? ' · ' + band(s.net.wifi.freq) : '' }}</b></div>
              <div class="kv"><span class="muted">Link speed (down / up)</span><b>{{ s.net.wifi.rx_rate ?? '–' }} / {{ s.net.wifi.tx_rate ?? '–' }} Mbit/s</b></div>
            </template>
          </div>
        </section>

        <!-- Processes -->
        <section class="card">
          <button class="head" @click="fold('procs')">
            <Icon name="list" /><h2 class="grow">Processes</h2>
            <span class="sum">{{ s.procs.count }} programs</span>
            <Icon name="chevrondown" :class="{ turned: folded.procs }" />
          </button>
          <div v-show="!folded.procs" class="body">
            <div class="row">
              <button class="chip" :class="{ on: procSort === 'cpu' }" @click="procSort = 'cpu'">by CPU</button>
              <button class="chip" :class="{ on: procSort === 'mem' }" @click="procSort = 'mem'">by memory</button>
              <span class="muted small grow right">programs with several processes are added up</span>
            </div>
            <div v-for="p in procs" :key="p.name" class="kv">
              <span class="grow ellipsis">{{ p.name }}<span v-if="p.count > 1" class="muted"> ×{{ p.count }}</span></span>
              <span class="num" :class="{ muted: procSort !== 'cpu' }">{{ pct(p.cpu, 1) }}</span>
              <span class="num" :class="{ muted: procSort !== 'mem' }">{{ mb(p.mem_mb) }}</span>
            </div>
          </div>
        </section>
      </div>
    </div>
    <div v-else class="empty"><h2>Reading the system…</h2></div>
  </div>
</template>

<style scoped>
.top { display: grid; grid-template-columns: repeat(auto-fit, minmax(320px, 1fr)); gap: 14px; margin-bottom: 14px; }
.health { display: flex; align-items: center; gap: 18px; }
.reclaim { display: flex; flex-direction: column; gap: 8px; }
.notes { margin: 0; padding-left: 18px; }
.big { font-size: 34px; font-weight: 700; letter-spacing: -0.02em; }
.sections { columns: 2 440px; column-gap: 14px; }
.sections > section { break-inside: avoid; margin-bottom: 14px; padding: 6px 16px 14px; display: inline-block; width: 100%; }
.head { display: flex; align-items: center; gap: 10px; width: 100%; padding: 8px 0; border: 0; background: none; color: var(--text); font: inherit; cursor: pointer; text-align: left; }
.head .turned { transform: rotate(-90deg); }
.sum { font-weight: 600; color: var(--muted); font-variant-numeric: tabular-nums; }
.sum.warn, b.warn, strong.warn { color: var(--warn); }
.sum.crit, b.crit, strong.crit { color: var(--danger); }
.body { display: flex; flex-direction: column; gap: 9px; padding-top: 4px; }
.kv { display: flex; align-items: center; gap: 12px; justify-content: space-between; min-height: 22px; font-size: 13px; }
.kv b { font-variant-numeric: tabular-nums; }
.io { min-width: 88px; text-align: right; }
.num { min-width: 70px; text-align: right; font-variant-numeric: tabular-nums; font-weight: 600; }
.tiles { display: grid; grid-template-columns: repeat(auto-fit, minmax(90px, 1fr)); gap: 8px; }
.tile { display: flex; flex-direction: column; background: var(--panel-2); border-radius: 8px; padding: 8px 10px; font-size: 12px; }
.tile strong { font-size: 17px; }
.cores { display: flex; gap: 12px; }
.cg { min-width: 0; }
.bars { display: flex; align-items: flex-end; gap: 2px; height: 38px; background: var(--panel-2); border-radius: 6px; padding: 0 3px; }
.bars i { flex: 1; border-radius: 2px; min-width: 3px; }
.cn { text-align: center; font-size: 11px; margin-top: 3px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
hr { border: 0; border-top: 1px solid var(--line); margin: 2px 0; width: 100%; }
.small { font-size: 12px; }
</style>
