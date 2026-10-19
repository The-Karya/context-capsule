// context-capsule — linux: the global hotkeys Ctrl+Alt+Shift+C (record) and Ctrl+Alt+Shift+V (voice).
// Wayland: the GlobalShortcuts portal. X11: XGrabKey (see x11.rs). Otherwise the tray menu only.

use crate::desktop::SessionKind;
use ashpd::desktop::global_shortcuts::{GlobalShortcuts, NewShortcut};
use context_capsule_core::Msg;
use futures_util::StreamExt;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;
use tokio::runtime::Handle;

pub const RECORD_ID: &str = "toggle-recording";
pub const VOICE_ID: &str = "toggle-voice-note";

/// The message a portal shortcut id stands for.
pub fn message_for(id: &str) -> Option<Msg> {
    match id {
        RECORD_ID => Some(Msg::ToggleRecord),
        VOICE_ID => Some(Msg::ToggleVoice),
        _ => None,
    }
}

/// The two shortcuts, with the preferred triggers in the XDG shortcuts format.
pub fn shortcuts() -> Vec<NewShortcut> {
    vec![
        NewShortcut::new(RECORD_ID, "Start or stop recording").preferred_trigger("CTRL+ALT+SHIFT+c"),
        NewShortcut::new(VOICE_ID, "Add or finish a voice note").preferred_trigger("CTRL+ALT+SHIFT+v"),
    ]
}

/// Set up the hotkeys for this session. On X11 the key grabs share the watcher thread that also
/// notes app switches (`changed`).
pub fn start(rt: &Handle, session: SessionKind, tx: Sender<Msg>, changed: Arc<Mutex<Option<SystemTime>>>) {
    match session {
        SessionKind::X11 => {
            if !crate::x11::spawn_watcher(changed, tx, true) {
                eprintln!("Context Capsule: cannot open the X display; hotkeys off, use the tray menu");
            }
        }
        SessionKind::Wayland => {
            rt.spawn(async move {
                if let Err(e) = portal_shortcuts(tx).await {
                    eprintln!("Context Capsule: no global hotkeys ({e}); use the tray menu");
                }
            });
        }
        SessionKind::Unknown => eprintln!("Context Capsule: no desktop session; hotkeys off, use the tray menu"),
    }
}

/// Bind through the portal (the desktop may ask the user to confirm), then forward each press.
async fn portal_shortcuts(tx: Sender<Msg>) -> Result<(), String> {
    let portal = GlobalShortcuts::new().await.map_err(|e| e.to_string())?;
    let session = portal.create_session(Default::default()).await.map_err(|e| e.to_string())?;
    let mut activated = portal.receive_activated().await.map_err(|e| e.to_string())?;
    let bound = portal
        .bind_shortcuts(&session, &shortcuts(), None, Default::default())
        .await
        .and_then(|r| r.response())
        .map_err(|e| e.to_string())?;
    for s in bound.shortcuts() {
        eprintln!("Context Capsule: hotkey {} = {}", s.id(), s.trigger_description());
    }
    while let Some(a) = activated.next().await {
        if let Some(msg) = message_for(a.shortcut_id()) {
            if tx.send(msg).is_err() {
                break;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcut_ids() {
        assert_eq!(message_for(RECORD_ID), Some(Msg::ToggleRecord));
        assert_eq!(message_for(VOICE_ID), Some(Msg::ToggleVoice));
        assert_eq!(message_for("other"), None);
        assert_eq!(shortcuts().len(), 2);
    }
}
