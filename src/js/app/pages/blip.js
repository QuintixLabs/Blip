// ============================================================
// blip.js
// The Blip page: how a blip looks and where it doesn't show. The
// controls themselves are in components/look-controls.js; this has
// the Reset to defaults button and the note shown when blips can't
// work on this system.
// ============================================================

import { $, $$, app, invoke, render, status } from '../state.js';
import { confirmAction } from '../components/dialog.js';

// The note about the taskbar and Start menu is about Windows only.
for (const el of $$('[data-windows-only]')) el.hidden = !navigator.userAgent.includes('Windows');

const reason = await invoke('blip_status');
for (const el of $$('[data-unsupported]')) {
  el.textContent = reason ?? '';
  el.hidden = !reason;
}

$('#reset-look').addEventListener('click', async () => {
  const ok = await confirmAction({
    title: 'Reset to defaults?',
    text: 'The shape, color, sizes and the "Don\'t show blips" choices go back to how Blip comes.',
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
