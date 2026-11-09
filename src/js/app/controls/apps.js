// ============================================================
// apps.js
// "Show on → Apps": the apps where blips never show. The chips under
// the row list them; "Choose apps" opens a list of the installed and
// open apps (with Browse for the rest). Only each app's name and
// file name are saved, never where it's installed.
// ============================================================

import { $, app, invoke, onRender, save } from '../state.js';
import { reveal } from '../components/reveal.js';

const dialog = $('#apps-dialog');
const list = $('#apps-list');
const search = dialog.querySelector('.apps-search input');
const filter = $('#apps-filter');
const chipsWrap = $('#app-chips-wrap');
const chips = $('#app-chips');

const EASE = 'cubic-bezier(0.4, 0, 0.2, 1)';

/** Icons and program paths by file name, from the last look at the apps. Never saved. */
const icons = new Map();
const paths = new Map();
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

/** The ticks while the dialog is open. Saved only with Done; Cancel or Esc drops them. */
let draft = [];
const isTicked = (exe) => draft.some((a) => a.exe === exe);
/** The "3 skipped" pill is on: the list shows only the ticked apps. */
let onlyTicked = false;

const escape = (text) => text.replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`);

function iconHtml(exe) {
  const src = icons.get(exe);
  return src ? `<img class="app-icon" src="${src}" alt="" />` : '<i class="icon icon-app app-icon"></i>';
}

function remember(apps) {
  for (const a of apps) {
    if (a.icon) icons.set(a.exe, a.icon);
    if (a.path) paths.set(a.exe, a.path);
  }
}

// ------------------------------------------------------------
// Chips under the row
// ------------------------------------------------------------

// Each chip is kept and only added or removed on its own, so they can grow in
// and fade out while the others slide over.

let chipsShown = false; // the page as it opens: no animation

function makeChip(a) {
  const chip = document.createElement('span');
  chip.className = 'app-chip';
  chip.dataset.exe = a.exe;
  chip.innerHTML = `${iconHtml(a.exe)}<span>${escape(a.name)}</span><button title="Remove" aria-label="Remove ${escape(a.name)}"><i class="icon icon-x"></i></button>`;
  return chip;
}

/** Fades and shrinks a chip away, then removes it. */
function leave(chip) {
  chip.dataset.leaving = '1';
  const width = chip.offsetWidth;
  chip.animate(
    [
      { opacity: 1, transform: 'scale(1)', width: `${width}px`, marginRight: '0px' },
      { opacity: 0, transform: 'scale(0.85)', width: '0px', paddingLeft: '0px', paddingRight: '0px', borderWidth: '0px', marginRight: '-6px' },
    ],
    { duration: 220, easing: EASE, fill: 'forwards' },
  ).finished.then(
    () => chip.remove(),
    () => chip.remove(),
  );
  // Animations pause while the window is hidden; it still goes.
  setTimeout(() => chip.remove(), 400);
}

function enter(chip) {
  chip.animate(
    [
      { opacity: 0, transform: 'scale(0.85)' },
      { opacity: 1, transform: 'scale(1)' },
    ],
    { duration: 200, easing: EASE },
  );
}

function renderChips(state) {
  const apps = state.settings.skipApps;
  const animate = chipsShown;
  reveal(chipsWrap, apps.length > 0, { animate });
  const existing = new Map([...chips.children].map((el) => [el.dataset.exe, el]));
  for (const [exe, chip] of existing) {
    if (!apps.some((a) => a.exe === exe) && !chip.dataset.leaving) {
      if (animate) leave(chip);
      else chip.remove();
    }
  }
  for (const a of apps) {
    let chip = existing.get(a.exe);
    if (!chip || chip.dataset.leaving) {
      chip = makeChip(a);
      chips.append(chip);
      if (animate) enter(chip);
    } else if (icons.has(a.exe) && !chip.querySelector('img')) {
      // Its icon arrived after the chip was made.
      chip.querySelector('.app-icon').outerHTML = iconHtml(a.exe);
    }
    chip.title = paths.get(a.exe) ?? a.name;
  }
  chipsShown = true;
}

chips.addEventListener('click', (e) => {
  const chip = e.target.closest('button') && e.target.closest('.app-chip');
  if (chip) save({ skipApps: skipped().filter((a) => a.exe !== chip.dataset.exe) });
});

// ------------------------------------------------------------
// The dialog
// ------------------------------------------------------------

function row(a) {
  const on = isTicked(a.exe);
  const path = paths.get(a.exe);
  const folder = path
    ? `<button type="button" class="app-folder" data-reveal title="Show in folder" aria-label="Show ${escape(a.name)} in its folder"><i class="icon icon-folder"></i></button>`
    : '';
  return `<div class="app-item ${on ? 'is-checked' : ''}" data-exe="${escape(a.exe)}" role="checkbox" aria-checked="${on}" tabindex="0">${iconHtml(a.exe)}<span class="app-name"${path ? ` title="${escape(path)}"` : ''}>${escape(a.name)}</span>${folder}<i class="icon icon-check app-check"></i></div>`;
}

function renderList() {
  const query = search.value.trim().toLowerCase();
  const base = onlyTicked ? shown.filter((a) => isTicked(a.exe)) : shown;
  const matches = base.filter((a) => !query || a.name.toLowerCase().includes(query) || a.exe.includes(query));
  if (matches.length === 0) {
    list.innerHTML = `<p class="apps-empty">${shown.length ? 'No apps match your search.' : 'No apps found. Use Browse to pick one.'}</p>`;
    return;
  }
  // Apps that are open right now first: usually the one you're after.
  const open = onlyTicked ? [] : matches.filter((a) => a.open);
  if (open.length === 0) {
    list.innerHTML = matches.map(row).join('');
    return;
  }
  const rest = matches.filter((a) => !a.open);
  list.innerHTML =
    `<p class="apps-group">Open now</p>${open.map(row).join('')}` +
    (rest.length ? `<p class="apps-group">All apps</p>${rest.map(row).join('')}` : '');
}

/** The pill in the search box: how many are ticked, and whether only those show. */
function updateFilter() {
  const count = draft.length;
  if (count === 0 && onlyTicked) {
    onlyTicked = false;
    renderList();
  }
  filter.hidden = count === 0;
  filter.textContent = `${count} skipped`;
  filter.classList.toggle('is-on', onlyTicked);
  filter.setAttribute('aria-pressed', String(onlyTicked));
  filter.title = onlyTicked ? 'Show all apps' : 'Show only the skipped apps';
}

filter.addEventListener('click', () => {
  onlyTicked = !onlyTicked;
  renderList();
  list.scrollTop = 0;
  updateFilter();
});

/** Every app found, plus skipped ones that weren't, by name. */
function arrange(apps) {
  const extra = skipped().filter((s) => !apps.some((r) => r.exe === s.exe));
  return [...apps, ...extra].sort((a, b) => a.name.localeCompare(b.name));
}

async function open() {
  draft = [...skipped()];
  onlyTicked = false;
  search.value = '';
  updateFilter();
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
  updateFilter();
}

list.addEventListener('click', (e) => {
  const folder = e.target.closest('[data-reveal]');
  if (folder) {
    const path = paths.get(folder.closest('[data-exe]').dataset.exe);
    if (path) invoke('reveal_app', { path }).catch(() => {});
    return;
  }
  const item = e.target.closest('[data-exe]');
  if (item) toggle(item);
});

// The rows tick with Space or Enter too.
list.addEventListener('keydown', (e) => {
  const item = e.target.closest('.app-item');
  if (item && e.target === item && (e.key === ' ' || e.key === 'Enter')) {
    e.preventDefault();
    toggle(item);
  }
});

search.addEventListener('input', renderList);
// Enter in the search box shouldn't close the dialog.
search.addEventListener('keydown', (e) => {
  if (e.key === 'Enter') e.preventDefault();
});
// Clicking anywhere on the box (not the pill) puts you in the search.
dialog.querySelector('.apps-search').addEventListener('mousedown', (e) => {
  if (e.target !== search && !e.target.closest('#apps-filter')) {
    e.preventDefault();
    search.focus();
  }
});

$('#apps-browse').addEventListener('click', async () => {
  const picked = await invoke('browse_app');
  if (!picked) return;
  remember([picked]);
  if (!shown.some((a) => a.exe === picked.exe)) shown = [...shown, picked].sort((a, b) => a.name.localeCompare(b.name));
  if (!isTicked(picked.exe)) draft = [...draft, { name: picked.name, exe: picked.exe }];
  renderList();
  updateFilter();
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
