// ============================================================
// apps.js
// "Show on → Apps": the apps where blips never show. The chips under
// the row list them; "Choose apps" opens a list of the installed and
// open apps (with Browse for the rest). Only each app's name and
// file name are saved, never where it's installed.
// ============================================================

import { $, app, invoke, onRender, save } from '../state.js';

const dialog = $('#apps-dialog');
const list = $('#apps-list');
const search = dialog.querySelector('.apps-search input');
const chips = $('#app-chips');

/** Icons by file name, from the last look at the apps (they aren't saved). */
const icons = new Map();
/** What the dialog lists: every app found, plus skipped ones that weren't. */
let shown = [];
/** The apps found, once looked up (the lookup starts when the window opens). */
let found = null;

function findApps() {
  found ??= invoke('list_apps').then((apps) => {
    remember(apps);
    renderChips(app.state);
    return apps;
  });
  return found;
}

const skipped = () => app.state.settings.skipApps;
const isSkipped = (exe) => skipped().some((a) => a.exe === exe);

/** The ticks while the dialog is open. Saved only with Done; Cancel or Esc drops them. */
let draft = [];
const isTicked = (exe) => draft.some((a) => a.exe === exe);

const escape = (text) => text.replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`);

function iconHtml(exe) {
  const src = icons.get(exe);
  return src ? `<img class="app-icon" src="${src}" alt="" />` : '<i class="icon icon-app app-icon"></i>';
}

function remember(apps) {
  for (const a of apps) if (a.icon) icons.set(a.exe, a.icon);
}

// ------------------------------------------------------------
// Chips under the row
// ------------------------------------------------------------

function renderChips(state) {
  const apps = state.settings.skipApps;
  chips.hidden = apps.length === 0;
  chips.innerHTML = apps
    .map(
      (a) =>
        `<span class="app-chip">${iconHtml(a.exe)}<span>${escape(a.name)}</span><button data-exe="${escape(a.exe)}" title="Remove" aria-label="Remove ${escape(a.name)}"><i class="icon icon-x"></i></button></span>`,
    )
    .join('');
}

chips.addEventListener('click', (e) => {
  const button = e.target.closest('[data-exe]');
  if (button) save({ skipApps: skipped().filter((a) => a.exe !== button.dataset.exe) });
});

// ------------------------------------------------------------
// The dialog
// ------------------------------------------------------------

function renderList() {
  const query = search.value.trim().toLowerCase();
  const matches = shown.filter((a) => !query || a.name.toLowerCase().includes(query) || a.exe.includes(query));
  if (matches.length === 0) {
    list.innerHTML = `<p class="apps-empty">${shown.length ? 'No apps match your search.' : 'No apps found. Use Browse to pick one.'}</p>`;
    return;
  }
  list.innerHTML = matches
    .map((a) => {
      const on = isTicked(a.exe);
      return `<button type="button" class="app-item ${on ? 'is-checked' : ''}" data-exe="${escape(a.exe)}" role="checkbox" aria-checked="${on}">${iconHtml(a.exe)}<span class="app-name">${escape(a.name)}</span><i class="icon icon-check app-check"></i></button>`;
    })
    .join('');
}

/** Skipped apps first (so they're easy to untick), then the rest by name. */
function arrange(apps) {
  const extra = skipped().filter((s) => !apps.some((r) => r.exe === s.exe));
  const all = [...apps, ...extra];
  return all.sort((a, b) => isTicked(b.exe) - isTicked(a.exe) || a.name.localeCompare(b.name));
}

async function open() {
  draft = [...skipped()];
  search.value = '';
  shown = arrange([]);
  list.innerHTML = '<p class="apps-empty">Looking for your apps...</p>';
  list.scrollTop = 0;
  dialog.classList.remove('is-closing');
  dialog.showModal();
  document.activeElement?.blur();
  shown = arrange(await findApps());
  renderList();
}

/** Ticks or unticks in place, so the list doesn't redraw or jump while you pick. */
function toggle(item) {
  const exe = item.dataset.exe;
  const entry = shown.find((a) => a.exe === exe);
  if (!entry) return;
  const on = !isTicked(exe);
  item.classList.toggle('is-checked', on);
  item.setAttribute('aria-checked', String(on));
  draft = on ? [...draft, { name: entry.name, exe }] : draft.filter((a) => a.exe !== exe);
}

list.addEventListener('click', (e) => {
  const item = e.target.closest('[data-exe]');
  if (item) toggle(item);
});

search.addEventListener('input', renderList);
// Enter in the search box shouldn't close the dialog.
search.addEventListener('keydown', (e) => {
  if (e.key === 'Enter') e.preventDefault();
});

$('#apps-browse').addEventListener('click', async () => {
  const picked = await invoke('browse_app');
  if (!picked) return;
  remember([picked]);
  if (!shown.some((a) => a.exe === picked.exe)) shown.push(picked);
  if (!isTicked(picked.exe)) draft = [...draft, { name: picked.name, exe: picked.exe }];
  renderList();
});

$('#choose-apps').addEventListener('click', open);

/** Plays the fade-out, then really closes (like the confirm box). */
function close() {
  if (dialog.classList.contains('is-closing')) return;
  dialog.classList.add('is-closing');
  dialog.addEventListener('animationend', () => dialog.close(), { once: true });
}

// Done saves the ticks; Cancel (and Esc, below) leaves things as they were.
dialog.querySelector('form').addEventListener('submit', (e) => {
  e.preventDefault();
  if (e.submitter?.value === 'done') save({ skipApps: draft });
  close();
});

dialog.addEventListener('cancel', (e) => {
  e.preventDefault();
  close();
});

// Looked up quietly once the window is up, so the list opens straight away and
// the chips get their icons.
onRender(renderChips);
setTimeout(findApps, 400);
