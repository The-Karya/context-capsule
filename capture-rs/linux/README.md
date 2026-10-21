# Context Capsule — the Linux recorder

A tray app for GNOME and KDE (Wayland first, X11 too). Press **Alt+C**: the
desktop's screen-sharing dialog opens. Pick a window or a screen and recording starts;
close the dialog to cancel. Press Alt+C again to stop.

**All my screens** (Settings → Every day → When you start, `"share_screens": "all"` in config.json):
Alt+C asks the ScreenCast portal for monitors only, several at once (`SelectSources` with
`multiple=true`, `types=MONITOR`). The desktop's dialog still opens and you confirm there; every monitor
you choose is shared (up to 4, one portal session with a stream each), the same recording as adding them
one by one. How many can be chosen at once is up to the desktop's portal backend: KDE lets you choose
several; a backend that offers one at a time shares that one, and Add a screen adds the rest. "Add a
screen" is off while every monitor is shared. On Wayland, with every monitor shared, the window in use is
read through accessibility (OCR is skipped, as with one shared monitor). On X11 the same portal is used (there is no other way to
start sharing): without xdg-desktop-portal, starting fails with the same message as picking one screen
does. The default is `"pick"`. It writes exactly the
same files as the macOS app (see [`../../SPEC.md`](../../SPEC.md)), driven by the shared
engine in [`../core`](../core), so the MCP server works unchanged.

While recording, every 3 seconds it notes the front app and, **only if the front window
is part of what you picked**, its title and text — from the app's accessibility tree
(AT-SPI) when that gives enough text, else Tesseract OCR of the picked content, cropped
to the front window. Frames stay in memory (the latest one only, at most one a second)
and are never written to disk. Password fields are never read; password managers and
private messengers (KeePassXC, Bitwarden, 1Password, Signal, WhatsApp…, plus your own
`exclude_apps` list) are never recorded.

On X11 it also reads where you look: when the mouse moved or scrolled over a window on
**another** shared screen in the last 30 seconds, that window is read too, by OCR (only its new
lines; never a private app). Those events say `"focus":"pointer"`: seen, not counted as time
spent. Scrolls come from XInput 2 raw events (no permission, no grab). On Wayland no app may know
where the mouse is, so only the focused window is read.

Tray icon: hollow circle idle, filled red circle recording (the running time in its title and
tooltip), a red microphone with bars that move with your voice while listening; nothing else on
screen. Soft sounds when recording or the mic starts and stops (`pw-play`, `paplay` or `aplay`;
`"sounds": false` in config.json turns them off). Menu: Open Context Capsule (the window, below), status line with the time, "Skipping: …"
(why the last sample wrote nothing), Start / Stop recording, Add / Finish voice note, Add another
screen… (while recording: the sharing dialog again, up to 4 monitors or windows, one portal
session each; each sample uses the one holding the focused window; picking a monitor that's
already shared says "That screen is already shared. Pick a different one."), Check for updates, Quit.
A session that ends with nothing written shows a message saying why. Each sample saves only the
lines not already saved for that window in the recording (no size limit; a 100,000-character
safety net).

## The window

Tray menu → **Open Context Capsule** (or start the app again from the app menu: the running one
opens its window). The first launch opens it by itself on Settings (a `.window-seen` marker in the
data folder remembers that), and so does every launch when the desktop shows no tray icon. Closing
the window only hides it; the app keeps running in the tray. It follows the system's light or dark
style unless Settings → Appearance says otherwise (`"appearance"` in config.json), and uses the
system accent color.

- **Home** — hello and the date; Today / This week / Streak; the status card (Start
  contextualizing / Stop / Cancel, and while recording Speak a note and Add a screen, with the running
  time; when idle, *Pick the screen or window you work on. Only that is read.* or, with All my screens,
  *Shares all your screens. Private apps are never read.*); today's time per project; the three newest capsules. On the side (under it when the window is
  narrow): the shortcuts, **Working on** (Find by itself, each project, New project), and
  **Make today's capsules** (opens Claude with the request typed in).
- **Capsules** — by project; Open, and Copy file / Copy text / Save a copy… / Show in folder. A search
  box filters by title and *In short* as you type; **Ask Claude** (or Enter; off while the box is
  empty) opens Claude with *Use Context Capsule to search my capsules: …* typed in. **Open a capsule…**
  picks a `.md` file someone sent (see below).
- **Check before sharing** — Copy file, Copy text and Save a copy… first show a dialog with what in the
  capsule might be private, by group (People, Emails, Phone numbers, Links, Money, Files), each with a
  check box, all checked. **Share** uses a copy with the checked ones replaced by `[removed]` (Copy
  file: a temporary file in `$TMPDIR/context-capsule-shared/`); the saved capsule never changes.
  Nothing found: it shares at once. Plain rules, no AI, the same as the Mac's (core `share_check`,
  cases in `spec/share-check.json`).
