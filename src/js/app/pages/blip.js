// ============================================================
// blip.js
// The Blip page: how a blip looks and where it shows. The controls
// themselves are in controls/ (shapes, colors, sliders, apps); this has the taskbar warning, Reset to defaults and the
// note shown when blips can't work on this system.
// ============================================================

import { $, $$, app, invoke, onRender, render, status } from '../state.js';
import { confirmAction } from '../components/dialog.js';

// Windows only, and only while blips are switched on for the taskbar: then it
// says why they'll show up underneath it.
const windows = navigator.userAgent.includes('Windows');
onRender((state) => {
  for (const el of $$('[data-taskbar-warning]')) el.hidden = !(windows && !state.settings.skipTaskbar);
});

const reason = await invoke('blip_status');
for (const el of $$('[data-unsupported]')) {
  el.textContent = reason ?? '';
  el.hidden = !reason;
}

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
