// ============================================================
// controls/apps/picker.js
// The "Skip these apps" dialog: ticking apps, the filters, search
// and Browse.
// ============================================================

import { $, $$, invoke, save } from '../../core/state.js';
import { escape, findApps, iconHtml, isBrowsed, picks, remember, skipped, toSave } from './apps.js';

const dialog = $('#apps-dialog');
const list = $('#apps-list');
const search = dialog.querySelector('.apps-search input');
const filters = $('#apps-filters');
const subtitle = dialog.querySelector('.dialog-text');
const SUBTITLE = subtitle.textContent;

let shown = [];
/** The ticks while the dialog is open. Saved only with Done; Cancel or Esc drops them. */
let draft = [];
const isTicked = (exe) => draft.some((a) => a.exe === exe);
/** "all", "skipped" or "browsed". */
let view = 'all';

// ------------------------------------------------------------
// The list
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

function arrange(apps) {
  const extra = skipped().filter((s) => !apps.some((r) => r.exe === s.exe));
  return [...apps, ...extra].sort((a, b) => a.name.localeCompare(b.name));
}

/** Ticks or unticks in place, so the list doesn't redraw or jump while you pick. */
function toggle(item) {
  const exe = item.dataset.exe;
  const entry = shown.find((a) => a.exe === exe);
  if (!entry) return;
  const on = !isTicked(exe);
  item.classList.toggle('is-checked', on);
  item.setAttribute('aria-checked', String(on));
  draft = on ? [...draft, toSave(entry.name, exe)] : draft.filter((a) => a.exe !== exe);
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

// ------------------------------------------------------------
// Filters and search
// ------------------------------------------------------------

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

// ------------------------------------------------------------
// Browse
// ------------------------------------------------------------

let browsing = false;

$('#apps-browse').addEventListener('click', async () => {
  if (browsing) return;
  browsing = true;
  let picked = null;
  try {
    picked = await invoke('browse_app');
  } catch (error) {
    if (dialog.open) showProblem(String(error));
  } finally {
    browsing = false;
  }
  if (!picked || !dialog.open || dialog.classList.contains('is-closing')) return;
  remember([picked]);
  picks.set(picked.exe, picked.picked);
  if (!shown.some((a) => a.exe === picked.exe)) shown = [...shown, picked].sort((a, b) => a.name.localeCompare(b.name));
  if (!isTicked(picked.exe)) draft = [...draft, toSave(picked.name, picked.exe)];
  updateFilters();
  renderList();
  showAdded(picked.exe);
});

let problemTimer = 0;

/** Says what went wrong in place of the line under the title, for a few seconds. */
function showProblem(text) {
  clearTimeout(problemTimer);
  subtitle.textContent = text;
  subtitle.classList.add('is-problem');
  problemTimer = setTimeout(clearProblem, 4000);
}

function clearProblem() {
  clearTimeout(problemTimer);
  subtitle.textContent = SUBTITLE;
  subtitle.classList.remove('is-problem');
}

/** Scrolls to the app just added and lights it up once. The list redraws on
 *  every add, so a newer one simply takes over from the last. */
function showAdded(exe) {
  const find = () => list.querySelector(`[data-exe="${CSS.escape(exe)}"]`);
  let row = find();
  if (!row && search.value) {
    search.value = '';
    renderList();
    row = find();
  }
  if (!row) return;
  const offset = row.getBoundingClientRect().top - list.getBoundingClientRect().top;
  list.scrollTo({ top: list.scrollTop + offset - (list.clientHeight - row.offsetHeight) / 2, behavior: 'smooth' });
  row.classList.add('is-added');
}

// ------------------------------------------------------------
// Opening and closing
// ------------------------------------------------------------

async function open() {
  draft = [...skipped()];
  view = 'all';
  search.value = '';
  clearProblem();
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
