# Context Capsule for Windows: test checklist

Thank you for testing! This takes about 60–75 minutes. You need Windows 10 (1903 or
later) or Windows 11, a microphone for part 8, the **Claude app** for Windows (signed
in) for parts 1, 9 and 10, any other AI apps you use (Cursor, VS Code, Windsurf, Codex,
LM Studio, Gemini CLI) for part 1, a ChatGPT account (the website, chatgpt.com) for part 11,
and a second monitor for part 5 if you have one.

Context Capsule records the **text** of the window or screen you choose (no screenshots,
no audio), so your AI can later write a short summary of your work. Recordings stay on
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
   It asks *Install Context Capsule?* Click **Just run it** for now (part 13 tries Install).
3. If Windows says *"Windows protected your PC"*: click **More info → Run anyway**. (The
   test build is not signed yet.)
4. ✅ A small **circle icon** appears in the system tray, bottom right. It may be hidden
   under the **^** arrow.
5. ✅ The **Context Capsule** window opens by itself on **Settings** (only the first time).

## 1. Set up

In the window's **Settings**, the **Set up** card has three parts: Voice notes, Connect your AI
(with your AI apps under it) and Open at login. Each shows a ✓ once that step is done.

1. **Voice notes**: if it shows **Allow**, click it. Windows Settings opens on the
   Microphone page. Turn on *Microphone access* and *Let desktop apps access your
   microphone*. ✅ Back in Context Capsule, the row shows a ✓ within a second or two.
