// ============================================================
// reveal.js
// Warnings and notes that come and go: they open by growing to
// their height while fading in, and close the same way back, so
// nothing under them jumps. A note whose text changes eases to its
// new height.
// ============================================================

const MS = 240;
const EASE = 'cubic-bezier(0.16, 1, 0.3, 1)';

/** Everything that takes up room, so it can all start (or end) at nothing. */
const ROOM = ['height', 'marginTop', 'marginBottom', 'paddingTop', 'paddingBottom', 'borderTopWidth', 'borderBottomWidth'];

function measure(el) {
  const style = getComputedStyle(el);
  const frame = { opacity: 1 };
  for (const key of ROOM) frame[key] = key === 'height' ? `${el.offsetHeight}px` : style[key];
  return frame;
}

const closed = () => Object.fromEntries([...ROOM.map((key) => [key, '0px']), ['opacity', 0]]);

/** Shows or hides `el`. `animate: false` does it at once (the first look at a page). */
export function reveal(el, show, { animate = true } = {}) {
  const wasShown = !el.hidden && !el.dataset.closing;
  if (show === wasShown) return;
  el.getAnimations().forEach((a) => a.cancel());
  delete el.dataset.closing;
  if (!animate) {
    el.hidden = !show;
    return;
  }
  if (show) {
    el.hidden = false;
    el.style.overflow = 'hidden';
    el.animate([closed(), measure(el)], { duration: MS, easing: EASE }).finished.then(
      () => (el.style.overflow = ''),
      () => {},
    );
  } else {
    el.dataset.closing = '1';
    el.style.overflow = 'hidden';
    const out = el.animate([measure(el), closed()], { duration: MS * 0.8, easing: EASE, fill: 'forwards' });
    const done = () => {
      if (!el.dataset.closing) return; // opened again meanwhile
      el.hidden = true;
      el.style.overflow = '';
      delete el.dataset.closing;
      out.cancel();
    };
    out.finished.then(done, () => {});
    // Animations pause while the window is hidden; it still ends up closed.
    setTimeout(done, MS + 100);
  }
}

/** Changes what's inside `el` (with `change`), easing from its old height to the new one. */
export function morph(el, change) {
  if (el.hidden) {
    change();
    return;
  }
  const before = el.offsetHeight;
  change();
  const after = el.offsetHeight;
  el.getAnimations().forEach((a) => a.cancel());
  el.style.overflow = 'hidden';
  el.animate(
    [
      { height: `${before}px`, opacity: 0.35 },
      { height: `${after}px`, opacity: 1 },
    ],
    { duration: MS, easing: EASE },
  ).finished.then(
    () => (el.style.overflow = ''),
    () => {},
  );
}
