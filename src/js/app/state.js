// ============================================================
// state.js
// The settings the window shows, and saving them.
// ============================================================

export const { invoke } = window.__TAURI__.core;
export const { listen } = window.__TAURI__.event;

export const $ = (selector) => document.querySelector(selector);
export const $$ = (selector) => [...document.querySelectorAll(selector)];

/** Everything from the settings file: { settings, … }. */
export const app = { state: null };

// ------------------------------------------------------------
// Redrawing
// ------------------------------------------------------------

const renderers = [];

export function onRender(fn) {
  renderers.push(fn);
}

export function render() {
  for (const fn of renderers) fn(app.state);
}

// ------------------------------------------------------------
// Loading and saving
// ------------------------------------------------------------

export async function load() {
  app.state = await invoke('state');
  listen('state', (e) => {
    app.state = e.payload;
    render();
  });
  render();
}

export async function save(patch) {
  try {
    app.state = await invoke('save_settings', { settings: { ...app.state.settings, ...patch } });
    status();
    render();
    return true;
  } catch (err) {
    status(String(err), true);
    return false;
  }
}

/** The small status line. The welcome screen only ever shows errors there. */
export function status(text, error = false) {
  for (const el of $$('.status')) {
    const quiet = el.closest('.welcome') && !error;
    el.textContent = quiet ? '' : (text ?? '');
    el.classList.toggle('is-error', error);
  }
}
