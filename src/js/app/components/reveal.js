// ============================================================
// reveal.js
// Opens and closes notes and warnings smoothly (the motion itself
// is in components/reveal.css). A closed one can't be reached with
// the keyboard either.
// ============================================================

/** Opens or closes a `.reveal`. `animate: false` puts it in place at once. */
export function reveal(el, open, { animate = true } = {}) {
  if (!animate) {
    el.classList.add('no-transition');
    el.classList.toggle('is-open', open);
    void el.offsetHeight; // apply it now, before transitions come back
    el.classList.remove('no-transition');
  } else {
    el.classList.toggle('is-open', open);
  }
  el.inert = !open;
}