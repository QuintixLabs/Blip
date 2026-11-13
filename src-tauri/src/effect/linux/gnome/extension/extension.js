// ============================================================
// effect/linux/gnome/extension/extension.js
// Runs inside GNOME Shell: notices clicks, asks Blip whether each one
// gets a blip and how it looks, then draws it. Once Blip is uninstalled,
// removes itself at the next login.
// ============================================================

import Cairo from 'cairo';
import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Shell from 'gi://Shell';
import St from 'gi://St';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import { Extension } from 'resource:///org/gnome/shell/extensions/extension.js';

const BUS = 'com.quintixlabs.blip.Gnome';
// Blip's numbers for the buttons: 0 left, 1 right, 2 middle. On Wayland,
// GNOME marks the right button as BUTTON2 and the middle as BUTTON3.
const BUTTONS = [
  [Clutter.ModifierType.BUTTON1_MASK, 0],
  [Clutter.ModifierType.BUTTON2_MASK, 1],
  [Clutter.ModifierType.BUTTON3_MASK, 2],
];
/** Moving further than this (px) while held is a drag. */
const DRAG = 8;
/** How often the buttons are checked (ms), only while Blip is running. */
const POLL_MS = 6;

// ------------------------------------------------------------
// After Blip is uninstalled
// ------------------------------------------------------------

/** Blip saves where it is next to this file. True when that's gone. */
function blipGone(folder) {
  try {
    const [, bytes] = GLib.file_get_contents(GLib.build_filenamev([folder, 'blip-path']));
    const program = new TextDecoder().decode(bytes).trim();
    return program !== '' && !GLib.file_test(program, GLib.FileTest.EXISTS);
  } catch {
    return false; // not saved (yet): leave everything as it is
  }
}

/** Takes this extension off GNOME's list and removes its files. */
function cleanUp(uuid, folder) {
  if (Gio.SettingsSchemaSource.get_default().lookup('org.gnome.shell', true)) {
    const shell = new Gio.Settings({ schema_id: 'org.gnome.shell' });
    shell.set_strv('enabled-extensions', shell.get_strv('enabled-extensions').filter((id) => id !== uuid));
  }
  for (const name of ['metadata.json', 'extension.js', 'blip-path']) {
    try {
      Gio.File.new_for_path(GLib.build_filenamev([folder, name])).delete(null);
    } catch {}
  }
  try {
    Gio.File.new_for_path(folder).delete(null);
  } catch {}
}

export default class BlipExtension extends Extension {
  enable() {
    if (blipGone(this.path)) {
      cleanUp(this.uuid, this.path);
      return;
    }
    this._held = [null, null, null];
    this._blips = new Set();
    this._timer = 0;
    this._watch = Gio.bus_watch_name(Gio.BusType.SESSION, BUS, Gio.BusNameWatcherFlags.NONE, () => this._start(), () => this._stop());
  }

  disable() {
    if (!this._watch) return;
    Gio.bus_unwatch_name(this._watch);
    this._watch = 0;
    this._stop();
    for (const blip of this._blips) blip.destroy();
    this._blips.clear();
  }

  _start() {
    if (this._timer) return;
    this._timer = GLib.timeout_add(GLib.PRIORITY_DEFAULT, POLL_MS, () => {
      this._poll();
      return GLib.SOURCE_CONTINUE;
    });
  }

  _stop() {
    if (this._timer) GLib.source_remove(this._timer);
    this._timer = 0;
    this._held = [null, null, null];
  }

  // GNOME doesn't tell extensions about clicks in app windows, but it does say
  // which buttons are down, so a press and release are spotted here.
  _poll() {
    const [x, y, mods] = global.get_pointer();
    for (const [mask, button] of BUTTONS) {
      const down = (mods & mask) !== 0;
      const held = this._held[button];
      if (down && !held) {
        this._held[button] = [x, y];
      } else if (!down && held) {
        this._held[button] = null;
        if ((x - held[0]) ** 2 + (y - held[1]) ** 2 <= DRAG * DRAG) this._click(button, x, y);
      }
    }
  }

  _click(button, x, y) {
    const [pid, app] = windowAt(x, y);
    const args = new GLib.Variant('(uddusb)', [button, x, y, pid, app, onShell(x, y)]);
    Gio.DBus.session.call(BUS, '/Gnome', BUS, 'Click', args, new GLib.VariantType('(s)'), Gio.DBusCallFlags.NONE, 500, null, (bus, result) => {
      try {
        const [reply] = bus.call_finish(result).deepUnpack();
        if (reply) this._show(x, y, JSON.parse(reply));
      } catch (e) {
        // Blip went away mid-click: no blip.
      }
    });
  }

