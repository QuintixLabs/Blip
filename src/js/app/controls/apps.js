// ============================================================
// apps.js
// Show on → Apps: the chips under the row and the Choose apps dialog.
// ============================================================

import { $, $$, app, invoke, onRender, save } from '../state.js';
import { reveal } from '../components/reveal.js';

const dialog = $('#apps-dialog');
const list = $('#apps-list');
const search = dialog.querySelector('.apps-search input');
const filters = $('#apps-filters');
const chipsWrap = $('#app-chips-wrap');
const chips = $('#app-chips');

/** By file name. Never saved. */
const icons = new Map();
const paths = new Map();
let shown = [];
let found = null;
/** File names of the apps Blip found itself; the rest were picked with Browse. */
let foundExes = new Set();

function findApps() {
  found ??= invoke('list_apps').then((apps) => {
    remember(apps);
    foundExes = new Set(apps.map((a) => a.exe));
    renderChips(app.state);
    return apps;
  });
  return found;
}

const skipped = () => app.state.settings.skipApps;

/** The ticks while the dialog is open. Saved only with Done; Cancel or Esc drops them. */
let draft = [];
const isTicked = (exe) => draft.some((a) => a.exe === exe);
const isBrowsed = (exe) => !foundExes.has(exe);
/** "all", "skipped" or "browsed". */
let view = 'all';

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

// Chips are added and removed one by one, so a removed one can fade out.

let chipsShown = false; // the page as it opens: no animation

function makeChip(a) {
  const chip = document.createElement('span');
  chip.className = 'app-chip';
  chip.dataset.exe = a.exe;
  chip.innerHTML = `${iconHtml(a.exe)}<span class="app-chip-name">${escape(a.name)}</span><button title="Remove" aria-label="Remove ${escape(a.name)}"><i class="icon icon-x"></i></button>`;
  return chip;
}

const EASE = 'cubic-bezier(0.4, 0, 0.2, 1)';

/** A quick fade, then gone. The chips after it glide into its place, and when a
 *  whole row empties, the chip area eases to its new height instead of jumping. */
function leave(chip) {
  chip.dataset.leaving = '1';
  let done = false;
  const remove = () => {
    if (done) return;
    done = true;
    const others = [...chips.children].filter((c) => c !== chip);
    const before = new Map(others.map((c) => [c, c.getBoundingClientRect()]));
    const height = chips.offsetHeight;
    // Hold the area at its height while the chip goes. Letting the page get
    // shorter here, even for a moment, makes it jump when you're at the bottom.
    chips.style.height = `${height}px`;
    chip.remove();
    for (const c of others) {
      const was = before.get(c);
      const now = c.getBoundingClientRect();
      const dx = was.left - now.left;
      const dy = was.top - now.top;
      if (dx || dy) {
        c.animate([{ transform: `translate(${dx}px, ${dy}px)` }, { transform: 'none' }], { duration: 180, easing: EASE });
      }
    }
    const newHeight = naturalHeight();
    if (newHeight >= height) {
      chips.style.height = '';
      return;
    }
    keepPageLength(height - newHeight);
    // First the chips glide while the area keeps its size, then the area eases to
    // its new height: done together, it would cut off a chip that's still moving.
    chips.animate([{ height: `${height}px` }, { height: `${newHeight}px` }], { duration: 220, delay: 180, easing: EASE, fill: 'backwards' }).finished.then(
      () => (chips.style.height = ''),
      () => (chips.style.height = ''),
    );
  };
  chip.animate([{ opacity: 1 }, { opacity: 0 }], { duration: 150, easing: 'ease-out', fill: 'forwards' }).finished.then(remove, remove);
  // Animations pause while the window is hidden; it still goes.
  setTimeout(remove, 300);
}

/** The chip area's height for the chips in it now, worked out from where they
 *  are (asking the browser would shorten the page early). */
function naturalHeight() {
  const box = chips.getBoundingClientRect();
  const bottom = Math.max(box.top, ...[...chips.children].map((c) => c.getBoundingClientRect().bottom));
  return Math.round(bottom - box.top + parseFloat(getComputedStyle(chips).paddingBottom));
}

// ------------------------------------------------------------
// Keeping the page still at the bottom
// ------------------------------------------------------------

// Scrolled all the way down, a page that gets shorter pulls everything above
// it down. So while the chip area shrinks, the same room is added under the
// page, and nothing you can see moves. That room goes once it's out of sight:
// when you scroll up or open another page.

