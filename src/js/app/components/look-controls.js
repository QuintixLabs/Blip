// ============================================================
// look-controls.js
// The controls for how a blip looks: the shape buttons, the color
// swatches and the sliders. Used on the welcome screen and the Blip
// page, wherever they appear.
// ============================================================

import { $$, app, onRender, save } from '../state.js';

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

// ------------------------------------------------------------
// Try it (welcome screen)
// ------------------------------------------------------------

// Real blips only start once the welcome screen is done, so this box shows one
// itself: same shape, color, sizes, opacity and timing as the real thing
// (see blip/style.rs).
const tryBox = $$('#try')[0];

function playBlip(x, y) {
  const s = app.state.settings;
  const canvas = tryBox.querySelector('canvas');
  const scale = window.devicePixelRatio || 1;
  const { width, height } = tryBox.getBoundingClientRect();
  if (canvas.width !== Math.round(width * scale)) {
    canvas.width = Math.round(width * scale);
    canvas.height = Math.round(height * scale);
  }
  const g = canvas.getContext('2d');
  const duration = 1500 / Math.min(Math.max(s.speed, 1), 10);
  const fadeStart = s.fadeStart / 100;
  const started = performance.now();

  const frame = (now) => {
    const t = (now - started) / duration;
    g.setTransform(1, 0, 0, 1, 0, 0);
    g.clearRect(0, 0, canvas.width, canvas.height);
    if (t >= 1) return;
    const eased = 1 - (1 - t) ** 3;
    const size = s.startSize + (s.endSize - s.startSize) * eased;
    const fade = t > fadeStart ? 1 - (t - fadeStart) / (1 - fadeStart) : 1;
    g.setTransform(scale, 0, 0, scale, 0, 0);
    g.globalAlpha = (s.opacity / 100) * Math.min(Math.max(fade, 0), 1);
    drawShape(g, s.shape, s.color, x, y, size);
    requestAnimationFrame(frame);
  };
  requestAnimationFrame(frame);
}

function drawShape(g, shape, color, x, y, size) {
  const r = size / 2;
  g.fillStyle = color;
  g.strokeStyle = color;
  g.beginPath();
  if (shape === 'square') {
    g.roundRect(x - r, y - r, size, size, size * 0.14);
  } else if (shape === 'hexagon' || shape === 'diamond') {
    const sides = shape === 'hexagon' ? 6 : 4;
    const start = shape === 'hexagon' ? 0 : -90;
    for (let i = 0; i < sides; i++) {
      const a = ((start + (i * 360) / sides) * Math.PI) / 180;
      g.lineTo(x + r * Math.cos(a), y + r * Math.sin(a));
    }
    g.closePath();
  } else if (shape === 'ring') {
    const width = Math.max(size * 0.12, 1.5);
    g.lineWidth = width;
    g.arc(x, y, Math.max(r - width / 2, 0.5), 0, Math.PI * 2);
    g.stroke();
    return;
  } else {
    g.arc(x, y, r, 0, Math.PI * 2);
  }
  g.fill();
}

if (tryBox) {
  // On release, like real blips.
  tryBox.addEventListener('pointerup', (e) => {
    const rect = tryBox.getBoundingClientRect();
    playBlip(e.clientX - rect.left, e.clientY - rect.top);
  });
}
onRender((state) => {
  renderShapes(state);
  renderColors(state);
  renderSliders(state);
});
