// ============================================================
// colors.js
// The color dots: ready colors and Custom (the system's color picker).
// ============================================================

import { $$, onRender, save } from '../state.js';

/** A few that work well; "Custom" opens the system's color picker for anything else. */
const PRESETS = [
  ['#000000', 'Black'],
  ['#FFFFFF', 'White'],
  ['#4FC4FF', 'Blue'],
  ['#A78BFA', 'Purple'],
  ['#F472B6', 'Pink'],
  ['#FF5C5C', 'Red'],
  ['#FFB020', 'Orange'],
  ['#3DD68C', 'Green'],
];

const sameColor = (a, b) => a.toUpperCase() === b.toUpperCase();

const SWITCH = { color: 'leftClick', rightColor: 'rightClick', middleColor: 'middleClick' };

function render(state) {
  for (const el of $$('[data-colors]')) {
    // A button that's switched off: its colors can't be picked (they're faded too).
    if (el.closest('.button-row')) el.inert = !state.settings[SWITCH[el.dataset.colors]];
    const current = state.settings[el.dataset.colors];
    const custom = !PRESETS.some(([hex]) => sameColor(hex, current));
    const swatches = PRESETS.map(([hex, name]) => {
      const active = sameColor(hex, current) ? 'is-active' : '';
      return `<button class="swatch ${active}" data-color="${hex}" style="--swatch:${hex}" title="${name}" aria-label="${name}"></button>`;
    }).join('');
    const customStyle = custom ? ` style="--swatch:${current}"` : '';
    el.innerHTML = `${swatches}<label class="swatch swatch-custom ${custom ? 'is-active is-set' : ''}"${customStyle} title="Custom color"><input type="color" value="${current.toLowerCase()}" aria-label="Custom color" /></label>`;
  }
}

for (const el of $$('[data-colors]')) {
  el.addEventListener('click', (e) => {
    const button = e.target.closest('[data-color]');
    if (button) save({ [el.dataset.colors]: button.dataset.color });
  });
  el.addEventListener('change', (e) => {
    if (e.target.type === 'color') save({ [el.dataset.colors]: e.target.value.toUpperCase() });
  });
}

onRender(render);
