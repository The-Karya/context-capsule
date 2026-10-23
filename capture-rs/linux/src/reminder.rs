// context-capsule — linux: the evening "Make today's capsules?" question (Make / Not now; Make follows Your AI);
// the clipboard without the window (wl-copy / wl-paste, xclip or xsel); Claude with a request (on the
// clipboard, and the Claude app on a new chat through its claude: link). Also plays the soft start/stop sounds.

use crate::notify::{on_path, Notifier};
use context_capsule_core::claude::new_chat_url;
use context_capsule_core::reminder::{LATER_BUTTON, MAKE_BUTTON, REMIND_QUESTION};
use context_capsule_core::Cue;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// Clipboard tools to try, in order: wl-copy on Wayland, then the X11 ones (Xwayland apps read those too).
pub fn clipboard_tools(wayland: bool) -> Vec<(&'static str, &'static [&'static str])> {
    let mut tools: Vec<(&'static str, &'static [&'static str])> = vec![];
    if wayland {
        tools.push(("wl-copy", &[]));
    }
    tools.push(("xclip", &["-selection", "clipboard"]));
    tools.push(("xsel", &["--clipboard", "--input"]));
    tools
}

/// Tools that read the clipboard, in the same order.
pub fn paste_tools(wayland: bool) -> Vec<(&'static str, &'static [&'static str])> {
    let mut tools: Vec<(&'static str, &'static [&'static str])> = vec![];
    if wayland {
        tools.push(("wl-paste", &["--no-newline"]));
    }
    tools.push(("xclip", &["-selection", "clipboard", "-o"]));
    tools.push(("xsel", &["--clipboard", "--output"]));
    tools
}

fn wayland() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some_and(|v| !v.is_empty())
}

/// Sound players to try, in order (PipeWire, PulseAudio, ALSA).
pub const PLAYERS: [&str; 3] = ["pw-play", "paplay", "aplay"];

/// Put `text` on the clipboard. false: no clipboard tool is installed, or it failed.
pub fn copy_text(text: &str) -> bool {
    for (tool, args) in clipboard_tools(wayland()) {
        if !on_path(tool) {
            continue;
        }
        let Ok(mut child) = Command::new(tool).args(args).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn()
        else {
            continue;
        };
        let wrote = child.stdin.take().is_some_and(|mut stdin| stdin.write_all(text.as_bytes()).is_ok());
        // wl-copy and xclip stay in the background to serve the clipboard; reap them later.
        std::thread::spawn(move || child.wait());
        if wrote {
            return true;
        }
    }
    false
}

/// The clipboard's text ("" when it holds none). None: no clipboard tool is installed.
pub fn paste_text() -> Option<String> {
    let (tool, args) = paste_tools(wayland()).into_iter().find(|(tool, _)| on_path(tool))?;
    let out = Command::new(tool).args(args).stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
    // wl-paste and xclip fail on an empty clipboard: that's empty, not missing.
    Some(if out.status.success() { String::from_utf8_lossy(&out.stdout).into_owned() } else { String::new() })
}

/// Open a link with its app. false: nothing handles it.
pub fn xdg_open(uri: &str) -> bool {
    Command::new("xdg-open")
        .arg(uri)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Claude with `prompt`, without the window: on the clipboard, and the Claude app on a new chat with it typed in
/// (xdg-open of its claude: link). Can't: say how to do it by hand.
pub fn claude_with(notifier: &Notifier, prompt: &str) {
    let copied = copy_text(prompt);
    if !xdg_open(&new_chat_url(prompt)) {
        notifier.alert("Couldn't open Claude", &format!("Open the Claude app (or claude.ai), start a new chat and type:\n\n{prompt}"));
    } else if !copied {
        notifier.alert("Claude", &format!("In Claude, type:\n\n{prompt}"));
    }
}

/// Ask (on a thread of its own: the dialog blocks); Make: `make` (Make today's capsules, the way Your AI works).
pub fn ask_and_make(notifier: Notifier, make: Box<dyn FnOnce() + Send>) {
    std::thread::spawn(move || {
        if !notifier.ask(context_capsule_core::window::APP_NAME, REMIND_QUESTION, MAKE_BUTTON, LATER_BUTTON) {
            eprintln!("Context Capsule: evening reminder: not now");
            return;
        }
        eprintln!("Context Capsule: evening reminder: make");
        make();
    });
}

/// Where the sound files go: made once per session in the runtime folder (memory-backed), never the data folder.
fn sound_file(cue: Cue) -> Option<PathBuf> {
    let dir = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join("context-capsule");
    let path = dir.join(format!("{}.wav", cue.name()));
    if !path.is_file() {
        std::fs::create_dir_all(&dir).ok()?;
        std::fs::write(&path, cue.wav()).ok()?;
    }
    Some(path)
}

/// Play a soft cue without waiting, with the first player found. Silent when there is none.
pub fn play_cue(cue: Cue) {
    let Some(path) = sound_file(cue) else { return };
    let Some(player) = PLAYERS.iter().find(|p| on_path(p)) else { return };
    if let Ok(mut child) =
        Command::new(player).arg(&path).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn()
    {
        std::thread::spawn(move || child.wait());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_tools_in_order() {
        let names = |w| clipboard_tools(w).into_iter().map(|(t, _)| t).collect::<Vec<_>>();
        assert_eq!(names(true), ["wl-copy", "xclip", "xsel"]);
        assert_eq!(names(false), ["xclip", "xsel"], "no wl-copy without Wayland");
        let reads = |w| paste_tools(w).into_iter().map(|(t, a)| format!("{t} {}", a.join(" "))).collect::<Vec<_>>();
        assert_eq!(reads(true), ["wl-paste --no-newline", "xclip -selection clipboard -o", "xsel --clipboard --output"]);
        assert_eq!(reads(false).len(), 2, "no wl-paste without Wayland");
        let day = context_capsule_core::reminder::DAY_PROMPT;
        assert_eq!(new_chat_url(day), context_capsule_core::reminder::CLAUDE_NEW_CHAT);
        assert!(new_chat_url(day).starts_with("claude://claude.ai/new?q=Use%20Context%20Capsule"));
    }

    #[test]
    fn sound_files_are_made_once() {
        let path = sound_file(Cue::MicOpened).expect("a writable runtime folder");
        assert_eq!(std::fs::read(&path).unwrap(), Cue::MicOpened.wav());
        assert!(path.to_string_lossy().ends_with("context-capsule/mic-opened.wav"));
    }
}
