// ============================================================
// controls/look/shapes.js
// The shape buttons.
// ============================================================

import { $$, onRender, save } from '../../core/state.js';

/** Each one's drawing is assets/icons/shapes/<value>.svg. */
const SHAPES = { circle: 'Circle', ring: 'Ring', square: 'Square', hexagon: 'Hexagon', diamond: 'Diamond' };

function render(state) {
  for (const el of $$('[data-shapes]')) {
    el.innerHTML = Object.entries(SHAPES)
      .map(([value, name]) => {
        const active = value === state.settings.shape ? 'is-active' : '';
        return `<button class="${active}" data-value="${value}" title="${name}" aria-label="${name}"><i class="icon icon-shape-${value}"></i></button>`;
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

onRender(render);
