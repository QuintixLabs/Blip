// ============================================================
// look-controls.js
// The controls for how a blip looks: the shape buttons, the color
// swatches and the sliders. Used on the welcome screen and the Blip
// page, wherever they appear.
// ============================================================

import { $$, onRender, save } from '../state.js';

// ------------------------------------------------------------
// Shapes
// ------------------------------------------------------------

/** Each shape as a small drawing for its button. */
const SHAPES = {
  circle: ['Circle', '<circle cx="8" cy="8" r="6.5"/>'],
  ring: ['Ring', '<circle cx="8" cy="8" r="5.5" fill="none" stroke="currentColor" stroke-width="2"/>'],
  square: ['Square', '<rect x="2" y="2" width="12" height="12" rx="2"/>'],
  hexagon: ['Hexagon', '<polygon points="14.5,8 11.25,13.63 4.75,13.63 1.5,8 4.75,2.37 11.25,2.37"/>'],
  diamond: ['Diamond', '<polygon points="8,1.5 14.5,8 8,14.5 1.5,8"/>'],
};

function renderShapes(state) {
  for (const el of $$('[data-shapes]')) {
    el.innerHTML = Object.entries(SHAPES)
      .map(([value, [name, art]]) => {
        const active = value === state.settings.shape ? 'is-active' : '';
        return `<button class="${active}" data-value="${value}" title="${name}" aria-label="${name}"><svg viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">${art}</svg></button>`;
      })
      .join('');
  }
}

for (const el of $$('[data-shapes]')) {
  el.addEventListener('click', (e) => {
    const button = e.target.closest('[data-value]');
    if (button) save({ shape: button.dataset.value });
  });
}

// ------------------------------------------------------------
// Colors
// ------------------------------------------------------------

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

function renderColors(state) {
  const current = state.settings.color;
  const custom = !PRESETS.some(([hex]) => sameColor(hex, current));
  for (const el of $$('[data-colors]')) {
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
    if (button) save({ color: button.dataset.color });
  });
  // The system color picker: saved once you've chosen.
  el.addEventListener('change', (e) => {
    if (e.target.type === 'color') save({ color: e.target.value.toUpperCase() });
  });
}

// ------------------------------------------------------------
// Sliders
// ------------------------------------------------------------

function showValue(input) {
  const output = input.parentElement.querySelector('output');
  if (output) output.textContent = `${input.value}${output.dataset.unit ?? ''}`;
  const fill = ((input.value - input.min) / (input.max - input.min)) * 100;
  input.style.setProperty('--fill', `${fill}%`);
}

function renderSliders(state) {
  for (const input of $$('.slider input[data-field]')) {
    // Leave a slider alone while it's being dragged.
    if (document.activeElement === input && input.matches(':active')) continue;
    input.value = state.settings[input.dataset.field];
    showValue(input);
  }
}

for (const input of $$('.slider input[data-field]')) {
  input.addEventListener('input', () => showValue(input));
  input.addEventListener('change', () => save({ [input.dataset.field]: Number(input.value) }));
}

onRender((state) => {
  renderShapes(state);
  renderColors(state);
  renderSliders(state);
});
