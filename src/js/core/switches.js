// ============================================================
// core/switches.js
// The switches and choice buttons on every page.
// ============================================================

import { $$, app, onRender, save } from './state.js';

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

  // data-invert: the switch shows the opposite of the setting ("Show on"
  // switches are on when blips show, while the setting says where they're skipped).
  for (const el of $$('.toggle')) {
    const value = Boolean(state.settings[el.dataset.field]);
    el.setAttribute('aria-checked', String('invert' in el.dataset ? !value : value));
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
