// ============================================================
// main.js
// Starts the window: loads the settings, then opens the right page.
// ============================================================

import './core/native.js';
import { app, listen, load } from './core/state.js';

// The theme goes first, so the page is in the right colors from the start.
import './window/theme.js';
import './core/switches.js';
import './window/dialog.js';
import './window/update-bar.js';
import './controls/look/shapes.js';
import './controls/look/colors.js';
import './controls/look/sliders.js';
import './controls/look/try-box.js';
import './controls/apps/chips.js';
import './controls/apps/picker.js';
import { show } from './window/sidebar.js';
import { goStep } from './pages/welcome.js';
import './pages/blip.js';
import './pages/settings.js';

await load();

// The tray's "Settings" opens the window straight on that page.
show(app.state.settings.showWelcome ? 'welcome' : (window.__START_PAGE__ ?? 'blip'));
listen('navigate', (e) => !app.state.settings.showWelcome && show(e.payload));
goStep(0);