2. **Connect your AI**: under it there is one row per AI app on this PC (Claude first), each
   with **Connect** or a ✓. The ones not on this PC are under **More**: click it ✅ they show,
   dimmed, with *Not installed*. Under the list: *Any other AI (ChatGPT website, Gemini,
   Copilot…): use Copy my day.*
   - **Claude**: click **Connect**. ✅ A note says *Click Install in Claude. The ✓ shows when it's
     done.*, and one of these happens:
     - The Claude app opens and asks to install Context Capsule. Click **Install** in Claude.
     - Or Claude opens, a folder opens with `context-capsule.mcpb` selected, and a box says
       *In Claude: Settings → Extensions → Advanced settings → Install Extension…, then pick
       context-capsule.mcpb*. Do that in Claude.
     - Or a box asks *Is the Claude app installed?* (it couldn't find it). **Yes** shows the
       folder and the steps above; **No** opens the download page.
     ✅ Once installed in Claude, Claude's row and the **Connect your AI** row show a ✓. With
     Claude installed it must **not** say Claude is missing. Either way, please send us the line
     starting with *claude app:* from `app.log` (part 12): it says how Context Capsule found Claude.
   - **Each other app you have** — Cursor, VS Code, Windsurf, Codex / ChatGPT (the desktop app),
     LM Studio, Gemini CLI: click **Connect**. ✅ It says *Connecting…*, then shows a ✓ and the note
     *Connected to Cursor. Restart Cursor to use it.* (or the app's own words). ✅ The app's
     settings file now has a `"context-capsule"` entry whose command is
     `…\Context Capsule\context-capsule-mcp.exe` (or, with **Just run it**, `context-capsule-mcp.exe`
     in the folder you ran it from) with args `["serve"]`; your other entries are still there:
     - Cursor: `C:\Users\<you>\.cursor\mcp.json`
     - VS Code: `%APPDATA%\Code\User\mcp.json` (under `servers`, with `"type": "stdio"`)
     - Windsurf: `C:\Users\<you>\.codeium\windsurf\mcp_config.json`
     - Codex / ChatGPT: `C:\Users\<you>\.codex\config.toml` (`[mcp_servers.context-capsule]`)
     - LM Studio: `C:\Users\<you>\.lmstudio\mcp.json`
     - Gemini CLI: `C:\Users\<you>\.gemini\settings.json`
     Restart the app and ask it: *Use Context Capsule to tell me if I recorded anything today.*
     ✅ It uses Context Capsule (it may ask you to allow its tools).
   - The first app you connect becomes **Your AI** (Settings → **Your AI**), unless you chose one.
   - On a connected app (not Claude) click **⋯** → **Disconnect**. ✅ The ✓ goes, the note says
     *Disconnected from Cursor.*, and the entry is gone from its settings file. Connect it again.
     On Claude, **⋯** → **Disconnect** ✅ *Remove it in Claude* (Claude removes its own extensions).
3. **Open at login**: click **Turn On**. ✅ The row shows a ✓. (To undo later:
   Settings → Apps → Startup.)
4. **Your name**: type your first name. ✅ Go to **Home**: it says *Good morning,
   <your name>.* (or afternoon / evening).
5. Under **Every day**, **Appearance**, **Never record** and **Updates**, try each control once
   (**Updates** shows the version; **Check for updates** says what it found):
   ✅ every change sticks after you close and reopen the window. **Appearance → Dark**
   turns the window dark at once; set it back to **System**.
6. **Light and dark**: set **Appearance → Light**. ✅ The whole window is light: a light grey
   background (behind the sidebar and the page too, never black), white cards, dark text that
   is easy to read in the sidebar (Home, Capsules, Projects, Settings, Help) and in the page
   title. Set **Appearance → System**, then switch Windows itself between light and dark
   (Settings → Personalization → Colors → *Choose your mode*) with the window open
   ✅ the window follows at once, everywhere (sidebar, page, text boxes, title bar).
   Please send a screenshot of the window in light mode.
7. **Shortcuts** (Settings → **Shortcuts**): click the button next to **Start or stop** (it
   says *Alt+C*). ✅ It says *Press keys…*. Press **Ctrl + Shift + R**. ✅ The button now says
   *Ctrl+Shift+R*; Home's **Shortcuts** card, **Help** and the tray menu say so too. Press
   **Ctrl + Shift + R** anywhere ✅ the picker opens (close it). **Alt + C** does nothing now.
   - Click **Speak a note**, press **Ctrl + Shift + R** ✅ refused: *Already used for Start or
     stop*, and it keeps *Alt+V*.
   - Click it again and press just **V** ✅ refused: *Add Ctrl or Alt*.
   - Click it, then press **Esc** ✅ nothing changes.
   - Click it and press keys another app uses (e.g. **Win** shortcuts don't count; try a key
     your other apps use, if you know one) ✅ *Another app uses these keys*, the old one stays.
   - Click **Reset** ✅ back to *Alt+C* and *Alt+V*, and they work again.
8. Close the window (✕). Right-click the tray icon → **Open Context Capsule**
   ✅ The window opens again, on Home. Make it very narrow, then very wide:
   ✅ nothing overlaps or gets cut off; when wide, **Shortcuts**, **Working on** and
   **Make today's capsules** sit on the right.
9. Press **Tab** a few times ✅ a ring moves from button to button; **Enter** presses it.
10. **Help** shows the shortcuts (Alt+C, Alt+V, and *Make my capsules for today*: *in your AI*,
    or *Copy my day* when Your AI is a website or any other AI), how it works and privacy.

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

1. Start a recording and pick your **main screen**. ✅ No text on screen.
2. Right-click the tray icon → **Add a screen** ✅ The Windows picker opens again.
   Pick the **other screen**. ✅ No text on screen; the menu item now reads *Add a screen
   (2 shared)*.
3. Work in a window on one screen for 20 seconds, then in a window on the other screen for
   20 seconds. Leave the mouse pointer on the *first* screen while you type on the second.
   ✅ The tray menu shows no *Skipping* line: the screen with the window you type in is the
   one that's read, wherever the mouse is.
4. Keep typing in a window on one screen, and **scroll a long page on the other screen with
   the mouse** (without clicking it) for 20 seconds. ✅ That page is read too (part 12: its
   lines are in `events.jsonl` with `"focus":"pointer"`). Leave the mouse still for a minute:
   ✅ it is not read any more.
5. **Add a screen** again and pick a screen that is **already shared**. ✅ A message says
   *That screen is already shared. Pick a different one.* and the menu still says
   *Add a screen (2 shared)*.
6. Optional: **Add a screen** again and close the picker without choosing.
   ✅ Recording goes on; nothing changes.
7. Optional: stop sharing one screen from Windows (the yellow border's sharing button, if your
   Windows shows one, or unplug that monitor). ✅ No text on screen, the recording goes on, and
   the menu item reads *Add a screen* again.
8. Stop the recording.

**All my screens** (all screens together, no picker):

9. Open the window → **Settings** → **Every day** → **When you start** → choose **All my
   screens**. Go to **Home**. ✅ The status card says *Alt+C to start. Shares all your screens.
   Private apps are never read.*
10. Press **Alt + C**. ✅ No picker opens: a yellow border appears around **every** screen at
    once, the tray circle turns red, a soft sound, no text on screen. ✅ Home says
    *0 moments saved · 2 screens shared* (or your number of screens, 4 at most), and **Add a
    screen** is greyed out (in the tray menu too: *Add a screen (2 shared)*).
11. Work in a window on one screen for 20 seconds, then on the other for 20 seconds.
    ✅ The tray menu shows no *Skipping* line.
12. Stop the recording (**Alt + C**). ✅ The borders go away. In `app.log` (part 12) there's a
    line *all screens: 2 shared*. (If it says *… the picker instead*, the picker opened instead
    of sharing every screen: please send us that line.)
13. Set **When you start** back to **Pick a screen or window**. ✅ Home says *Pick the screen or
    window you work on. Only that is read.* and Alt+C opens the picker again.

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
4. Check that **Your AI** (Settings → Your AI) is **Claude**. Click **Make**. ✅ The Claude app
   opens on a new chat with *Use Context Capsule to make my capsules for today.* already typed in
   (Claude may show a small caution note; that's expected). Don't send it yet: part 10 does.
   (With another app as Your AI, Make copies the request for it instead; with a website, it is
   Copy my day: part 11.)
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
   click **⋯** → **Copy file**.
6. **Check before sharing**: if the capsule has names, emails, phone numbers, links, amounts
   or file paths, ✅ the window shows *Check before sharing* with them in groups (People,
   Emails, …), all checked. Uncheck one, click **Share**, and paste into a chat or a folder
   ✅ the checked ones say `[removed]`, the unchecked one is still there. Click **Open** on the
   capsule ✅ your saved capsule is unchanged. Try **Copy text** and **Save a copy…** the same
   way, and **Cancel** (or Esc) once ✅ nothing is copied. A capsule with nothing private
   ✅ copies at once. **Show in folder** works too.
7. **Search**: on **Capsules**, type a word from one capsule's title or *In short* in the search
   box ✅ only matching capsules stay; Esc clears it. **Ask Claude** (it says **Ask your AI** when
   Your AI isn't Claude) is grey while the box is empty. Type a question (e.g. *what did I decide about pricing?*) and click **Ask Claude** (or
   press Enter) ✅ Claude opens on a new chat with *Use Context Capsule to search my capsules: …*
   typed in.

## 11. Any other AI: Copy my day (ChatGPT website)

For any AI without Context Capsule built in (a website, Copilot…): copy your day into it, then save
its reply.

1. Make sure you recorded something today (part 3). Open the window → **Settings** → **Your AI** →
   *Make today's capsules with* → **ChatGPT (website)**. ✅ Under it: *Make today's capsules copies
   your day to paste into ChatGPT, then you save its reply.*
2. **Home** → *Make today's capsules* ✅ says *Copy your day, paste it into ChatGPT, then save its
   reply here.*, with **Copy my day** and **Save from clipboard**, and under them *Your AI: ChatGPT*
   and **Change** (✅ it goes to Settings). Right-click the tray icon ✅ **Save from clipboard** is
   right under *Make today's capsules*.
3. Click **Copy my day**. ✅ chatgpt.com opens in your browser, a message near the tray says *Paste
   it into your AI. When it answers, copy its whole reply, then click Save from clipboard.*, and Home
   shows that line too (so does **Capsules**).
4. In ChatGPT, start a new chat, paste (Ctrl+V) and send. ✅ It answers with your capsules (each
   starts with a line `=== CAPSULE ===`).
5. Copy its **whole** reply (the copy button under the answer), then click **Save from clipboard**.
   ✅ A box says *Saved 2 capsules* (your number) with a ✓ line for each; any it couldn't save are
   listed under *Not saved:* with the reason. ✅ They are on **Capsules** and in Home's *Recent
   capsules*, and the paste line is gone.
6. Copy a few plain words (not a capsule) and click **Save from clipboard** ✅ *No capsules found*.
   Take a screenshot with **Win + Shift + S** (now the clipboard has no text) and click it again
   ✅ *The clipboard is empty* — *Copy your AI's whole reply first, then click Save from clipboard.*
7. Copy the reply again and use the tray's **Save from clipboard** ✅ the same box.
8. **Capsules** page ✅ *Using an AI on the web, or any other AI?* with **Copy my day** and **Save
   from clipboard**; the Ask button says **Ask your AI**.
9. **Your AI** → **Any other AI** ✅ Home says *Your AI: any AI*; **Copy my day** copies but opens no
   website. Paste it anywhere (Notepad) to see it: ✅ one long message, with your day in it.
10. Optional, with an app from part 1: pick one that is **not connected** (*VS Code (not
    connected)*), click **Make capsules** on Home ✅ *Connect VS Code first* with **Connect** and
    **Cancel**; **Connect** connects it. With a connected one (e.g. Cursor) ✅ *Copied. Open Cursor
    and paste it in a new chat.*: paste it in a new chat in that app.
11. Set **Your AI** back to **Claude** ✅ Home's button says **Make capsules**, and the tray menu has
    no **Save from clipboard**.

## 12. Send us the results

1. Open `C:\Users\<you>\.context-capsule\sessions\`.
2. **Look inside first**: each session folder has an `events.jsonl` file with the text
   that was read from your screen. Delete any session you don't want to share.
3. Check: does any `events.jsonl` contain `TestPass123` or text from a private app?
   It **must not**. From part 3, the long page you scrolled should be there in full, each
   line once (later moments of the same window only add the lines that were new).
4. Zip the `sessions` folder and send it, together with:
   - `app.log`, one folder up in `C:\Users\<you>\.context-capsule\` (it only has times,
     counts, reasons and error codes, never what was on screen or what you said; from part 11,
     lines like *copy my day: 12345 characters copied* and *save from clipboard: 2 saved, 0 refused*),
   - the text from the PowerShell window (select all, copy, paste into a file),
   - screenshots of anything that looked wrong (Win + Shift + S),
   - your Windows version (Settings → System → About), and your screen scaling
     (Settings → System → Display → Scale, e.g. 125%).
5. Quit: tray icon → **Quit Context Capsule**.

## 13. Install, update, uninstall

1. Quit Context Capsule. Double-click `Context-Capsule-0.1.0-windows.exe` in its folder.
   ✅ A box asks *Install Context Capsule?* Click **Install**.
2. ✅ The tray icon comes back. In the Start menu, ✅ *Context Capsule* is there. In
   `%LOCALAPPDATA%\Programs\Context Capsule\` ✅ you see `context-capsule.exe`,
   `context-capsule.mcpb` and `context-capsule-mcp.exe` (written a few seconds after it starts). No
   admin question came up. Apps you connected in part 1 still point at the copy you ran first:
   in Settings → Connect your AI, **⋯** → **Disconnect** and **Connect** them again so they use
   the installed one (its path stays the same across updates).
3. Settings → Apps → Installed apps: ✅ *Context Capsule*, publisher *Karya*, version 0.1.0.
4. If open at login was on (part 1): Settings → Apps → Startup ✅ still lists Context Capsule
   (it now starts the installed copy).
5. **Open a capsule someone sent**: save a copy of one of your capsules to the Desktop
   (Capsules → ⋯ → **Save a copy…**). Right-click it → **Open with** (Windows 11: *Show more
   options* first if needed) ✅ *Context Capsule* is in the list, and double-clicking a `.md`
   file still opens your usual app. Choose **Context Capsule** ✅ Claude opens on a new chat with
   *Use Context Capsule to continue from the capsule at C:\Users\…* typed in (with Context
   Capsule running, and again after quitting it). Do the same with any other `.md` file ✅ a box
   says *Not a capsule*. **Capsules → Open a capsule…** does the same from a file dialog.
6. Right-click the tray icon → **Check for updates**. ✅ A message says *You have the latest
   version.* or *Couldn't check for updates. Try again later.* (until we publish a release,
   the second one is expected). An update itself can only be tested once a newer release is
   out: then a box says *Context Capsule 0.2.0 is ready. Restart to update.*, **Restart now**
   starts the new version within a few seconds, and Settings → Apps shows the new version.
7. Settings → Apps → Context Capsule → **Uninstall**. ✅ The tray icon goes away, and a box asks
   *Also delete your recordings and capsules?* Click **No**. ✅ *Context Capsule is removed.*
8. ✅ The Start menu entry, the Settings → Apps entry and the folder
   `%LOCALAPPDATA%\Programs\Context Capsule` are gone, and
   `C:\Users\<you>\.context-capsule\` is still there. Right-click a `.md` file → **Open with**
   ✅ Context Capsule is no longer offered.

## Notes for the tester

- Nothing is uploaded by Context Capsule. The internet is used for the one-time speech
  model download in part 8, from huggingface.co, after you say yes, and to check for a new
  version on github.com (nothing about you is sent). Your AI only sees your
  recordings when you ask it to (part 10), or when you paste your day into it yourself (part 11).
- To remove everything: uninstall it (part 13) and say **Yes** to deleting your recordings,
  remove *Context Capsule* in Claude (Settings → Extensions), **Disconnect** the other AI apps first
  (Settings → Connect your AI), and delete the downloaded file.
