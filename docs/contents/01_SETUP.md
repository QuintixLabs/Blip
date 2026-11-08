# Setup

Blip is made with [Tauri](https://tauri.app). The window is HTML, CSS and JavaScript in `src/`. The rest (watching clicks, drawing the blips, the tray) is Rust in `src-tauri/`.

## What you need

- [Node.js](https://nodejs.org) 20 or newer
- [Rust](https://rustup.rs)
- The [Tauri prerequisites](https://tauri.app/start/prerequisites/) for your system

## Get the code

Fork Blip on GitHub, then:

```sh
git clone https://github.com/QuintixLabs/Blip.git
cd Blip
git switch -c my-change
npm install
```

## Run it

```sh
npm run dev
```

The first run takes a few minutes. After that it's quick.

Blip stays in the tray when you close its window. Exit it from the tray before running `npm run dev` again.

## Make an installer

```sh
npm run build
```

It ends up in `src-tauri/target/release/bundle/`. At the very end you'll see a signing error. That's normal: only maintainers have the update key, and the installer still works.

## Your settings

Everything is saved in `blip.json`. **Settings → Open settings folder** takes you there.

- **Windows:** `%APPDATA%\Blip\`
- **macOS:** `~/Library/Application Support/Blip/`
- **Linux:** `~/.config/Blip/`

Exit Blip before you edit it, or it will save over your changes.

To see the welcome screen again, set `"showWelcome": true`. Deleting the file resets everything.

The first time the new Blip starts, it brings over the settings of the old AutoHotkey Blip (`%LOCALAPPDATA%\Blip\settings.ini`), if there are any.

## If something goes wrong

Rust errors show in the terminal running `npm run dev`. For the window, press `Ctrl+Shift+I` in a dev build to open the developer tools. Put what you find in your issue.

---

#### [02. Committing ➔](/docs/contents/02_COMMITING.md)
