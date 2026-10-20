// context-capsule — linux: the evening "Make today's capsules?" question (Make / Not now) and what Make does:
// the request goes on the clipboard (wl-copy, xclip or xsel), the Claude app opens on a new chat (xdg-open of
// its claude: link), then "In Claude, press Ctrl+V, then Enter." Also plays the soft start/stop sounds.

use crate::notify::{on_path, Notifier};
use context_capsule_core::reminder::{CLAUDE_NEW_CHAT, DAY_PROMPT, LATER_BUTTON, MAKE_BUTTON, REMIND_QUESTION};
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

/// Sound players to try, in order (PipeWire, PulseAudio, ALSA).
pub const PLAYERS: [&str; 3] = ["pw-play", "paplay", "aplay"];

/// Put `text` on the clipboard. false: no clipboard tool is installed, or it failed.
fn copy_text(text: &str) -> bool {
    let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some_and(|v| !v.is_empty());
    for (tool, args) in clipboard_tools(wayland) {
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

/// Open the Claude app on a new chat. false: nothing handles claude: links (no Claude app).
fn open_claude() -> bool {
    Command::new("xdg-open")
        .arg(CLAUDE_NEW_CHAT)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Ask (on a thread of its own: the dialog blocks); Make: clipboard, Claude, a hint.
pub fn ask_and_make(notifier: Notifier) {
    std::thread::spawn(move || {
        if !notifier.ask("Context Capsule", REMIND_QUESTION, MAKE_BUTTON, LATER_BUTTON) {
            eprintln!("Context Capsule: evening reminder: not now");
            return;
        }
        eprintln!("Context Capsule: evening reminder: make");
        let copied = copy_text(DAY_PROMPT);
        if !open_claude() {
            notifier.alert(
                "Couldn't open Claude",
                &format!("Open the Claude app (or claude.ai), start a new chat and type:\n\n{DAY_PROMPT}"),
            );
        } else if !copied {
            notifier.alert("Make today's capsules", &format!("In Claude, type:\n\n{DAY_PROMPT}"));
        }
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
        assert!(CLAUDE_NEW_CHAT.starts_with("claude://claude.ai/new?q=Use%20Context%20Capsule"));
    }

    #[test]
    fn sound_files_are_made_once() {
        let path = sound_file(Cue::MicOpened).expect("a writable runtime folder");
        assert_eq!(std::fs::read(&path).unwrap(), Cue::MicOpened.wav());
        assert!(path.to_string_lossy().ends_with("context-capsule/mic-opened.wav"));
    }
}
