# Context Capsule for Windows: test checklist

Thank you for testing! This takes about 40–50 minutes. You need Windows 10 (1903 or
later) or Windows 11, a microphone for part 8, the **Claude app** for Windows (signed
in) for parts 1, 9 and 10, and a second monitor for part 5 if you have one.

Context Capsule records the **text** of the window or screen you choose (no screenshots,
no audio), so Claude can later write a short summary of your work. Recordings stay on
your PC, in `C:\Users\<you>\.context-capsule\`.

## 0. Start it

1. Unzip the kit anywhere, e.g. your Desktop. Keep `context-capsule.exe` and
   `context-capsule.mcpb` together in the same folder.
2. Open **PowerShell** in that folder (Shift + right-click in the folder → *Open PowerShell
   window here*, or *Open in Terminal*), and run:
   ```
   .\context-capsule.exe
   ```
   Leave this window open: it shows a log, which helps us. It never shows what you type.
3. If Windows says *"Windows protected your PC"*: click **More info → Run anyway**. (The
   test build is not signed yet.)
4. ✅ A small **circle icon** appears in the system tray, bottom right. It may be hidden
   under the **^** arrow.
5. ✅ The **Context Capsule** setup window opens by itself (only the first time).

## 1. Set up

The setup window has three rows. Each row has one button, and shows a ✓ instead once
that step is done.

1. **Voice notes**: if it shows **Allow**, click it. Windows Settings opens on the
   Microphone page. Turn on *Microphone access* and *Let desktop apps access your
   microphone*. ✅ Back in the setup window, the row shows a ✓ within a second or two.
2. **Connect Claude**: click **Connect**. ✅ The Claude app opens and asks to install
   Context Capsule. Click **Install** in Claude. ✅ The row shows a ✓.
   - No Claude app yet? It offers to open the download page. Install Claude, sign in,
     then click **Connect** again.
3. **Open at login**: click **Turn On**. ✅ The row shows a ✓. (To undo later:
   Settings → Apps → Startup.)
4. The **Use** list says: Record = Ctrl+Alt+Shift+C, Talk = Ctrl+Alt+Shift+V, Make my
   capsules = in Claude. Click **Done**.
5. Right-click the tray icon → **Set up Context Capsule…** ✅ The window opens again.
   Close it.

## 2. Start and cancel

1. Press **Ctrl + Alt + Shift + C**. ✅ The Windows picker opens ("choose what to share").
2. Close it without choosing. ✅ Nothing is recorded. Right-click the tray icon:
   it says *Not recording*.
3. Right-click the tray icon → **Start recording…** ✅ The picker opens again.

## 3. Record one window

1. In the picker, choose **one window**, e.g. a browser window with a long web page.
2. ✅ A yellow border appears around that window (Windows shows what is shared), the
   tray circle turns **red and filled**, you hear a soft sound, and a message says
   *● Recording started. Press Ctrl+Alt+Shift+C to stop, Ctrl+Alt+Shift+V to talk.*
   Nothing else stays on screen.
3. Hold the mouse over the tray icon. ✅ The tooltip says *● Recording 0:12 — …*, and the
   time goes up while you watch (move the mouse off and back to refresh it).
4. Use that window for 1–2 minutes: scroll down a long page, open a few pages.
5. Switch to **another app** for 20 seconds, then back.
6. Right-click the tray icon. ✅ The first line says *● Recording 2:05 — since … — N events*.
   While you were in the other app it should have said *Skipping: the front window is not
   in what you picked*.
7. Press **Ctrl + Alt + Shift + C** again to stop. ✅ A soft sound, a message says
   *■ Recording stopped: N events saved.* and the circle is hollow again.

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
   Menu → Add another screen.*
2. Right-click the tray icon → **Add another screen…** ✅ The Windows picker opens again.
   Pick the **other screen**. ✅ A message says *Screen added: 2 screens shared.* and the
   menu item now reads *Add another screen… (2 shared)*.
3. Work in a window on one screen for 20 seconds, then in a window on the other screen for
   20 seconds. Leave the mouse pointer on the *first* screen while you type on the second.
   ✅ The tray menu shows no *Skipping* line: the screen with the window you type in is the
   one that's read, wherever the mouse is.
4. Optional: **Add another screen…** again and close the picker without choosing.
   ✅ Recording goes on; nothing changes.
5. Optional: stop sharing one screen from Windows (the yellow border's sharing button, if your
   Windows shows one, or unplug that monitor). ✅ *A screen stopped sharing: 1 left.* and the
   recording goes on.
6. Stop the recording.

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
   ✅ Within a few seconds a message says *Project: Test project*.
5. Stop the recording.

## 8. Voice notes

1. Start a recording (whole screen).
2. Press **Ctrl + Alt + Shift + V**. The first time it asks to download the speech
   model (about 148 MB, one time only). Click **Yes** and wait for *Voice notes are ready*.
3. Press **Ctrl + Alt + Shift + V** (✅ a soft tick), then say one or two sentences about
   what you are doing and why.
   ✅ The tray icon is a **red microphone with three bars** that grow when you talk and
   shrink when you stop. Nothing else is on screen while it listens.
4. While it is listening, press **Ctrl + Alt + Shift + C**.
   ✅ A message says *Finish the voice note first: press Ctrl+Alt+Shift+V. Then
   Ctrl+Alt+Shift+C stops recording.* Recording goes on.
5. Press **Ctrl + Alt + Shift + V**. ✅ A soft sound, the icon is a red circle again, and a
   message says *Voice note saved*.
6. Press it again, say **nothing** for 3 seconds, press again.
   ✅ *Didn't catch that…*
7. Optional: in Settings → Privacy & security → Microphone, turn **off** *Let desktop apps
   access your microphone*, and try a voice note. ✅ The message says the microphone is
   blocked and where to turn it on (not *Didn't catch that*). Turn it back on.
8. Stop the recording, then press **Ctrl + Alt + Shift + V**.
   ✅ *Start recording first (Ctrl+Alt+Shift+C)*.

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
4. Click **Make**. ✅ The Claude app opens on a new chat, and a message says *In Claude,
   press Ctrl+V, then Enter.* Press Ctrl+V: ✅ the text *Use Context Capsule to make my
   capsules for today.* appears. Don't send it yet: part 10 does.
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

## 11. Send us the results

1. Open `C:\Users\<you>\.context-capsule\sessions\` (or: tray menu → **Show recordings folder**).
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

## Notes for the tester

- Nothing is uploaded by Context Capsule. The only internet use is the one-time speech
  model download in part 8, from huggingface.co, after you say yes. Claude only sees your
  recordings when you ask it to (part 10).
- To remove everything: quit the app, turn Context Capsule off in Settings → Apps →
  Startup, remove *Context Capsule* in Claude (Settings → Extensions), then delete the kit
  folder and `C:\Users\<you>\.context-capsule\`.
