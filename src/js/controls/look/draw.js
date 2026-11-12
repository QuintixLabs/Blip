// ============================================================
// controls/look/draw.js
// The blip shapes: their button drawings, drawing one on a canvas, and
// telling light colors from dark.
// ============================================================

/** Each shape as a small drawing for its button: [name, svg inside a 16×16 box]. */
export const SHAPES = {
  circle: ['Circle', '<circle cx="8" cy="8" r="6.5"/>'],
  ring: ['Ring', '<circle cx="8" cy="8" r="5.5" fill="none" stroke="currentColor" stroke-width="2"/>'],
  square: ['Square', '<rect x="2" y="2" width="12" height="12" rx="2"/>'],
  hexagon: ['Hexagon', '<polygon points="14.5,8 11.25,13.63 4.75,13.63 1.5,8 4.75,2.37 11.25,2.37"/>'],
  diamond: ['Diamond', '<polygon points="8,1.5 14.5,8 8,14.5 1.5,8"/>'],
};

/** Draws one blip centered on x, y, the same way the app does (see blip/draw.rs). */
export function drawShape(g, shape, color, x, y, size) {
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

export function brightness(hex) {
  const n = parseInt(hex.slice(1), 16);
  const [r, g, b] = [(n >> 16) & 255, (n >> 8) & 255, n & 255];
  return (0.299 * r + 0.587 * g + 0.114 * b) / 255;
}
