// ============================================================
// pages/settings.js
// The Settings page: Reset, the settings folder and the links.
// ============================================================

import { $, $$, app, invoke, render, status } from '../core/state.js';
import { confirmAction } from '../window/dialog.js';

// macOS keeps tray icons in the menu bar.
if (navigator.userAgent.includes('Mac')) {
  for (const el of $$('[data-tray-hint]')) el.textContent = 'Show Blip in the menu bar';
}

// ------------------------------------------------------------
// Reset
// ------------------------------------------------------------

$('#reset').addEventListener('click', async () => {
  const ok = await confirmAction({
    title: 'Reset all settings?',
    text: 'All settings go back to their defaults.',
    action: 'Reset',
  });
  if (!ok) return;
  try {
    app.state = await invoke('reset_settings');
    status();
    render();
  } catch (err) {
    status(String(err), true);
  }
});

// ------------------------------------------------------------
// Folder and links
// ------------------------------------------------------------

$('#open-folder').addEventListener('click', () => invoke('open_settings_folder'));

for (const button of $$('[data-link]')) {
  button.addEventListener('click', () => invoke('open_link', { name: button.dataset.link }));
}