- **Projects** — each project and what finds it; Edit (name, clues) and Delete (asked first).
  Saved to projects.json; the recorder reads it again by itself.
- **Settings** — your name; Set up (Voice notes: the speech model, downloaded only after you say
  yes; Connect Claude; Open at login), each with a check mark when done; Every day (When you start:
  *Pick a screen or window* or *All my screens*, `share_screens`; reminder and its time, deleting old
  recordings, sounds); the voice notes' language; Never record (apps, picked from the
  installed ones; stored as their desktop id, e.g. `org.gnome.Calculator`); Appearance (System, Light,
  Dark); Updates (Update automatically, the version, Check for updates). Every change is saved at once.
- **Help** — shortcuts, how it works, privacy.

Raw recordings are never shown: only time per project, counts and capsules.

**Connect Claude**: Claude has no Linux app yet. If `context-capsule.mcpb` sits next to the program
and a Claude app is the default for it, it opens there; otherwise the window shows the one line to
run for Claude Code (`claude mcp add --scope user context-capsule -- node …/mcp/dist/index.js`, with
the path filled in when `mcp/dist/index.js` is found next to or above the program) and a Copy button.
It counts as connected when `~/.claude.json` or `~/.config/Claude/claude_desktop_config.json` lists a
`context-capsule` MCP server, or `~/.config/Claude/Claude Extensions` has it.

GTK runs on the main thread; the tray and the engine (each on its own thread) reach the window
through a small link that queues on GTK's main loop. Without a display, the app runs as before,
tray and hotkeys only. Build without the window: `cargo build --no-default-features --features voice`
(or `--no-default-features` for neither).

To try the window alone (no tray, no engine, no screen sharing; buttons that would start recording do
nothing): `context-capsule --window-only` (Home) or `--window-only=settings` (`home`, `capsules`,
`projects`, `settings`, `help`). It uses the normal data folder, or `CONTEXT_CAPSULE_HOME`.
`CONTEXT_CAPSULE_WINDOW_SIZE=1100x760` opens the window at that size (for pictures of it, e.g. under
Xvfb, where there is no window manager to resize it).

**Opening a capsule someone sent**: the app's desktop entry has `MimeType=text/markdown;` and
`Exec=… %f`, so it is offered under *Open With* for `.md` files (never made the default app). Opening
one (or `context-capsule --open <file>`, or **Open a capsule…**) checks it looks like a capsule (front
matter with `capsule:` and `title:`), then opens Claude on a new chat with *Use Context Capsule to
continue from the capsule at <full path>.* typed in (also on the clipboard). Not a capsule: *Not a
capsule*, nothing opened. If Context Capsule already runs, GApplication hands it the file (its
`open` signal) and the new copy quits.

Evening reminder: at 18:00 (`"remind_at"`, `false` = off), if something was recorded today and
no capsule exists for today, it asks "Make today's capsules?" once a day (zenity / kdialog, else a
notification with buttons). **Make** copies the request (`wl-copy`, `xclip` or `xsel`) and opens
Claude on a new chat with the request typed in (`xdg-open`); press Enter. Recordings older than
`"keep_recordings_days"` (30; 0 = keep) are deleted; capsules never are.

## The AppImage

The release is one file, `Context-Capsule-<version>-x86_64.AppImage`. Make it runnable
(Properties → Allow executing, or `chmod +x`) and open it. The first time it asks *Add Context
Capsule to your apps?* — **Add** writes `~/.local/share/applications/com.thekarya.contextcapsule.desktop`
(pointing at the AppImage, offered for `.md` files under Open With) and its icon; an entry written by an
older version is brought up to date at the next start. Open at login points at the AppImage too. It uses the
system's PipeWire, D-Bus, ALSA and Tesseract (the "run" packages below). Built on Debian 12, so it
needs glibc 2.36 or newer (Ubuntu 23.04+, Fedora 37+, Debian 12+).

Build it with `./appimage.sh`, for this machine's arch (on a Mac it runs itself in the Docker
image from `../ci/linux.Dockerfile`, natively: an arm Mac makes an aarch64 AppImage). The x86_64
release file comes from the `appimage` job of the manual CI workflow (an artifact). The file lands in
`../target-appimage/`, and the script prints its size and SHA-256 (for `latest.json`).

## Updates

