<script setup>
import { pickFolder } from '../api.js';
import { saveSettings, store, toast, errorText } from '../store.js';
import Icon from '../components/Icon.vue';
import PageHead from '../components/PageHead.vue';

defineProps({ active: Boolean });

async function save() {
  try {
    await saveSettings();
  } catch (e) {
    toast(errorText(e), 'error');
  }
}
async function add(key, title) {
  const p = await pickFolder(title);
  if (p && !store.settings[key].includes(p)) {
    store.settings[key].push(p);
    save();
  }
}
function drop(key, p) {
  store.settings[key] = store.settings[key].filter((x) => x !== p);
  save();
}
</script>

<template>
  <div class="page">
    <PageHead>
      <template #title>Settings</template>
      <template #lead>Safety first: the defaults never delete your own files for good.</template>
    </PageHead>
    <div class="page-body col" style="max-width: 760px">
      <div class="card col">
        <label class="row"><span class="switch"><input type="checkbox" v-model="store.settings.use_trash" @change="save" /><span /></span>
          <span class="grow"><strong>Use the Trash</strong><br /><span class="muted">Large files, duplicates and app leftovers go to the Trash instead of being deleted. Caches in Clean are always deleted, the Trash would only keep them.</span></span></label>
      </div>
      <div class="card col">
        <div class="row"><strong class="grow">Smallest file shown in the analysis</strong>
          <select v-model.number="store.settings.min_file_bytes" @change="save"><option :value="262144">256 KB</option><option :value="1048576">1 MB</option><option :value="10485760">10 MB</option></select></div>
        <p class="muted">Smaller files are counted in their folder but not listed one by one. A lower value uses more memory on big disks.</p>
      </div>
      <div class="card col">
        <div class="row"><strong class="grow">Tick project folders idle for at least</strong><input type="number" min="1" max="3650" v-model.number="store.settings.purge_idle_days" @change="save" style="width: 80px" /><span class="muted">days</span></div>
      </div>
      <div class="card col">
        <div class="row"><strong class="grow">Extra project folders</strong><button class="btn small" @click="add('purge_roots', 'Folder with your projects')"><Icon name="plus" />Add</button></div>
        <p v-if="!store.settings.purge_roots.length" class="muted">Pumice already looks in ~/Projects, ~/dev, ~/code, ~/src and similar folders.</p>
        <div v-for="p in store.settings.purge_roots" :key="p" class="row"><span class="mono grow ellipsis">{{ p }}</span><button class="btn ghost icon" aria-label="Remove" @click="drop('purge_roots', p)"><Icon name="x" /></button></div>
      </div>
      <div class="card col">
        <div class="row"><strong class="grow">Protected folders</strong><button class="btn small" @click="add('protected', 'Folder Pumice must never clean')"><Icon name="plus" />Add</button></div>
        <p class="muted">Cleanup rules skip anything inside these folders.</p>
        <div v-for="p in store.settings.protected" :key="p" class="row"><span class="mono grow ellipsis">{{ p }}</span><button class="btn ghost icon" aria-label="Remove" @click="drop('protected', p)"><Icon name="x" /></button></div>
      </div>
      <div class="card col">
        <strong>Dashboard warning limits</strong>
        <p class="muted">Values turn orange at the first limit and red at the second.</p>
        <div class="row wrap" style="gap: 18px">
          <label class="row">CPU % <input type="number" min="1" max="100" v-model.number="store.settings.cpu_warn" @change="save" style="width: 64px" /> <input type="number" min="1" max="100" v-model.number="store.settings.cpu_crit" @change="save" style="width: 64px" /></label>
          <label class="row">Memory % <input type="number" min="1" max="100" v-model.number="store.settings.ram_warn" @change="save" style="width: 64px" /> <input type="number" min="1" max="100" v-model.number="store.settings.ram_crit" @change="save" style="width: 64px" /></label>
          <label class="row">Temperature °C <input type="number" min="30" max="120" v-model.number="store.settings.temp_warn" @change="save" style="width: 64px" /> <input type="number" min="30" max="120" v-model.number="store.settings.temp_crit" @change="save" style="width: 64px" /></label>
          <label class="row">Show temperatures in
            <select v-model="store.settings.temp_unit" @change="save"><option value="c">°C</option><option value="f">°F</option></select></label>
        </div>
      </div>
      <div class="card col">
        <strong>Administrator helper</strong>
        <p class="muted">{{ store.helper.installed ? `Installed at ${store.helper.path}.` : `Not installed. Run scripts/install-helper.sh once to enable package removal, journal cleanup and trim.` }}</p>
      </div>
      <p class="muted">Pumice is free software (GPL-3.0). It is inspired by Mole by tw93 and MangoDisk by harry0703, both GPL-3.0.</p>
    </div>
  </div>
</template>
