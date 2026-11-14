// ============================================================
// controls/apps/apps.js
// Skip these apps: finding the apps.
// ============================================================

import { app, invoke } from '../../core/state.js';

/** By file name. Icons are never saved. */
export const icons = new Map();
export const paths = new Map();
/** The file each browsed app was picked from: what's saved for it. */
export const picks = new Map();
let found = null;
let picked = null;
/** File names of the apps Blip found itself; the rest were picked with Browse. */
let foundExes = new Set();

export const skipped = () => app.state.settings.skipApps;
export const isBrowsed = (exe) => !foundExes.has(exe);

export function findApps() {
  found ??= invoke('list_apps').then(async (apps) => {
    remember(apps);
    foundExes = new Set(apps.map((a) => a.exe));
    await findPicked();
    return apps;
  });
  return found;
}

/** Browsed apps aren't in the list, so their icons are read again from where
 *  they were picked. That's quick, so it doesn't wait for the list. */
export function findPicked() {
  picked ??= (async () => {
    const browsed = skipped().filter((a) => a.path);
    for (const a of browsed) picks.set(a.exe, a.path);
    const infos = await Promise.all(browsed.map((a) => invoke('app_info', { path: a.path }).catch(() => null)));
    remember(infos.filter(Boolean));
  })();
  return picked;
}

export function remember(apps) {
  for (const a of apps) {
    if (a.icon) icons.set(a.exe, a.icon);
    if (a.path) paths.set(a.exe, a.path);
  }
}

/** What's saved for an app: browsed ones keep the file they were picked from too. */
export function toSave(name, exe) {
  const path = isBrowsed(exe) ? picks.get(exe) : undefined;
  return path ? { name, exe, path } : { name, exe };
}

export const escape = (text) => text.replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`);

export function iconHtml(exe) {
  const src = icons.get(exe);
  return src ? `<img class="app-icon" src="${src}" alt="" />` : '<i class="icon icon-app app-icon"></i>';
}
