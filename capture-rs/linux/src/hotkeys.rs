// context-capsule — linux: the global hotkeys Alt+C (record) and Alt+V (voice), like the Mac's ⌥C / ⌥V.
// Wayland: the GlobalShortcuts portal. X11: XGrabKey (see x11.rs). Otherwise the tray menu only.
// A hotkey another app already has is logged and said once; the tray menu still works.

use crate::desktop::SessionKind;
use crate::notify::Notifier;
use crate::x11::Watch;
use ashpd::desktop::global_shortcuts::{GlobalShortcuts, NewShortcut};
use context_capsule_core::{hotkeys_taken_message, Msg};
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
        NewShortcut::new(RECORD_ID, "Start or stop contextualizing").preferred_trigger("ALT+c"),
        NewShortcut::new(VOICE_ID, "Add or finish a voice note").preferred_trigger("ALT+v"),
    ]
}

/// The hotkey labels for shortcut ids the desktop bound without a key (another app has it).
pub fn unbound(bound: &[(String, String)]) -> Vec<&'static str> {
    [(RECORD_ID, crate::tray::RECORD_KEY), (VOICE_ID, crate::tray::VOICE_KEY)]
        .into_iter()
        .filter(|(id, _)| bound.iter().any(|(b, trigger)| b == id && trigger.trim().is_empty()))
        .map(|(_, label)| label)
        .collect()
}

/// Set up the hotkeys for this session. On X11 the key grabs share the watcher thread that also
/// notes app switches (`changed`) and mouse scrolls (`scrolled`).
pub fn start(
    rt: &Handle,
    session: SessionKind,
    tx: Sender<Msg>,
    changed: Arc<Mutex<Option<SystemTime>>>,
    scrolled: Arc<Mutex<Option<SystemTime>>>,
    notifier: Notifier,
) {
    let taken = move |keys: Vec<&'static str>| notifier.toast(&hotkeys_taken_message(&keys));
    match session {
        SessionKind::X11 => {
            let watch = Watch { changed, scrolled, tx, grab: true, taken: Box::new(taken) };
            if !crate::x11::spawn_watcher(watch) {
                eprintln!("Context Capsule: cannot open the X display; hotkeys off, use the tray menu");
            }
        }
        SessionKind::Wayland => {
            rt.spawn(async move {
                if let Err(e) = portal_shortcuts(tx, taken).await {
                    eprintln!("Context Capsule: no global hotkeys ({e}); use the tray menu");
                }
            });
        }
        SessionKind::Unknown => eprintln!("Context Capsule: no desktop session; hotkeys off, use the tray menu"),
    }
}

/// Bind through the portal (the desktop may ask the user to confirm), then forward each press.
async fn portal_shortcuts(tx: Sender<Msg>, taken: impl FnOnce(Vec<&'static str>)) -> Result<(), String> {
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
    let pairs: Vec<(String, String)> =
        bound.shortcuts().iter().map(|s| (s.id().to_string(), s.trigger_description().to_string())).collect();
    let missing = unbound(&pairs);
    if !missing.is_empty() {
        eprintln!("Context Capsule: {} not bound (taken by another app?); use the tray menu", missing.join(", "));
        taken(missing);
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

    #[test]
    fn unbound_keys() {
        let b = |id: &str, t: &str| (id.to_string(), t.to_string());
        assert!(unbound(&[b(RECORD_ID, "Alt+C"), b(VOICE_ID, "Alt+V")]).is_empty());
        assert_eq!(unbound(&[b(RECORD_ID, "Alt+C"), b(VOICE_ID, "")]), ["Alt+V"]);
        assert_eq!(unbound(&[b(RECORD_ID, " "), b(VOICE_ID, "")]), ["Alt+C", "Alt+V"]);
    }
}
