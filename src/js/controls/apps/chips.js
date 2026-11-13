// ============================================================
// controls/apps/chips.js
// The chips under the Apps row: one per skipped app.
// ============================================================

import { $, app, invoke, onRender, save } from '../../core/state.js';
import { reveal } from '../../window/reveal.js';
import { escape, findApps, findPicked, icons, iconHtml, paths, skipped } from './apps.js';

const chipsWrap = $('#app-chips-wrap');
const chips = $('#app-chips');

// Chips are added and removed one by one, so a removed one can fade out.

let chipsShown = false; // the page as it opens: no animation

function makeChip(a) {
  const chip = document.createElement('span');
  chip.className = 'app-chip';
  chip.dataset.exe = a.exe;
  chip.innerHTML = `${iconHtml(a.exe)}<span class="app-chip-name">${escape(a.name)}</span><button title="Remove" aria-label="Remove ${escape(a.name)}"><i class="icon icon-x"></i></button>`;
  return chip;
}

const EASE = 'cubic-bezier(0.4, 0, 0.2, 1)';

/** A quick fade, then gone. The chips after it glide into its place, and when a
 *  whole row empties, the chip area eases to its new height instead of jumping. */
function leave(chip) {
  chip.dataset.leaving = '1';
  let done = false;
  const remove = () => {
    if (done) return;
    done = true;
    const others = [...chips.children].filter((c) => c !== chip);
    const before = new Map(others.map((c) => [c, c.getBoundingClientRect()]));
    const height = chips.offsetHeight;
    // Hold the area at its height while the chip goes. Letting the page get
    // shorter here, even for a moment, makes it jump when you're at the bottom.
    chips.style.height = `${height}px`;
    chip.remove();
    for (const c of others) {
      const was = before.get(c);
      const now = c.getBoundingClientRect();
      const dx = was.left - now.left;
      const dy = was.top - now.top;
      if (dx || dy) {
        c.animate([{ transform: `translate(${dx}px, ${dy}px)` }, { transform: 'none' }], { duration: 180, easing: EASE });
      }
    }
    const newHeight = naturalHeight();
    if (newHeight >= height) {
      chips.style.height = '';
      return;
    }
    keepPageLength(height - newHeight);
    // First the chips glide while the area keeps its size, then the area eases to
    // its new height: done together, it would cut off a chip that's still moving.
    chips.animate([{ height: `${height}px` }, { height: `${newHeight}px` }], { duration: 220, delay: 180, easing: EASE, fill: 'backwards' }).finished.then(
      () => (chips.style.height = ''),
      () => (chips.style.height = ''),
    );
  };
  chip.animate([{ opacity: 1 }, { opacity: 0 }], { duration: 150, easing: 'ease-out', fill: 'forwards' }).finished.then(remove, remove);
  // Animations pause while the window is hidden; it still goes.
  setTimeout(remove, 300);
}

/** The chip area's height for the chips in it now, worked out from where they
 *  are (asking the browser would shorten the page early). */
function naturalHeight() {
  const box = chips.getBoundingClientRect();
  const bottom = Math.max(box.top, ...[...chips.children].map((c) => c.getBoundingClientRect().bottom));
  return Math.round(bottom - box.top + parseFloat(getComputedStyle(chips).paddingBottom));
}

// ------------------------------------------------------------
// Keeping the page still at the bottom
// ------------------------------------------------------------

// Scrolled all the way down, a page that gets shorter pulls everything above
// it down. So while the chip area shrinks, the same room is added under the
// page, and nothing you can see moves. That room goes once it's out of sight:
// when you scroll up or open another page.

const main = $('main');
let extraRoom = 0;
let basePadding = null;

function keepPageLength(lost) {
  const atBottom = main.scrollTop + main.clientHeight >= main.scrollHeight - 2;
  if (!atBottom) return;
  basePadding ??= parseFloat(getComputedStyle(main).paddingBottom);
  extraRoom += lost;
  main.style.paddingBottom = `${basePadding + extraRoom}px`;
}

main.addEventListener('scroll', () => {
  if (!extraRoom) return;
  const outOfSight = main.scrollTop + main.clientHeight < main.scrollHeight - extraRoom - 1;
  if (outOfSight || $('#blip').hidden) {
    extraRoom = 0;
    main.style.paddingBottom = '';
  }
});

// ------------------------------------------------------------
// Drawing them
// ------------------------------------------------------------

function renderChips(state) {
  const apps = state.settings.skipApps;
  const animate = chipsShown;
  reveal(chipsWrap, apps.length > 0, { animate });
  const existing = new Map([...chips.children].map((el) => [el.dataset.exe, el]));
  for (const [exe, chip] of existing) {
    if (!apps.some((a) => a.exe === exe) && !chip.dataset.leaving) {
      if (animate) leave(chip);
      else chip.remove();
    }
  }
  for (const a of apps) {
    let chip = existing.get(a.exe);
    if (!chip || chip.dataset.leaving) {
      chip = makeChip(a);
      chips.append(chip);
    } else if (icons.has(a.exe) && !chip.querySelector('img')) {
      chip.querySelector('.app-icon').outerHTML = iconHtml(a.exe);
    }
    chip.classList.toggle('has-folder', paths.has(a.exe));
  }
  chipsShown = true;
}

chips.addEventListener('click', (e) => {
  const chip = e.target.closest('.app-chip');
  if (!chip) return;
  if (e.target.closest('button')) {
    save({ skipApps: skipped().filter((a) => a.exe !== chip.dataset.exe) });
    return;
  }
  const path = paths.get(chip.dataset.exe);
  if (path) invoke('reveal_app', { path }).catch(() => {});
});

// Looked up early, so the list opens straight away.
onRender(renderChips);
let pickedAsked = false;
onRender(() => {
  if (pickedAsked) return;
  pickedAsked = true;
  findPicked().then(() => renderChips(app.state));
});
setTimeout(() => findApps().then(() => renderChips(app.state)), 400);
