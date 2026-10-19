# Context Capsule for Windows: test checklist

Thank you for testing! This takes about 20–30 minutes. You need Windows 10 (1903 or
later) or Windows 11, and a microphone for part 5.

Context Capsule records the **text** of the window or screen you choose (no screenshots,
no audio), so an AI can later write a short summary of your work. Everything stays on
your PC, in `C:\Users\<you>\.context-capsule\`.

## 0. Start it

1. Unzip the kit anywhere, e.g. your Desktop.
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

## 1. Start and cancel

1. Press **Ctrl + Alt + Shift + C**. ✅ The Windows picker opens ("choose what to share").
2. Close it without choosing. ✅ Nothing is recorded. Right-click the tray icon:
   it says *Not recording*.
3. Right-click the tray icon → **Start recording…** ✅ The picker opens again.

## 2. Record one window

1. In the picker, choose **one window**, e.g. a browser window with a long web page.
2. ✅ A yellow border appears around that window (Windows shows what is shared), and
   the tray circle is filled.
3. Use that window for 1–2 minutes: scroll, open a few pages.
4. Switch to **another app** for 20 seconds, then back.
5. Right-click the tray icon. ✅ It says *Recording since … — N events*. While you were
   in the other app it should have said *Skipping: the front window is not in what you picked*.
6. Press **Ctrl + Alt + Shift + C** again to stop. ✅ The circle is hollow again.

## 3. Record a whole screen

1. Start again and choose a **whole screen** (display).
2. Use 2–3 different apps for 2 minutes: e.g. a browser, Notepad (type a few lines),
   File Explorer, Settings or Calculator.
3. Open a **login page** in the browser and type something into the **password box**
   (a fake password, e.g. `TestPass123`). Don't press Enter.
4. Press **Win + Tab** (Task View) for a few seconds, then close it.
5. If you have a second monitor: move a window to the other screen for 20 seconds.
6. Stop the recording.

## 4. Private apps are never recorded

1. If you have **WhatsApp**, **Signal**, **1Password**, **Bitwarden** or **KeePass**: start a
   recording of the whole screen, and open that app for 20 seconds.
2. ✅ The tray menu says *Skipping: a private app is in front (not recorded)*.
3. Stop the recording.

## 5. Voice notes

1. Start a recording (whole screen).
2. Press **Ctrl + Alt + Shift + V**. The first time it asks to download the speech
   model (about 148 MB, one time only). Click **Yes** and wait for *Voice notes are ready*.
3. Press **Ctrl + Alt + Shift + V**, then say one or two sentences about what you are
   doing and why, then press **Ctrl + Alt + Shift + V** again.
   ✅ The tray icon is a microphone while it listens, and goes back after.
   ✅ A message says *Voice note saved*.
4. Press it again, say **nothing** for 3 seconds, press again.
   ✅ *Didn't catch that…*
5. Stop the recording, then press **Ctrl + Alt + Shift + V**.
   ✅ *Start recording first (Ctrl+Alt+Shift+C)*.

## 6. Send us the results

1. Open `C:\Users\<you>\.context-capsule\sessions\` (or: tray menu → **Open sessions folder**).
2. **Look inside first**: each session folder has an `events.jsonl` file with the text
   that was read from your screen. Delete any session you don't want to share.
3. Check: does any `events.jsonl` contain `TestPass123` or text from a private app?
   It **must not**.
4. Zip the `sessions` folder and send it, together with:
   - the text from the PowerShell window (select all, copy, paste into a file),
   - a screenshot of anything that looked wrong,
   - your Windows version (Settings → System → About), and your screen scaling
     (Settings → System → Display → Scale, e.g. 125%).
5. Quit: tray icon → **Quit Context Capsule**.

## Notes for the tester

- Nothing is uploaded. The only internet use is the one-time speech model download in
  part 5, from huggingface.co, after you say yes.
- To remove everything: quit the app, then delete the kit folder and
  `C:\Users\<you>\.context-capsule\`.