A minute after launch and then once a day (never while contextualizing or during a voice note), it
fetches `latest.json` and its Ed25519 signature from the release feed (`"update_feed"` in
config.json overrides it; `"auto_update": false` turns automatic checks off; **Check for updates** in
the tray menu or in Settings → Updates checks now). Signature first, then the version (only newer).

- **As an AppImage** (`$APPIMAGE` set and its folder writable): the new AppImage is downloaded next to
  it (`.<name>.new`) and checked (size and SHA-256), then *Context Capsule 0.2.0 is ready. Restart to
  update.* **Restart now** makes it runnable, renames it over `$APPIMAGE`, quits cleanly and starts it.
- **Built from source**: it only says *A new version is out: Context Capsule 0.2.0.*

Any failure keeps the current version and is only logged (shown only after Check for updates). No
network = no update, silently. Nothing about you is sent.

## Install the dependencies

Debian / Ubuntu:

```sh
# build
sudo apt install build-essential pkg-config clang libclang-dev cmake \
  libdbus-1-dev libpipewire-0.3-dev libspa-0.2-dev libasound2-dev \
  libgtk-4-dev libadwaita-1-dev      # the window: GTK 4.8+, libadwaita 1.2+ (Debian 12, Ubuntu 22.10+)
# run
sudo apt install xdg-desktop-portal pipewire at-spi2-core tesseract-ocr zenity libgtk-4-1 libadwaita-1-0
sudo apt install xdg-desktop-portal-gnome gnome-shell-extension-appindicator   # GNOME
sudo apt install xdg-desktop-portal-kde                                        # KDE
```

Fedora:

```sh
sudo dnf install gcc-c++ pkgconf clang clang-devel cmake dbus-devel pipewire-devel alsa-lib-devel \
  gtk4-devel libadwaita-devel
sudo dnf install xdg-desktop-portal pipewire at-spi2-core tesseract zenity gtk4 libadwaita
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

- **Alt+C** — start (opens the dialog) / cancel the dialog / stop.
- **Alt+V** — start / finish a voice note (only while recording).

The same keys as the Mac's ⌥C / ⌥V.

Wayland: through the GlobalShortcuts portal (GNOME 48+, KDE Plasma 6); the desktop may ask
you once to confirm them, and you can change them in its keyboard settings. X11: grabbed
directly. If neither works the log says so; if another app holds a key, a message says so
once. Either way the tray menu still works.

## Wayland and X11

| | Wayland (GNOME, KDE) | X11 |
|---|---|---|
| Picker | ScreenCast portal dialog | the same portal (GNOME; KDE's portal needs Wayland) |
| All my screens | the portal dialog, monitors only, several at once; you confirm there | the same (no portal: it can't start, as with picking) |
| Front app, title, text | AT-SPI | AT-SPI + the real window stack (`_NET_ACTIVE_WINDOW`, stacking order, pids) |
| Picked a **window** | read when the front window is the one window with the picked size | the same, against real window frames |
| Picked a **screen** | one monitor: accessibility text is read, **OCR is skipped** (apps cannot say where their windows are, so private windows on top could not be painted out); several monitors: nothing is read — pick a window instead | read and OCR'd, cropped to the front window; private windows and notification bubbles on top are painted out |
| App switches | AT-SPI `window:activate` | that, plus `_NET_ACTIVE_WINDOW` / desktop changes |
| Overview | GNOME Activities and KDE Overview are detected (read nothing) | the same |
| Hotkeys | GlobalShortcuts portal | XGrabKey |
| Where you look (the mouse) | not read (no app may know where the mouse is) | the window under the mouse on another shared screen, by OCR |

## Voice notes

While recording press Alt+V, talk, press it again. The microphone opens only
between the presses; the audio stays in memory, is turned into text on this computer by
whisper.cpp (multilingual "base" model) and is never written anywhere. The first time,
Context Capsule asks before downloading the model (about 150 MB, from Hugging Face) into
`<data dir>/models/`; the download is checked against its SHA-256. Until then a voice note
says how to get it (you can also put `ggml-base.bin` there yourself). Silent or very short
clips are dropped ("Didn't catch that"). Language: your locale, or `"voice_locale"` in
`config.json`.

## Permissions

- **Screen** — nothing to grant: the portal dialog is the consent, and it asks every time
  (nothing is remembered), with All my screens too. Stopping the share from the desktop's own indicator ends the
  recording.
- **Accessibility** — Context Capsule turns the session's accessibility bus on at launch
  (`org.a11y.Status.IsEnabled`), so GTK, Qt, Firefox, Chromium and Electron apps expose
  their text. Apps started before it may need a restart to do so. Without it, text comes
  from OCR.
- **Microphone** — only for voice notes; Linux usually asks nothing. If the input is muted
  you get "Didn't catch that"; check your Sound settings (Input).
