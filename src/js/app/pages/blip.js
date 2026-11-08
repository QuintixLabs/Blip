// ============================================================
// blip.js
// The Blip page: how a blip looks and where it doesn't show. The
// controls themselves are in components/look-controls.js; this
// shows a note when blips can't work on this system.
// ============================================================

import { $$, invoke } from '../state.js';

const reason = await invoke('blip_status');
for (const el of $$('[data-unsupported]')) {
  el.textContent = reason ?? '';
  el.hidden = !reason;
}
