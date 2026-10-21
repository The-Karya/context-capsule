// context-capsule — windows: the evening "Make today's capsules?" box (Make / Not now) and what Make does:
// the request goes on the clipboard and the Claude app opens on a new chat (its claude: link when registered,
// else the app itself); then "In Claude, press Ctrl+V, then Enter." The same for any request: a received
// capsule ("continue from…") and Ask Claude ("search my capsules…").

use std::path::{Path, PathBuf};

/// How to open the Claude app.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClaudeOpener {
    /// The claude: link scheme is registered: open a new chat.
    Link,
    /// No link scheme, but the app is installed: start it.
    App(PathBuf),
    /// No Claude app.
    Missing,
}

/// The way to open Claude, from what is installed.
pub fn opener(link_registered: bool, app: Option<&Path>) -> ClaudeOpener {
    match (link_registered, app) {
        (true, _) => ClaudeOpener::Link,
        (false, Some(exe)) => ClaudeOpener::App(exe.to_path_buf()),
        (false, None) => ClaudeOpener::Missing,
    }
}

#[cfg(windows)]
pub use win::{ask_and_make, ask_claude, make, open_received};

#[cfg(windows)]
mod win {
    use super::*;
    use crate::app::{Ui, UiCmd};
    use crate::{connect, input, win};
    use context_capsule_core::claude::{self, new_chat_url, Opened};
    use context_capsule_core::reminder::{CLAUDE_NEW_CHAT, DAY_PROMPT, LATER_BUTTON, MAKE_BUTTON, PASTE_HINT, REMIND_QUESTION};
    use context_capsule_core::AppLog;

    /// The claude: scheme, registered for this user or the machine.
    fn link_registered() -> bool {
        win::reg_string_classes(r"claude\shell\open\command", "").is_some_and(|c| !c.trim().is_empty())
    }

    fn installed_app() -> Option<PathBuf> {
        let local = std::env::var_os("LOCALAPPDATA").filter(|v| !v.is_empty())?;
        let exe = PathBuf::from(local).join("AnthropicClaude").join("claude.exe");
        exe.is_file().then_some(exe)
    }

    /// Ask (on a thread of its own: the box has its own modal loop); Make: clipboard, Claude, a hint.
    pub fn ask_and_make(ui: Ui, log: AppLog) {
        std::thread::spawn(move || {
            if !input::ask_choice("Context Capsule", REMIND_QUESTION, MAKE_BUTTON, LATER_BUTTON) {
                log.write("evening reminder: not now");
                return;
            }
            log.write("evening reminder: make");
            make(&ui, &log);
        });
    }

    /// "Make today's capsules": Claude with the request typed in.
    pub fn make(ui: &Ui, log: &AppLog) {
        debug_assert_eq!(new_chat_url(DAY_PROMPT), CLAUDE_NEW_CHAT);
        ask_claude(ui, log, DAY_PROMPT, "make capsules");
    }

    /// Claude on a new chat with `prompt` typed in (and on the clipboard, for when only the app opens).
    /// Blocking (it may ask or tell something): call on a thread of its own. `what`: for the log.
    pub fn ask_claude(ui: &Ui, log: &AppLog, prompt: &str, what: &str) {
        let how = opener(link_registered(), installed_app().as_deref());
        if how == ClaudeOpener::Missing {
            log.write(&format!("{what}: the Claude app is not installed"));
            let body = "Context Capsule works with the Claude desktop app. Download it, sign in and \
                        connect Context Capsule (tray menu → Open Context Capsule → Settings).\n\nOpen the download page now?";
            if win::ask_yes_no("Install the Claude app first", body) {
                win::open(connect::DOWNLOAD_URL);
            }
            return;
        }
        let copied = win::copy_text(prompt);
        let typed_in = how == ClaudeOpener::Link;
        let opened = match &how {
            ClaudeOpener::Link => win::open(&new_chat_url(prompt)),
            ClaudeOpener::App(exe) => win::open(&exe.to_string_lossy()),
            ClaudeOpener::Missing => false,
        };
        log.write(&format!("{what}: clipboard {}, Claude {}", if copied { "ok" } else { "failed" }, if opened { "opened" } else { "not opened" }));
        if !opened {
            win::alert(None, "Couldn't open Claude", &format!("Open the Claude app, start a new chat and type:\n\n{prompt}"));
        } else if typed_in {
            // The request is already typed in Claude: nothing to say.
        } else if copied {
            ui.post(UiCmd::Toast(PASTE_HINT.into()));
        } else {
            win::alert(None, "Claude", &format!("In Claude, type:\n\n{prompt}"));
        }
    }

    /// A capsule someone sent (Open with, or "Open a capsule…"): Claude carries on from it. Not a capsule: say so.
    /// Blocking: call on a thread of its own.
    pub fn open_received(ui: &Ui, log: &AppLog, path: &Path) {
        match claude::open_file(path) {
            Opened::Capsule { prompt } => ask_claude(ui, log, &prompt, "open a received capsule"),
            Opened::NotACapsule => {
                log.write("open a received capsule: not a capsule");
                win::alert(None, claude::NOT_A_CAPSULE, claude::NOT_A_CAPSULE_BODY);
            }
            Opened::Unreadable => {
                log.write("open a received capsule: cannot read it");
                win::alert(None, claude::NOT_A_CAPSULE, claude::CANT_READ);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_first_then_the_app() {
        let exe = Path::new(r"C:\Users\s\AppData\Local\AnthropicClaude\claude.exe");
        assert_eq!(opener(true, Some(exe)), ClaudeOpener::Link);
        assert_eq!(opener(true, None), ClaudeOpener::Link);
        assert_eq!(opener(false, Some(exe)), ClaudeOpener::App(exe.to_path_buf()));
        assert_eq!(opener(false, None), ClaudeOpener::Missing);
        assert!(context_capsule_core::reminder::CLAUDE_NEW_CHAT.starts_with("claude://claude.ai/new?q=Use%20Context%20Capsule"));
    }
}
