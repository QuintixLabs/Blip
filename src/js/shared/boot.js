// ============================================================
// boot.js
// Sets the theme and the sidebar before the window first appears, so it
// never flashes the wrong one.
// ============================================================

// Loaded as a plain script in <head>, before anything is drawn.
try {
  const root = document.documentElement;
  const theme = localStorage.getItem('blip.theme') ?? 'system';
  // On Linux the app says whether the desktop is light (window.__SYSTEM_LIGHT__).
  const systemLight = window.__SYSTEM_LIGHT__ ?? matchMedia('(prefers-color-scheme: light)').matches;
  const light = theme === 'light' || (theme === 'system' && systemLight);
  root.dataset.theme = light ? 'light' : 'dark';

  if (localStorage.getItem('blip.sidebarCollapsed') === '1') {
    root.classList.add('is-collapsed');
  }
  if (window.__START_PAGE__ === 'welcome') {
    root.classList.add('is-welcome');
  }
  // Until pages/blip.js knows whether blips work here (it may need your OK first).
  root.classList.add('checking-blips');
} catch {}