const main = $('main');
let extraRoom = 0;
let basePadding = null;

function keepPageLength(lost) {
  const atBottom = main.scrollTop + main.clientHeight >= main.scrollHeight - 2;
  if (!atBottom) return;
  basePadding ??= parseFloat(getComputedStyle(main).paddingBottom);
  extraRoom += lost;
  main.style.paddingBottom = `${basePadding + extraRoom}px`;
}

main.addEventListener('scroll', () => {
  if (!extraRoom) return;
  const outOfSight = main.scrollTop + main.clientHeight < main.scrollHeight - extraRoom - 1;
  if (outOfSight || $('#blip').hidden) {
    extraRoom = 0;
    main.style.paddingBottom = '';
  }
});
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
    } else if (icons.has(a.exe) && !chip.querySelector('img')) {
      chip.querySelector('.app-icon').outerHTML = iconHtml(a.exe);
    }
    chip.classList.toggle('has-folder', paths.has(a.exe));
  }
  chipsShown = true;
}

chips.addEventListener('click', (e) => {
  const chip = e.target.closest('.app-chip');
  if (!chip) return;
  if (e.target.closest('button')) {
    save({ skipApps: skipped().filter((a) => a.exe !== chip.dataset.exe) });
    return;
  }
  const path = paths.get(chip.dataset.exe);
  if (path) invoke('reveal_app', { path }).catch(() => {});
});

// ------------------------------------------------------------
// The dialog
// ------------------------------------------------------------

function row(a) {
  const on = isTicked(a.exe);
  return `<div class="app-item ${on ? 'is-checked' : ''}" data-exe="${escape(a.exe)}" role="checkbox" aria-checked="${on}" tabindex="0">${iconHtml(a.exe)}<span class="app-name">${escape(a.name)}</span><i class="icon icon-check app-check"></i></div>`;
}

const EMPTY = {
  all: 'No apps found. Use Browse to pick one.',
  skipped: 'No apps skipped yet.',
  browsed: 'No apps picked with Browse yet.',
};

function renderList() {
  const query = search.value.trim().toLowerCase();
  const inView = shown.filter((a) => (view === 'skipped' ? isTicked(a.exe) : view === 'browsed' ? isBrowsed(a.exe) : true));
  const matches = inView.filter((a) => !query || a.name.toLowerCase().includes(query) || a.exe.includes(query));
  if (matches.length === 0) {
    list.innerHTML = `<p class="apps-empty">${inView.length ? 'No apps match your search.' : EMPTY[view]}</p>`;
    return;
  }
  list.innerHTML = matches.map(row).join('');
}

function updateFilters() {
  const browsed = shown.some((a) => isBrowsed(a.exe));
  if (view === 'browsed' && !browsed) view = 'all';
  for (const button of $$('#apps-filters [data-filter]')) {
    button.classList.toggle('is-active', button.dataset.filter === view);
    button.setAttribute('aria-pressed', String(button.dataset.filter === view));
  }
  filters.querySelector('[data-filter="browsed"]').hidden = !browsed;
  filters.querySelector('.apps-count').textContent = draft.length ? String(draft.length) : '';
}

filters.addEventListener('click', (e) => {
  const button = e.target.closest('[data-filter]');
  if (!button || button.dataset.filter === view) return;
  view = button.dataset.filter;
  updateFilters();
  renderList();
  list.scrollTop = 0;
});

function arrange(apps) {
  const extra = skipped().filter((s) => !apps.some((r) => r.exe === s.exe));
  return [...apps, ...extra].sort((a, b) => a.name.localeCompare(b.name));
}

async function open() {
  draft = [...skipped()];
  view = 'all';
  search.value = '';
  shown = arrange([]);
  updateFilters();
  list.innerHTML = '<p class="apps-empty">Looking for your apps...</p>';
  list.scrollTop = 0;
  dialog.classList.remove('is-closing');
  dialog.showModal();
  document.activeElement?.blur();
  shown = arrange(await findApps());
  updateFilters();
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
  updateFilters();
}

list.addEventListener('click', (e) => {
  const item = e.target.closest('[data-exe]');
  if (item) toggle(item);
});

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
dialog.querySelector('.apps-search').addEventListener('mousedown', (e) => {
  if (e.target !== search) {
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
  updateFilters();
  renderList();
});

$('#choose-apps').addEventListener('click', open);

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

// Looked up early, so the list opens straight away.
onRender(renderChips);
setTimeout(findApps, 400);
