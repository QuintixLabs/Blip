// ============================================================
// controls/look/draw.js
// Drawing a blip on a canvas, and telling light colors from dark.
// ============================================================

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
