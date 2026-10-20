# Context Capsule — the Linux recorder

A tray app for GNOME and KDE (Wayland first, X11 too). Press **Ctrl+Alt+Shift+C**: the
desktop's screen-sharing dialog opens. Pick a window or a screen and recording starts;
close the dialog to cancel. Press Ctrl+Alt+Shift+C again to stop. It writes exactly the
same files as the macOS app (see [`../../SPEC.md`](../../SPEC.md)), driven by the shared
engine in [`../core`](../core), so the MCP server works unchanged.

While recording, every 3 seconds it notes the front app and, **only if the front window
is part of what you picked**, its title and text — from the app's accessibility tree
(AT-SPI) when that gives enough text, else Tesseract OCR of the picked content, cropped
to the front window. Frames stay in memory (the latest one only, at most one a second)
and are never written to disk. Password fields are never read; password managers and
private messengers (KeePassXC, Bitwarden, 1Password, Signal, WhatsApp…, plus your own
`exclude_apps` list) are never recorded.

Tray icon: hollow circle idle, filled red circle recording (the running time in its title and
tooltip), a red microphone with bars that move with your voice while listening; nothing else on
screen. Soft sounds when recording or the mic starts and stops (`pw-play`, `paplay` or `aplay`;
`"sounds": false` in config.json turns them off). Menu: status line with the time, "Skipping: …"
(why the last sample wrote nothing), Start / Stop recording, Add / Finish voice note, Add another
screen… (while recording: the sharing dialog again, up to 4 monitors or windows, one portal
session each; each sample uses the one holding the focused window), Show recordings folder, Quit.
A session that ends with nothing written shows a message saying why. Each sample saves only the
lines not already saved for that window in the recording (no size limit; a 100,000-character
safety net).

Evening reminder: at 18:00 (`"remind_at"`, `false` = off), if something was recorded today and
no capsule exists for today, it asks "Make today's capsules?" once a day (zenity / kdialog, else a
notification with buttons). **Make** copies the request (`wl-copy`, `xclip` or `xsel`) and opens
`claude://claude.ai/new` with `xdg-open`; press Ctrl+V, then Enter. Recordings older than
`"keep_recordings_days"` (30; 0 = keep) are deleted; capsules never are.

## Install the dependencies

Debian / Ubuntu:

```sh
# build
sudo apt install build-essential pkg-config clang libclang-dev cmake \
  libdbus-1-dev libpipewire-0.3-dev libspa-0.2-dev libasound2-dev
# run
sudo apt install xdg-desktop-portal pipewire at-spi2-core tesseract-ocr zenity
sudo apt install xdg-desktop-portal-gnome gnome-shell-extension-appindicator   # GNOME
sudo apt install xdg-desktop-portal-kde                                        # KDE
```

Fedora:

```sh
sudo dnf install gcc-c++ pkgconf clang clang-devel cmake dbus-devel pipewire-devel alsa-lib-devel
sudo dnf install xdg-desktop-portal pipewire at-spi2-core tesseract zenity
sudo dnf install xdg-desktop-portal-gnome gnome-shell-extension-appindicator   # GNOME
sudo dnf install xdg-desktop-portal-kde                                        # KDE
```

OCR in other languages: add the Tesseract language pack (`tesseract-ocr-deu` /
`tesseract-langpack-deu`…). The language follows `voice_locale` in `config.json`, else your
locale, plus English; without the pack it reads English.

On GNOME the tray icon needs the AppIndicator extension (Ubuntu has it; on Fedora enable
it in Extensions). Without it the app still runs: use the hotkeys.

## Build and run

```sh
./build.sh              # → ../target/release/context-capsule
./build.sh --no-voice   # without voice notes (no whisper.cpp, no cmake/clang needed)
../target/release/context-capsule
```

Install the desktop entry once, so the portals and notifications show the app's name and
the hotkeys can be remembered:

```sh
install -Dm644 com.thekarya.contextcapsule.desktop ~/.local/share/applications/com.thekarya.contextcapsule.desktop
install -Dm755 ../target/release/context-capsule ~/.local/bin/context-capsule
```

`context-capsule --probe` prints what it sees right now (session, monitors, front app and
window, title, accessibility text) and exits — handy when something is skipped.

Data lives in `$XDG_DATA_HOME/context-capsule` when `XDG_DATA_HOME` is set, else
`~/.context-capsule` (or `CONTEXT_CAPSULE_HOME`), the same rule as the MCP server.

## Hotkeys

- **Ctrl+Alt+Shift+C** — start (opens the dialog) / cancel the dialog / stop.
- **Ctrl+Alt+Shift+V** — start / finish a voice note (only while recording).

Wayland: through the GlobalShortcuts portal (GNOME 48+, KDE Plasma 6); the desktop may ask
you once to confirm them, and you can change them in its keyboard settings. X11: grabbed
directly. If neither works (or another app holds the keys) the log says so; use the tray
menu.

## Wayland and X11

| | Wayland (GNOME, KDE) | X11 |
|---|---|---|
| Picker | ScreenCast portal dialog | the same portal (GNOME; KDE's portal needs Wayland) |
| Front app, title, text | AT-SPI | AT-SPI + the real window stack (`_NET_ACTIVE_WINDOW`, stacking order, pids) |
| Picked a **window** | read when the front window is the one window with the picked size | the same, against real window frames |
| Picked a **screen** | one monitor: accessibility text is read, **OCR is skipped** (apps cannot say where their windows are, so private windows on top could not be painted out); several monitors: nothing is read — pick a window instead | read and OCR'd, cropped to the front window; private windows and notification bubbles on top are painted out |
| App switches | AT-SPI `window:activate` | that, plus `_NET_ACTIVE_WINDOW` / desktop changes |
| Overview | GNOME Activities and KDE Overview are detected (read nothing) | the same |
| Hotkeys | GlobalShortcuts portal | XGrabKey |

## Voice notes

While recording press Ctrl+Alt+Shift+V, talk, press it again. The microphone opens only
between the presses; the audio stays in memory, is turned into text on this computer by
whisper.cpp (multilingual "base" model) and is never written anywhere. The first time,
Context Capsule asks before downloading the model (about 150 MB, from Hugging Face) into
`<data dir>/models/`; the download is checked against its SHA-256. Until then a voice note
says how to get it (you can also put `ggml-base.bin` there yourself). Silent or very short
clips are dropped ("Didn't catch that"). Language: your locale, or `"voice_locale"` in
`config.json`.

## Permissions

- **Screen** — nothing to grant: the portal dialog is the consent, and it asks every time
  (nothing is remembered). Stopping the share from the desktop's own indicator ends the
  recording.
- **Accessibility** — Context Capsule turns the session's accessibility bus on at launch
  (`org.a11y.Status.IsEnabled`), so GTK, Qt, Firefox, Chromium and Electron apps expose
  their text. Apps started before it may need a restart to do so. Without it, text comes
  from OCR.
- **Microphone** — only for voice notes; Linux usually asks nothing. If the input is muted
  you get "Didn't catch that"; check your Sound settings (Input).
