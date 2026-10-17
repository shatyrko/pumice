<script setup>
import { onMounted, ref } from 'vue';
import { call, pickFolder } from '../api.js';
import { saveSettings, store, toast, errorText } from '../store.js';
import Icon from '../components/Icon.vue';
import PageHead from '../components/PageHead.vue';

defineProps({ active: Boolean });

const ai = ref({ configured: false, has_key: false, model: '' });
const keyInput = ref('');
async function loadAi() {
  ai.value = await call('ai_status');
}
onMounted(loadAi);
async function saveKey() {
  try {
    await call('ai_set_key', { key: keyInput.value });
    keyInput.value = '';
    toast('Key saved in the system keyring.');
    await loadAi();
  } catch (e) {
    toast(errorText(e), 'error');
  }
}
async function removeKey() {
  await call('ai_clear_key');
  toast('Key removed.');
  await loadAi();
}
async function testAi() {
  try {
    const t = await call('ai_explain', { topic: 'test', facts: 'Name: ~/.cache/thumbnails\nExplain what this folder is.' });
    toast(t.slice(0, 160));
  } catch (e) {
    toast(errorText(e), 'error', 9000);
  }
}
async function saveAi() {
  await save();
  await loadAi();
}

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
        <div class="row"><strong class="grow">AI explanations</strong>
          <select v-model="store.settings.ai_provider" @change="saveAi">
            <option value="off">Off</option><option value="anthropic">Anthropic (Claude)</option><option value="openai">OpenAI-compatible server</option>
          </select></div>
        <p class="muted">An “Explain” button next to cleanup rules, apps, startup entries and maintenance checks tells you what an item is and whether it is safe to remove. Only the name, the description and a few example paths of that one item are sent, never file contents. Off by default.</p>
        <template v-if="store.settings.ai_provider !== 'off'">
          <div v-if="store.settings.ai_provider === 'openai'" class="row"><span class="muted" style="width: 90px">Server</span><input type="text" class="grow" v-model="store.settings.ai_base_url" @change="saveAi" placeholder="http://localhost:11434/v1" /></div>
          <div class="row"><span class="muted" style="width: 90px">Model</span><input type="text" class="grow" v-model="store.settings.ai_model" @change="saveAi" :placeholder="store.settings.ai_provider === 'anthropic' ? 'claude-haiku-5-5' : 'model name'" /></div>
          <div class="row"><span class="muted" style="width: 90px">API key</span>
            <input type="password" class="grow" v-model="keyInput" :placeholder="ai.has_key ? 'A key is saved (type a new one to replace it)' : store.settings.ai_provider === 'openai' ? 'Optional for local servers' : 'Paste your key'" autocomplete="off" />
            <button class="btn small" :disabled="!keyInput" @click="saveKey">Save key</button>
            <button class="btn small" :disabled="!ai.has_key" @click="removeKey">Remove</button></div>
          <div class="row"><span class="muted grow">{{ ai.configured ? `Ready: ${ai.model}` : 'Not ready yet: ' + (store.settings.ai_provider === 'anthropic' ? 'save a key.' : 'enter the server and the model.') }} The key is kept in the system keyring.</span>
            <button class="btn small" :disabled="!ai.configured" @click="testAi">Test</button></div>
        </template>
      </div>
      <div class="card col">
        <strong>Administrator helper</strong>
        <p class="muted">{{ store.helper.installed ? `Installed at ${store.helper.path}.` : `Not installed. Run scripts/install-helper.sh once to enable package removal, journal cleanup and trim.` }}</p>
      </div>
      <p class="muted">Pumice is free software (GPL-3.0). It is inspired by Mole by tw93 and MangoDisk by harry0703, both GPL-3.0.</p>
    </div>
  </div>
</template>
