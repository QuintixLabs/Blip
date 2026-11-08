// ============================================================
// sidebar.js
// The sidebar: switching pages and collapsing it to icons.
// ============================================================

import { $, $$, status } from '../state.js';

const root = document.documentElement;

// ------------------------------------------------------------
// Pages
// ------------------------------------------------------------

/** Shows a page: "welcome", "blip" or "settings". */
export function show(page) {
  status();
  root.classList.toggle('is-welcome', page === 'welcome');
  $$('.view').forEach((el) => (el.hidden = el.id !== page));
  $$('[data-tab]').forEach((tab) => tab.classList.toggle('is-active', tab.dataset.tab === page));
  $('main').scrollTop = 0;
}

for (const tab of $$('[data-tab]')) {
  tab.addEventListener('click', () => show(tab.dataset.tab));
}

// ------------------------------------------------------------
// Collapsing (remembered per computer; boot.js restores it first thing)
// ------------------------------------------------------------

const isCollapsed = () => root.classList.contains('is-collapsed');
const updateToggleTitle = () => ($('#sidebar-toggle').title = isCollapsed() ? 'Expand sidebar' : 'Collapse sidebar');

updateToggleTitle();
// Animate only when you click, not the saved state on open.
requestAnimationFrame(() => requestAnimationFrame(() => root.classList.add('can-animate')));

$('#sidebar-toggle').addEventListener('click', () => {
  const collapsed = !isCollapsed();
  root.classList.toggle('is-collapsed', collapsed);
  updateToggleTitle();
  try {
    localStorage.setItem('blip.sidebarCollapsed', collapsed ? '1' : '0');
  } catch {}

  // The mouse is still over the logo right after clicking; ignore that hover
  // until it leaves, or the logo and the toggle would flicker.
  root.classList.add('hover-lock');
  $('.brand').addEventListener('mouseleave', () => root.classList.remove('hover-lock'), { once: true });
});

