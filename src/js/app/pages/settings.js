// ============================================================
// settings.js
// Switches and choices (on every page), the reset button, the
// settings folder and the links at the bottom of Settings.
// ============================================================

import { $, $$, app, invoke, onRender, render, save, status } from '../state.js';
import { confirmAction } from '../components/dialog.js';

// ------------------------------------------------------------
// Switches and choices
// ------------------------------------------------------------

// data-options is "value:label,…", or "value:icon:name,…" with data-kind="icons".
function renderChoices(state) {
  for (const el of $$('.segmented[data-field]')) {
    const current = String(state.settings[el.dataset.field]);
    const icons = el.dataset.kind === 'icons';
    el.innerHTML = el.dataset.options
      .split(',')
      .map((option) => {
        const [value, label, name] = option.split(':');
        const content = icons ? `<i class="icon icon-${label}"></i>` : label;
        const title = icons ? ` title="${name}" aria-label="${name}"` : '';
        return `<button class="${value === current ? 'is-active' : ''}" data-value="${value}"${title}>${content}</button>`;
      })
      .join('');
  }

  for (const el of $$('.toggle')) {
    el.setAttribute('aria-checked', String(Boolean(state.settings[el.dataset.field])));
  }
}

onRender(renderChoices);

for (const el of $$('.segmented[data-field]')) {
  el.addEventListener('click', (e) => {
    const button = e.target.closest('[data-value]');
    if (!button) return;
    const raw = button.dataset.value;
    save({ [el.dataset.field]: Number.isNaN(Number(raw)) ? raw : Number(raw) });
  });
}

for (const el of $$('.toggle')) {
  el.addEventListener('click', () => save({ [el.dataset.field]: !app.state.settings[el.dataset.field] }));
}

// ------------------------------------------------------------
// Reset
// ------------------------------------------------------------

$('#reset').addEventListener('click', async () => {
  const ok = await confirmAction({
    title: 'Reset all settings?',
    text: 'The look, the rules and everything else go back to their defaults.',
    action: 'Reset',
  });
  if (!ok) return;
  try {
    app.state = await invoke('reset_settings');
    status();
    render();
  } catch (err) {
    status(String(err), true);
  }
});

// ------------------------------------------------------------
// Folder and links
// ------------------------------------------------------------

$('#open-folder').addEventListener('click', () => invoke('open_settings_folder'));

for (const button of $$('[data-link]')) {
  button.addEventListener('click', () => invoke('open_link', { name: button.dataset.link }));
}
