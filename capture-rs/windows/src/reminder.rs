// context-capsule — windows: the evening "Make today's capsules?" box (Make / Not now; Make follows Your AI: see
// `ai::make`), and Claude's flow for a request: it goes on the clipboard and the Claude app opens on a new chat
// (its claude: link when registered, else the app, its alias or its packaged app: see connect::find); then
// "In Claude, press Ctrl+V, then Enter." Not found: it asks whether Claude is installed. Used when Your AI is
// Claude (and, as on the Mac, for requests when it is a website or any other AI): make today's capsules, a
// received capsule ("continue from…") and Ask ("search my capsules…").

use crate::connect::{ClaudeOpener, Found};

/// When Claude couldn't be found, but the user says it's installed: the request is on the clipboard.
pub fn paste_by_hand(prompt: &str) -> String {
    format!("Open the Claude app, start a new chat, then press Ctrl+V and Enter. Or type:\n\n{prompt}")
}

/// Whether Claude opens on a new chat with the request typed in (its link), or only the app opens (paste it).
pub fn typed_in(how: &ClaudeOpener) -> bool {
    *how == ClaudeOpener::Link
}

/// The way to open Claude, from what was found.
pub fn opener(found: &Found) -> ClaudeOpener {
    found.opener()
}

#[cfg(windows)]
pub use win::{ask_and_make, ask_claude, make};

#[cfg(windows)]
mod win {
    use super::*;
    use crate::app::{Ui, UiCmd};
    use crate::{connect, input, win};
    use connect::{open_claude, IS_IT_INSTALLED};
    use context_capsule_core::claude::new_chat_url;
    use context_capsule_core::reminder::{CLAUDE_NEW_CHAT, DAY_PROMPT, LATER_BUTTON, MAKE_BUTTON, PASTE_HINT, REMIND_QUESTION};
    use context_capsule_core::AppLog;

    /// Ask (on a thread of its own: the box has its own modal loop); Make: the UI thread makes today's capsules
    /// with Your AI.
    pub fn ask_and_make(ui: Ui, log: AppLog) {
        std::thread::spawn(move || {
            if !input::ask_choice("Context Capsule", REMIND_QUESTION, MAKE_BUTTON, LATER_BUTTON) {
                log.write("evening reminder: not now");
                return;
            }
            log.write("evening reminder: make");
            ui.post(UiCmd::Make);
        });
    }

    /// "Make today's capsules" when Your AI is Claude: Claude with the request typed in.
    pub fn make(ui: &Ui, log: &AppLog) {
        debug_assert_eq!(new_chat_url(DAY_PROMPT), CLAUDE_NEW_CHAT);
        ask_claude(ui, log, DAY_PROMPT, "make capsules");
    }

    /// Claude on a new chat with `prompt` typed in (and on the clipboard, for when only the app opens).
    /// Blocking (it may ask or tell something): call on a thread of its own. `what`: for the log.
    pub fn ask_claude(ui: &Ui, log: &AppLog, prompt: &str, what: &str) {
        let found = connect::find();
        log.write(&format!("{what}: {}", found.log_line()));
        let how = opener(&found);
        if how == ClaudeOpener::Missing {
            // Not found: don't assume it's missing.
            let body = "Context Capsule couldn't find the Claude desktop app on this PC.\n\n\
                        Yes: copy the request, then you open Claude.\nNo: open the download page.";
            if win::ask_yes_no(IS_IT_INSTALLED, body) {
                let copied = win::copy_text(prompt);
                log.write(&format!("{what}: not found, user says installed; clipboard {}", if copied { "ok" } else { "failed" }));
                win::alert(None, "Claude", &paste_by_hand(prompt));
            } else {
                log.write(&format!("{what}: the Claude app is not installed: download page"));
                win::open(connect::DOWNLOAD_URL);
            }
            return;
        }
        let copied = win::copy_text(prompt);
        let opened = open_claude(&how, &new_chat_url(prompt));
        log.write(&format!("{what}: clipboard {}, Claude {} ({how:?})", if copied { "ok" } else { "failed" }, if opened { "opened" } else { "not opened" }));
        if !opened {
            win::alert(None, "Couldn't open Claude", &paste_by_hand(prompt));
        } else if typed_in(&how) {
            // The request is already typed in Claude: nothing to say.
        } else if copied {
            ui.post(UiCmd::Toast(PASTE_HINT.into()));
        } else {
            win::alert(None, "Claude", &format!("In Claude, type:\n\n{prompt}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn link_first_then_the_app() {
        let exe = PathBuf::from(r"C:\Users\s\AppData\Local\AnthropicClaude\claude.exe");
        let both = Found { link: true, exe: Some(exe.clone()), ..Default::default() };
        assert_eq!(opener(&both), ClaudeOpener::Link);
        assert!(typed_in(&opener(&both)));
        assert_eq!(opener(&Found { exe: Some(exe.clone()), ..Default::default() }), ClaudeOpener::App(exe.clone()));
        let alias = PathBuf::from(r"C:\Users\s\AppData\Local\Microsoft\WindowsApps\claude.exe");
        assert_eq!(opener(&Found { alias: Some(alias.clone()), ..Default::default() }), ClaudeOpener::App(alias), "the app alias");
        assert_eq!(opener(&Found::default()), ClaudeOpener::Missing);
        assert!(!typed_in(&ClaudeOpener::App(exe)));
        assert!(paste_by_hand("Use Context Capsule").ends_with("Use Context Capsule"));
        assert!(context_capsule_core::reminder::CLAUDE_NEW_CHAT.starts_with("claude://claude.ai/new?q=Use%20Context%20Capsule"));
    }
}
