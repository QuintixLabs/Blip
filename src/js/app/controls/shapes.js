// ============================================================
// shapes.js
// The shape buttons (circle, ring, square…) on the welcome screen
// and the Blip page.
// ============================================================

import { $$, onRender, save } from '../state.js';
import { SHAPES } from '../../shared/draw.js';

function render(state) {
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

onRender(render);
