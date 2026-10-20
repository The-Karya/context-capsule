# Context Capsule for Windows: test checklist

Thank you for testing! This takes about 45–55 minutes. You need Windows 10 (1903 or
later) or Windows 11, a microphone for part 8, the **Claude app** for Windows (signed
in) for parts 1, 9 and 10, and a second monitor for part 5 if you have one.

Context Capsule records the **text** of the window or screen you choose (no screenshots,
no audio), so Claude can later write a short summary of your work. Recordings stay on
your PC, in `C:\Users\<you>\.context-capsule\`.

## 0. Start it

1. Put the file you got (`Context-Capsule-0.1.0-windows.exe`, or `context-capsule.exe` from
   the kit) in a folder, e.g. your Desktop.
2. Open **PowerShell** in that folder (Shift + right-click in the folder → *Open PowerShell
   window here*, or *Open in Terminal*), and run it, e.g.:
   ```
   .\Context-Capsule-0.1.0-windows.exe
   ```
   Leave this window open: it shows a log, which helps us. It never shows what you type.
   It asks *Install Context Capsule?* Click **Just run it** for now (part 12 tries Install).
3. If Windows says *"Windows protected your PC"*: click **More info → Run anyway**. (The
   test build is not signed yet.)
4. ✅ A small **circle icon** appears in the system tray, bottom right. It may be hidden
   under the **^** arrow.
5. ✅ The **Context Capsule** window opens by itself on **Settings** (only the first time).

## 1. Set up

In the window's **Settings**, the **Set up** card has three rows. Each row has one button,
and shows a ✓ instead once that step is done.

1. **Voice notes**: if it shows **Allow**, click it. Windows Settings opens on the
   Microphone page. Turn on *Microphone access* and *Let desktop apps access your
   microphone*. ✅ Back in Context Capsule, the row shows a ✓ within a second or two.
2. **Connect Claude**: click **Connect**. ✅ The Claude app opens and asks to install
   Context Capsule. Click **Install** in Claude. ✅ The row shows a ✓.
   - No Claude app yet? It offers to open the download page. Install Claude, sign in,
     then click **Connect** again.
3. **Open at login**: click **Turn On**. ✅ The row shows a ✓. (To undo later:
   Settings → Apps → Startup.)
4. **Your name**: type your first name. ✅ Go to **Home**: it says *Good morning,
   <your name>.* (or afternoon / evening).
5. Under **Every day**, **Appearance**, **Never record** and **Updates**, try each control once
   (**Updates** shows the version; **Check for updates** says what it found):
   ✅ every change sticks after you close and reopen the window. **Appearance → Dark**
   turns the window dark at once; set it back to **System**.
6. Close the window (✕). Right-click the tray icon → **Open Context Capsule**
   ✅ The window opens again, on Home. Make it very narrow, then very wide:
   ✅ nothing overlaps or gets cut off; when wide, **Shortcuts**, **Working on** and
   **Make today's capsules** sit on the right.
7. Press **Tab** a few times ✅ a ring moves from button to button; **Enter** presses it.
8. **Help** shows the shortcuts (Alt+C, Alt+V), how it works and privacy.

## 2. Start and cancel

1. Press **Alt + C**. ✅ The Windows picker opens ("choose what to share").
   (If another app already uses Alt+C or Alt+V, a message says so when Context Capsule
   starts; use the tray menu instead, and tell us which app.)
2. Close it without choosing. ✅ Nothing is recorded. Right-click the tray icon:
   it says *Not recording*.
3. Right-click the tray icon → **Start recording…** ✅ The picker opens again.

## 3. Record one window

1. In the picker, choose **one window**, e.g. a browser window with a long web page.
2. ✅ A yellow border appears around that window (Windows shows what is shared), the
   tray circle turns **red and filled**, and you hear a soft sound. ✅ No text appears on
   screen.
3. Hold the mouse over the tray icon. ✅ The tooltip says *● Recording 0:12 — …*, and the
   time goes up while you watch (move the mouse off and back to refresh it).
4. Use that window for 1–2 minutes: scroll down a long page, open a few pages.
5. Switch to **another app** for 20 seconds, then back.
6. Right-click the tray icon. ✅ The first line says *● Recording 2:05 — since … — N events*.
   While you were in the other app it should have said *Skipping: the front window is not
   in what you picked*.
7. Press **Alt + C** again to stop. ✅ A soft sound, no text on screen, and the circle is
   hollow again.

## 4. Record a whole screen

1. Start again and choose a **whole screen** (display).
2. Use 2–3 different apps for 2 minutes: e.g. a browser, Notepad (type a few lines),
   File Explorer, Settings or Calculator.
3. Open a **login page** in the browser and type something into the **password box**
   (a fake password, e.g. `TestPass123`). Don't press Enter.
4. Click the **taskbar**, open the **Start menu**, and click the **clock** (notifications)
   for a few seconds each, then close them.
5. Press **Win + Tab** (Task View) for a few seconds, then close it.
6. If you have a second monitor: move a window to the other screen for 20 seconds.
   ✅ The tray menu says *Skipping: the front window is not in what you picked* (only the
   screen you picked is read).
7. Stop the recording.

## 5. Several screens (needs a second monitor)

1. Start a recording and pick your **main screen**. ✅ The message ends with *More screens?
   Menu → Add a screen.*
2. Right-click the tray icon → **Add a screen** ✅ The Windows picker opens again.
   Pick the **other screen**. ✅ No text on screen; the menu item now reads *Add a screen
   (2 shared)*.
3. Work in a window on one screen for 20 seconds, then in a window on the other screen for
   20 seconds. Leave the mouse pointer on the *first* screen while you type on the second.
   ✅ The tray menu shows no *Skipping* line: the screen with the window you type in is the
   one that's read, wherever the mouse is.
4. Keep typing in a window on one screen, and **scroll a long page on the other screen with
   the mouse** (without clicking it) for 20 seconds. ✅ That page is read too (part 11: its
   lines are in `events.jsonl` with `"focus":"pointer"`). Leave the mouse still for a minute:
   ✅ it is not read any more.
5. **Add a screen** again and pick a screen that is **already shared**. ✅ A message says
   *That screen is already shared. Pick a different one.* and the menu still says
   *Add a screen (2 shared)*.
6. Optional: **Add a screen** again and close the picker without choosing.
   ✅ Recording goes on; nothing changes.
7. Optional: stop sharing one screen from Windows (the yellow border's sharing button, if your
   Windows shows one, or unplug that monitor). ✅ *A screen stopped sharing: 1 left.* and the
   recording goes on.
8. Stop the recording.

## 6. Private apps are never recorded

1. If you have **WhatsApp**, **Signal**, **1Password**, **Bitwarden** or **KeePass**: start a
   recording of the whole screen, and open that app for 20 seconds.
2. ✅ The tray menu says *Skipping: a private app is in front (not recorded)*.
3. Stop the recording.

## 7. Projects

Context Capsule sorts your work into projects by itself. You can also name one.

1. **Without recording**, open a window of something you work on (a web page or a
   document). Right-click the tray icon → **Project** → **New project from this
   window…** ✅ A box asks for a name. It may already be filled in (e.g. the site or file
   name). Type a name such as `Test project` and click **Create**.
2. Right-click the tray icon. ✅ It shows *Project: Test project*, and in the **Project**
   menu *Test project* has a check mark.
3. **Project** → **Auto-detect** to go back to automatic.
4. Start a recording of the whole screen, and go back to that same window.
   ✅ Within a few seconds the tray menu shows *Project: Test project* (no text on screen).
5. Stop the recording.
6. Open the window → **Home**. ✅ *Working on* lists *Find by itself* and *Test project*;
   clicking one puts the check mark on it (the tray's **Project** menu agrees).
   ✅ **Today** shows a bar for *Test project* with its time.
7. **Projects** → *Test project* → **Edit**: change the name, add a word after a comma in
   *Found by*, **Save** ✅ the row shows the new name and words. **Delete** asks first;
   click **Cancel**.
8. Tell us if you can find the raw recording (the saved text) anywhere in the window:
   you should not be able to — only times, projects and capsules are shown.

## 8. Voice notes

1. Start a recording (whole screen).
2. Press **Alt + V**. The first time it asks to download the speech
   model (about 148 MB, one time only). Click **Yes** and wait for *Voice notes are ready*.
3. Press **Alt + V** (✅ a soft tick), then say one or two sentences about
   what you are doing and why.
   ✅ The tray icon is a **red microphone with three bars** that grow when you talk and
   shrink when you stop. Nothing else is on screen while it listens.
4. While it is listening, press **Alt + C**.
   ✅ A message says *Finish the voice note first: press Alt+V. Then
   Alt+C stops recording.* Recording goes on.
5. Press **Alt + V**. ✅ A soft sound, the icon is a red circle again, and no text on
   screen.
6. Press it again, say **nothing** for 3 seconds, press again.
   ✅ *Didn't catch that…*
7. Optional: in Settings → Privacy & security → Microphone, turn **off** *Let desktop apps
   access your microphone*, and try a voice note. ✅ The message says the microphone is
   blocked and where to turn it on (not *Didn't catch that*). Turn it back on.
8. Stop the recording, then press **Alt + V**.
   ✅ *Start recording first (Alt+C)*.

## 9. The evening reminder

At 18:00, if you recorded something today and have no capsule for today yet, Context Capsule
asks once: *Make today's capsules?* To try it now instead of waiting:

1. Quit Context Capsule. Open `C:\Users\<you>\.context-capsule\` and delete the file
   `.reminded` if it is there. Open (or create) `config.json` in Notepad and put in
   `{"remind_at": "00:00"}` (keep any other lines you have).
2. Make sure you recorded something today (part 3), and that Claude made **no** capsule
   today yet (that comes in part 10).
3. Start `.\context-capsule.exe` again. ✅ Within a minute a small box asks *Make today's
   capsules?* with **Make** and **Not now**.
4. Click **Make**. ✅ The Claude app opens on a new chat with *Use Context Capsule to make
   my capsules for today.* already typed in (Claude may show a small caution note; that's
   expected). Don't send it yet: part 10 does.
5. Quit and start the app again. ✅ It does **not** ask again today.
6. Put `config.json` back as it was (remove the `remind_at` line).

Other settings you may try in `config.json`: `"sounds": false` (no start/stop sounds),
`"remind_at": false` (no reminder), `"keep_recordings_days": 7` (raw recordings are deleted
after 7 days instead of 30; capsules are never deleted).

## 10. Make your capsules in Claude

1. Use the Claude chat from part 9 (or open the Claude app and start a new chat).
2. Paste (Ctrl+V) or type: **Use Context Capsule to make my capsules for today**, then Enter.
3. ✅ Claude reads today's recordings and writes your capsules. If Claude asks to allow
   Context Capsule's tools, click **Allow**.
4. Tell us: did it work, and did the capsules match what you did?
5. Back in Context Capsule (tray → **Open Context Capsule**): ✅ **Home → Recent capsules**
   lists them; **Capsules** groups them by project (*Whole project* first). On one capsule,
   click **⋯** → **Copy file** and paste into a chat or a folder ✅ the file arrives;
   **Copy text**, **Save a copy…**, **Show in folder** and **Open** work too.

## 11. Send us the results

1. Open `C:\Users\<you>\.context-capsule\sessions\`.
2. **Look inside first**: each session folder has an `events.jsonl` file with the text
   that was read from your screen. Delete any session you don't want to share.
3. Check: does any `events.jsonl` contain `TestPass123` or text from a private app?
   It **must not**. From part 3, the long page you scrolled should be there in full, each
   line once (later moments of the same window only add the lines that were new).
4. Zip the `sessions` folder and send it, together with:
   - `app.log`, one folder up in `C:\Users\<you>\.context-capsule\` (it only has times,
     counts, reasons and error codes, never what was on screen or what you said),
   - the text from the PowerShell window (select all, copy, paste into a file),
   - screenshots of anything that looked wrong (Win + Shift + S),
   - your Windows version (Settings → System → About), and your screen scaling
     (Settings → System → Display → Scale, e.g. 125%).
5. Quit: tray icon → **Quit Context Capsule**.

## 12. Install, update, uninstall

1. Quit Context Capsule. Double-click `Context-Capsule-0.1.0-windows.exe` in its folder.
   ✅ A box asks *Install Context Capsule?* Click **Install**.
2. ✅ The tray icon comes back. In the Start menu, ✅ *Context Capsule* is there. In
   `%LOCALAPPDATA%\Programs\Context Capsule\` ✅ you see `context-capsule.exe` and
   `context-capsule.mcpb`. No admin question came up.
3. Settings → Apps → Installed apps: ✅ *Context Capsule*, publisher *Karya*, version 0.1.0.
4. If open at login was on (part 1): Settings → Apps → Startup ✅ still lists Context Capsule
   (it now starts the installed copy).
5. Right-click the tray icon → **Check for updates**. ✅ A message says *You have the latest
   version.* or *Couldn't check for updates. Try again later.* (until we publish a release,
   the second one is expected). An update itself can only be tested once a newer release is
   out: then a box says *Context Capsule 0.2.0 is ready. Restart to update.*, **Restart now**
   starts the new version within a few seconds, and Settings → Apps shows the new version.
6. Settings → Apps → Context Capsule → **Uninstall**. ✅ The tray icon goes away, and a box asks
   *Also delete your recordings and capsules?* Click **No**. ✅ *Context Capsule is removed.*
7. ✅ The Start menu entry, the Settings → Apps entry and the folder
   `%LOCALAPPDATA%\Programs\Context Capsule` are gone, and
   `C:\Users\<you>\.context-capsule\` is still there.

## Notes for the tester

- Nothing is uploaded by Context Capsule. The internet is used for the one-time speech
  model download in part 8, from huggingface.co, after you say yes, and to check for a new
  version on github.com (nothing about you is sent). Claude only sees your
  recordings when you ask it to (part 10).
- To remove everything: uninstall it (part 12) and say **Yes** to deleting your recordings,
  remove *Context Capsule* in Claude (Settings → Extensions), and delete the downloaded file.
