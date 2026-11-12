// ============================================================
// sliders.js
// The sliders: the value next to each, saved when you let go.
// ============================================================

import { $$, onRender, save } from '../state.js';

function showValue(input) {
  const output = input.parentElement.querySelector('output');
  if (output) output.textContent = `${input.value}${output.dataset.unit ?? ''}`;
  const fill = ((input.value - input.min) / (input.max - input.min)) * 100;
  input.style.setProperty('--fill', `${fill}%`);
}

function render(state) {
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

onRender(render);
