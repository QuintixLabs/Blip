// ============================================================
// try-box.js
// "Click here to see your blip" on the welcome screen. Real blips
// only start once the welcome screen is done, so this box plays
// one itself: same shape, color, sizes, opacity and timing as the
// real thing (see blip/style.rs).
// ============================================================

import { $, app, onRender } from '../state.js';
import { brightness, drawShape } from '../../shared/draw.js';

const box = $('#try');

function play(x, y) {
  const s = app.state.settings;
  const canvas = box.querySelector('canvas');
  const scale = window.devicePixelRatio || 1;
  const { width, height } = box.getBoundingClientRect();
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

/** Light colors get a dark box and dark ones a light box, so every blip shows. */
function render(state) {
  box.classList.toggle('is-dark', brightness(state.settings.color) > 0.6);
}

if (box) {
  // On release, like real blips.
  box.addEventListener('pointerup', (e) => {
    const rect = box.getBoundingClientRect();
    play(e.clientX - rect.left, e.clientY - rect.top);
  });
  onRender(render);
}
