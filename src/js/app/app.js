// ============================================================
// app.js
// Starts the window: loads the settings, then opens the right page.
// ============================================================

import '../shared/native.js';
import { app, listen, load } from './state.js';

// The theme goes first, so the page is in the right colors from the start.
import './components/theme.js';
import './components/dialog.js';
import './components/update-bar.js';
import './controls/shapes.js';
import './controls/colors.js';
import './controls/sliders.js';
import './controls/try-box.js';
import './controls/apps.js';
import { show } from './components/sidebar.js';
import { goStep } from './pages/welcome.js';
import './pages/blip.js';
import './pages/settings.js';

await load();

// The tray's "Settings" opens the window straight on that page.
show(app.state.settings.showWelcome ? 'welcome' : (window.__START_PAGE__ ?? 'blip'));
listen('navigate', (e) => !app.state.settings.showWelcome && show(e.payload));
goStep(0);
