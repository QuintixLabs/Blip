// ============================================================
// blip.js
// The Blip page: the taskbar warning, Reset to defaults, and the note
// when blips can't work here or need the user's OK.
// ============================================================

import { $, $$, app, invoke, onRender, render, status } from '../state.js';
import { confirmAction } from '../components/dialog.js';
import { reveal } from '../components/reveal.js';

// Windows only, and only while blips are switched on for the taskbar: then it
// says why they'll show up underneath it.
const windows = navigator.userAgent.includes('Windows');
let firstLook = true; // the page as it opens: no animation
onRender((state) => {
  for (const el of $$('[data-taskbar-warning]')) {
    reveal(el, windows && !state.settings.skipTaskbar, { animate: !firstLook });
  }
  firstLook = false;
});

// Linux has panels rather than a Start menu.
if (!windows) {
  for (const el of $$('[data-feature="taskbar"] .row-hint')) el.textContent = 'Panels and the taskbar';
}

// ------------------------------------------------------------
// What works on this system
// ------------------------------------------------------------

// If the check itself fails, carry on as if blips work: never leave the welcome stuck.
const EVERYTHING = { text: true, links: true, taskbar: true, games: true, apps: true };
const FALLBACK = { reason: null, needsAccess: false, features: EVERYTHING };
const getInfo = () => invoke('blip_info').catch(() => FALLBACK);

function accessNote(note, info) {
  if (note.querySelector('[data-allow]')) return;
  note.classList.add('notice-action');
  note.innerHTML = `
    <span class="notice-text"></span>
    <button class="btn btn-accent btn-sm allow-button" data-allow>
      <span class="when-ready"></span><span class="when-waiting">Waiting…</span>
    </button>
    <div class="reveal notice-error"><div class="reveal-inner"><span class="notice-error-text"></span></div></div>`;
  note.querySelector('.notice-text').textContent = info.accessText;
  note.querySelector('.when-ready').textContent = info.accessButton;
}

/** Shows why blips can't work here, or the button when they only need your OK.
 *  Until then the welcome screen can't go on: without it there would be no blips. */
function showNotice(info, error = '') {
  const { reason, needsAccess } = info;
  const waiting = !reason && needsAccess && !error.startsWith('Allowed');
  document.documentElement.classList.toggle('needs-access', waiting);
  for (const el of $$('.welcome .step-actions')) el.inert = waiting;

  for (const wrap of $$('[data-unsupported]')) {
    const note = wrap.querySelector('.notice');
    if (reason) {
      note.classList.remove('notice-action');
      note.textContent = reason;
    } else if (needsAccess) {
      accessNote(note, info);
      if (error) note.querySelector('.notice-error-text').textContent = error;
      reveal(note.querySelector('.notice-error'), Boolean(error));
    }
    reveal(wrap, Boolean(reason) || needsAccess);
  }
}

function setWaiting(on) {
  for (const button of $$('[data-allow]')) {
    button.disabled = on;
    button.classList.toggle('is-waiting', on);
  }
}

const info = await getInfo();
showNotice(info);
// Known now: the welcome buttons may fade in (see welcome.css).
document.documentElement.classList.remove('checking-blips');

// Rules this system can't check aren't shown at all (Linux on Wayland can't
// tell which cursor is showing, for one).
for (const el of $$('[data-feature]')) {
  if (!info.features[el.dataset.feature]) el.remove();
}
if (!Object.values(info.features).some(Boolean)) {
  for (const el of $$('[data-feature-section]')) el.remove();
}

const sleep = (ms) => new Promise((done) => setTimeout(done, ms));

document.addEventListener('click', async (e) => {
  if (!e.target.closest('[data-allow]')) return;
  setWaiting(true);
  let error = '';
  try {
    await invoke('blip_allow_access');
  } catch (err) {
    error = String(err);
  }
  let now = await getInfo();
  // macOS: you switch it on in System Settings, so keep checking for a while.
  for (let tries = 0; !error && now.needsAccess && now.accessWaits && tries < 150; tries++) {
    await sleep(1500);
    now = await getInfo();
  }
  showNotice(now, error);
  setWaiting(false);
});
$('#reset-look').addEventListener('click', async () => {
  const ok = await confirmAction({
    title: 'Reset to defaults?',
    text: 'All settings on this page go back to their defaults.',
    action: 'Reset',
  });
  if (!ok) return;
  try {
    app.state = await invoke('reset_look');
    status();
    render();
  } catch (err) {
    status(String(err), true);
  }
});
