// ============================================================
// blip.js
// The Blip page: how a blip looks and where it shows. The controls
// themselves are in controls/ (shapes, colors, sliders, apps); this has
// the taskbar warning, Reset to defaults, the note shown when blips
// can't work on this system, and the Allow button some Linux desktops need.
// ============================================================

import { $, $$, app, invoke, onRender, render, status } from '../state.js';
import { confirmAction } from '../components/dialog.js';

// Windows only, and only while blips are switched on for the taskbar: then it
// says why they'll show up underneath it.
const windows = navigator.userAgent.includes('Windows');
onRender((state) => {
  for (const el of $$('[data-taskbar-warning]')) el.hidden = !(windows && !state.settings.skipTaskbar);
});

// Linux has panels rather than a Start menu.
if (!windows) {
  for (const el of $$('[data-feature="taskbar"] .row-hint')) el.textContent = 'Panels and the taskbar';
}

// ------------------------------------------------------------
// What works on this system
// ------------------------------------------------------------

/** Shows why blips can't work here, or the Allow button when they only need your OK.
 *  Until then the welcome screen can't go on: without it there would be no blips. */
function showNotice({ reason, needsAccess }, error = '') {
  const waiting = !reason && needsAccess && !error.startsWith('Allowed');
  document.documentElement.classList.toggle('needs-access', waiting);
  for (const el of $$('[data-unsupported]')) {
    el.hidden = !reason && !needsAccess;
    el.classList.toggle('notice-action', !reason && needsAccess);
    if (reason) {
      el.textContent = reason;
    } else if (needsAccess) {
      el.innerHTML =
        '<span>Blip needs your OK to see mouse clicks on this system. Your password is asked once.</span>' +
        '<button class="btn btn-accent btn-sm" data-allow>Allow</button>' +
        (error ? `<span class="notice-error"></span>` : '');
      if (error) el.querySelector('.notice-error').textContent = error;
    }
  }
}

const info = await invoke('blip_info');
showNotice(info);

// Rules this system can't check aren't shown at all (Linux on Wayland can't
// tell which cursor is showing, for one).
for (const el of $$('[data-feature]')) {
  if (!info.features[el.dataset.feature]) el.remove();
}
if (!Object.values(info.features).some(Boolean)) {
  for (const el of $$('[data-feature-section]')) el.remove();
}

document.addEventListener('click', async (e) => {
  const button = e.target.closest('[data-allow]');
  if (!button) return;
  button.disabled = true;
  let error = '';
  try {
    await invoke('blip_allow_access');
  } catch (err) {
    error = String(err);
  }
  showNotice(await invoke('blip_info'), error);
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