  /** The same growing, fading shape as everywhere else (effect/shape in Blip). */
  _show(x, y, style) {
    const side = Math.ceil(Math.max(style.start, style.end)) + 4;
    const area = new St.DrawingArea({ width: side, height: side, reactive: false });
    area.set_position(Math.round(x - side / 2), Math.round(y - side / 2));
    Main.layoutManager.uiGroup.add_child(area);
    this._blips.add(area);

    let size = style.start;
    let opacity = style.opacity;
    area.connect('repaint', () => {
      const cr = area.get_context();
      draw(cr, style, side, size, opacity);
      cr.$dispose();
    });

    const timeline = new Clutter.Timeline({ actor: area, duration: style.ms });
    timeline.connect('new-frame', () => {
      const t = timeline.get_progress();
      const eased = 1 - (1 - t) ** 3;
      size = style.start + (style.end - style.start) * eased;
      const fade = t > style.fade ? 1 - (t - style.fade) / (1 - style.fade) : 1;
      opacity = style.opacity * Math.min(1, Math.max(0, fade));
      area.queue_repaint();
    });
    timeline.connect('completed', () => {
      this._blips.delete(area);
      area.destroy();
    });
    timeline.start();
  }
}

/** The program under the cursor (0 when unknown) and its app id. */
function windowAt(x, y) {
  const actors = global.get_window_actors();
  for (let i = actors.length - 1; i >= 0; i--) {
    if (!actors[i].visible) continue;
    const window = actors[i].get_meta_window();
    if (!window || window.minimized) continue;
    const r = window.get_frame_rect();
    if (x < r.x || y < r.y || x >= r.x + r.width || y >= r.y + r.height) continue;
    const app = Shell.WindowTracker.get_default().get_window_app(window);
    const id = app ? app.get_id().replace(/\.desktop$/, '') : '';
    return [Math.max(0, window.get_pid()), id];
  }
  return [0, ''];
}

/** On GNOME's own parts: the top bar, or the overview with the dash. */
function onShell(x, y) {
  if (Main.overview.visible) return true;
  const [px, py] = Main.layoutManager.panelBox.get_transformed_position();
  const [pw, ph] = Main.layoutManager.panelBox.get_transformed_size();
  return x >= px && y >= py && x < px + pw && y < py + ph;
}

function draw(cr, style, side, size, opacity) {
  cr.setOperator(Cairo.Operator.CLEAR);
  cr.paint();
  cr.setOperator(Cairo.Operator.OVER);
  if (size < 0.5 || opacity <= 0) return;
  const c = side / 2;
  const r = size / 2;
  const [red, green, blue] = style.color;
  cr.setSourceRGBA(red / 255, green / 255, blue / 255, opacity);
  switch (style.shape) {
    case 'ring': {
      const width = Math.max(size * 0.12, 1.5);
      cr.setLineWidth(width);
      cr.arc(c, c, Math.max(r - width / 2, 0.5), 0, 2 * Math.PI);
      cr.stroke();
      return;
    }
    case 'square':
      roundedSquare(cr, c - r, c - r, size, size * 0.14);
      break;
    case 'hexagon':
      polygon(cr, c, r, 6, 0);
      break;
    case 'diamond':
      polygon(cr, c, r, 4, -90);
      break;
    default:
      cr.arc(c, c, r, 0, 2 * Math.PI);
  }
  cr.fill();
}

function polygon(cr, c, r, sides, startDeg) {
  for (let i = 0; i < sides; i++) {
    const angle = ((startDeg + (i * 360) / sides) * Math.PI) / 180;
    const [x, y] = [c + r * Math.cos(angle), c + r * Math.sin(angle)];
    if (i === 0) cr.moveTo(x, y);
    else cr.lineTo(x, y);
  }
  cr.closePath();
}

function roundedSquare(cr, left, top, size, radius) {
  const k = Math.min(radius, size / 2);
  const [right, bottom] = [left + size, top + size];
  cr.newSubPath();
  cr.arc(right - k, top + k, k, -Math.PI / 2, 0);
  cr.arc(right - k, bottom - k, k, 0, Math.PI / 2);
  cr.arc(left + k, bottom - k, k, Math.PI / 2, Math.PI);
  cr.arc(left + k, top + k, k, Math.PI, (3 * Math.PI) / 2);
  cr.closePath();
}
