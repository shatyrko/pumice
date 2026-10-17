import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { revealItemInDir } from '@tauri-apps/plugin-opener';

export const call = (cmd, args) => invoke(cmd, args);

export function on(event, handler) {
  let off = null;
  let gone = false;
  listen(event, (e) => handler(e.payload)).then((fn) => {
    if (gone) fn();
    else off = fn;
  });
  return () => {
    gone = true;
    if (off) off();
  };
}

export async function pickFolder(title = 'Choose a folder') {
  const r = await openDialog({ directory: true, multiple: false, title });
  return typeof r === 'string' ? r : null;
}

export async function reveal(path) {
  try {
    await revealItemInDir(path);
  } catch (e) {
    // The file manager may refuse a path that no longer exists.
  }
}
