// context-capsule — linux: the global hotkeys — your own shortcuts (config.json "hotkeys", default Alt+C to
// record and Alt+V for a voice note, like the Mac's ⌥C / ⌥V). Wayland: the GlobalShortcuts portal. X11: XGrabKey
// (see x11.rs). Otherwise the tray menu only. A hotkey another app already has is logged and said once; the tray
// menu still works. Settings → Shortcuts changes them while the app runs ([`Control`]).
#![cfg_attr(not(feature = "window"), allow(dead_code))]

use crate::desktop::SessionKind;
use crate::notify::Notifier;
use crate::x11::{Grabs, Watch};
use ashpd::desktop::global_shortcuts::{GlobalShortcuts, NewShortcut};
use ashpd::desktop::Session;
use context_capsule_core::shortcuts::{Hotkeys, Key, Refusal, Shortcut, Which};
use context_capsule_core::{hotkeys_taken_message, Msg};
use futures_util::StreamExt;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, SystemTime};
use tokio::runtime::Handle;

pub const RECORD_ID: &str = "toggle-recording";
pub const VOICE_ID: &str = "toggle-voice-note";

/// How long a change waits for the desktop (it may show a dialog).
const DESKTOP_WAIT: Duration = Duration::from_secs(120);

/// The shortcuts in use now, for every place that names them (tray, messages, voice).
static CURRENT: RwLock<Hotkeys> = RwLock::new(Hotkeys::DEFAULT);

pub fn current() -> Hotkeys {
    *CURRENT.read().unwrap_or_else(|e| e.into_inner())
}

pub fn set_current(h: Hotkeys) {
    *CURRENT.write().unwrap_or_else(|e| e.into_inner()) = h;
}

/// The message a portal shortcut id stands for.
pub fn message_for(id: &str) -> Option<Msg> {
    match id {
        RECORD_ID => Some(Msg::ToggleRecord),
        VOICE_ID => Some(Msg::ToggleVoice),
        _ => None,
    }
}

fn portal_id(which: Which) -> &'static str {
    match which {
        Which::Record => RECORD_ID,
        Which::Voice => VOICE_ID,
    }
}

/// A shortcut in the XDG shortcuts format the portal takes: modifiers CTRL, ALT, SHIFT, LOGO joined by `+`,
/// then the key's xkb keysym name ("ALT+c", "CTRL+SHIFT+r", "F5").
pub fn portal_trigger(s: &Shortcut) -> String {
    let mut parts: Vec<String> = [(s.ctrl, "CTRL"), (s.alt, "ALT"), (s.shift, "SHIFT"), (s.cmd, "LOGO")]
        .iter()
        .filter(|(on, _)| *on)
        .map(|(_, m)| m.to_string())
        .collect();
    parts.push(match s.key {
        Key::Letter(c) => c.to_ascii_lowercase().to_string(),
        Key::Digit(d) => d.to_string(),
        Key::F(n) => format!("F{n}"),
    });
    parts.join("+")
}

/// The two shortcuts, with the preferred triggers.
pub fn shortcuts(h: &Hotkeys) -> Vec<NewShortcut> {
    vec![
        NewShortcut::new(RECORD_ID, "Start or stop contextualizing").preferred_trigger(portal_trigger(&h.record).as_str()),
        NewShortcut::new(VOICE_ID, "Add or finish a voice note").preferred_trigger(portal_trigger(&h.voice).as_str()),
    ]
}

/// The labels of the shortcuts the desktop bound without a key (another app has it).
pub fn unbound(bound: &[(String, String)], h: &Hotkeys) -> Vec<String> {
    [(RECORD_ID, h.record), (VOICE_ID, h.voice)]
        .into_iter()
        .filter(|(id, _)| bound.iter().any(|(b, trigger)| b == id && trigger.trim().is_empty()))
        .map(|(_, s)| s.label())
        .collect()
}

/// What came of a change that was not refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Applied {
    /// In use now.
    Done,
    /// Asked of the desktop (Wayland): what it says each one is now ("" = not set).
    Desktop(Vec<(Which, String)>),
}

/// Ask the portal task to bind these; it answers with what the desktop bound (id, its words for the keys).
type Rebind = (Hotkeys, std::sync::mpsc::Sender<Vec<(String, String)>>);

#[derive(Clone, Default)]
enum Backend {
    /// No hotkeys here (no session, or `--window-only`): changes are only saved.
    #[default]
    None,
    X11(Grabs),
    Portal(tokio::sync::mpsc::UnboundedSender<Rebind>),
}

/// Changes the hotkeys while the app runs (Settings → Shortcuts). Cheap to clone, usable from any thread.
#[derive(Clone, Default)]
pub struct Control {
    backend: Backend,
    /// The engine: its messages name the new keys.
    tx: Option<Sender<Msg>>,
    /// The tray: its menu and hover text name the new keys.
    poke: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl Control {
    /// Nothing to grab (`--window-only`).
    pub fn none() -> Control {
        Control::default()
    }

    /// Let go of the keys while the window listens for new ones (X11: the grab would take them first).
    pub fn pause(&self) {
        if let Backend::X11(g) = &self.backend {
            g.release();
        }
    }

    /// Hold `h` again after [`Control::pause`] (nothing changed).
    pub fn resume(&self, h: Hotkeys) {
        if let Backend::X11(g) = &self.backend {
            g.grab(h);
        }
    }

    /// Use `new` instead of `old` (already checked with `Hotkeys::check`). X11: a changed shortcut another app
    /// already has is refused and `old` stays. Blocks (Wayland: until the desktop answers): call it off the
    /// GTK thread. Not refused: the tray and the engine's messages name the new keys from now on.
    pub fn apply(&self, old: Hotkeys, new: Hotkeys) -> Result<Applied, (Which, Refusal)> {
        let applied = match &self.backend {
            Backend::None => Applied::Done,
            Backend::X11(g) => {
                let taken = g.grab(new);
                if let Some(w) = Which::ALL.into_iter().find(|w| new.get(*w) != old.get(*w) && taken.contains(w)) {
                    g.grab(old);
                    return Err((w, Refusal::Taken));
                }
                Applied::Done
            }
            Backend::Portal(rebind) => {
                let (reply, answer) = std::sync::mpsc::channel();
                let bound = match rebind.send((new, reply)) {
                    Ok(()) => answer.recv_timeout(DESKTOP_WAIT).unwrap_or_default(),
                    Err(_) => vec![], // the portal is gone
                };
                Applied::Desktop(
                    Which::ALL
                        .into_iter()
                        .map(|w| {
                            let id = portal_id(w);
                            (w, bound.iter().find(|(b, _)| b == id).map(|(_, t)| t.clone()).unwrap_or_default())
                        })
                        .collect(),
                )
            }
        };
        set_current(new);
        if let Some(tx) = &self.tx {
            let _ = tx.send(Msg::Shortcuts(new));
        }
        if let Some(poke) = &self.poke {
            poke();
        }
        Ok(applied)
    }
}

/// Set up the hotkeys (`h`) for this session. On X11 the key grabs share the watcher thread that also notes
/// app switches (`changed`) and mouse scrolls (`scrolled`). `poke`: the tray shows the keys again.
#[allow(clippy::too_many_arguments)]
pub fn start(
    rt: &Handle,
    session: SessionKind,
    h: Hotkeys,
    tx: Sender<Msg>,
    changed: Arc<Mutex<Option<SystemTime>>>,
    scrolled: Arc<Mutex<Option<SystemTime>>>,
    notifier: Notifier,
    poke: Arc<dyn Fn() + Send + Sync>,
) -> Control {
    let taken = move |keys: Vec<String>| {
        let keys: Vec<&str> = keys.iter().map(String::as_str).collect();
        notifier.toast(&hotkeys_taken_message(&keys));
    };
    let backend = match session {
        SessionKind::X11 => match crate::x11::spawn_watcher(Watch { changed, scrolled, tx: tx.clone() }) {
            Some(grabs) => {
                let missing: Vec<String> = grabs.grab(h).into_iter().map(|w| h.get(w).label()).collect();
                if !missing.is_empty() {
                    taken(missing);
                }
                Backend::X11(grabs)
            }
            None => {
                eprintln!("Context Capsule: cannot open the X display; hotkeys off, use the tray menu");
                Backend::None
            }
        },
        SessionKind::Wayland => {
            let (rebind, requests) = tokio::sync::mpsc::unbounded_channel();
            let to_engine = tx.clone();
            rt.spawn(async move {
                if let Err(e) = portal_shortcuts(to_engine, h, requests, taken).await {
                    eprintln!("Context Capsule: no global hotkeys ({e}); use the tray menu");
                }
            });
            Backend::Portal(rebind)
        }
        SessionKind::Unknown => {
            eprintln!("Context Capsule: no desktop session; hotkeys off, use the tray menu");
            Backend::None
        }
    };
    Control { backend, tx: Some(tx), poke: Some(poke) }
}

/// A new portal session with `h` bound (the desktop may ask the user to confirm). Returns what it bound.
async fn bind(portal: &GlobalShortcuts, h: &Hotkeys) -> Result<(Session<GlobalShortcuts>, Vec<(String, String)>), String> {
    let session = portal.create_session(Default::default()).await.map_err(|e| e.to_string())?;
    let bound = portal
        .bind_shortcuts(&session, &shortcuts(h), None, Default::default())
        .await
        .and_then(|r| r.response())
        .map_err(|e| e.to_string())?;
    let pairs: Vec<(String, String)> =
        bound.shortcuts().iter().map(|s| (s.id().to_string(), s.trigger_description().to_string())).collect();
    for (id, trigger) in &pairs {
        eprintln!("Context Capsule: hotkey {id} = {trigger}");
    }
    Ok((session, pairs))
}

/// Bind through the portal, then forward each press. A change (Settings → Shortcuts) closes the session and binds
/// again in a new one, with the new keys as the preferred triggers; the desktop decides what it binds.
async fn portal_shortcuts(
    tx: Sender<Msg>,
    h: Hotkeys,
    mut requests: tokio::sync::mpsc::UnboundedReceiver<Rebind>,
    taken: impl FnOnce(Vec<String>),
) -> Result<(), String> {
    let portal = GlobalShortcuts::new().await.map_err(|e| e.to_string())?;
    let mut activated = portal.receive_activated().await.map_err(|e| e.to_string())?;
    let (mut session, bound) = bind(&portal, &h).await?;
    let missing = unbound(&bound, &h);
    if !missing.is_empty() {
        eprintln!("Context Capsule: {} not bound (taken by another app?); use the tray menu", missing.join(", "));
        taken(missing);
    }
    let mut bound_keys = h;
    let mut open = true;
    loop {
        tokio::select! {
            a = activated.next() => {
                let Some(a) = a else { break };
                if let Some(msg) = message_for(a.shortcut_id()) {
                    if tx.send(msg).is_err() {
                        break;
                    }
                }
            }
            r = requests.recv(), if open => {
                let Some((new, reply)) = r else {
                    open = false;
                    continue;
                };
                let _ = session.close().await;
                match bind(&portal, &new).await {
                    Ok((s, bound)) => {
                        session = s;
                        bound_keys = new;
                        let _ = reply.send(bound);
                    }
                    Err(e) => {
                        eprintln!("Context Capsule: the desktop didn't take the new hotkeys ({e})");
                        let _ = reply.send(vec![]);
                        // Put the last ones back.
                        session = bind(&portal, &bound_keys).await?.0;
                    }
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(t: &str) -> Shortcut {
        Shortcut::parse(t).unwrap()
    }

    #[test]
    fn shortcut_ids() {
        assert_eq!(message_for(RECORD_ID), Some(Msg::ToggleRecord));
        assert_eq!(message_for(VOICE_ID), Some(Msg::ToggleVoice));
        assert_eq!(message_for("other"), None);
        assert_eq!((portal_id(Which::Record), portal_id(Which::Voice)), (RECORD_ID, VOICE_ID));
        assert_eq!(shortcuts(&Hotkeys::DEFAULT).len(), 2);
    }

    #[test]
    fn portal_triggers() {
        assert_eq!(portal_trigger(&Shortcut::RECORD), "ALT+c");
        assert_eq!(portal_trigger(&Shortcut::VOICE), "ALT+v");
        assert_eq!(portal_trigger(&s("Ctrl+Shift+R")), "CTRL+SHIFT+r");
        assert_eq!(portal_trigger(&s("Ctrl+Alt+Shift+Cmd+K")), "CTRL+ALT+SHIFT+LOGO+k");
        assert_eq!(portal_trigger(&s("Alt+7")), "ALT+7");
        assert_eq!(portal_trigger(&s("F5")), "F5");
        assert_eq!(portal_trigger(&s("Ctrl+F12")), "CTRL+F12");
    }

    #[test]
    fn unbound_keys() {
        let b = |id: &str, t: &str| (id.to_string(), t.to_string());
        let h = Hotkeys::DEFAULT;
        assert!(unbound(&[b(RECORD_ID, "Alt+C"), b(VOICE_ID, "Alt+V")], &h).is_empty());
        assert_eq!(unbound(&[b(RECORD_ID, "Alt+C"), b(VOICE_ID, "")], &h), ["Alt+V"]);
        assert_eq!(unbound(&[b(RECORD_ID, " "), b(VOICE_ID, "")], &h), ["Alt+C", "Alt+V"]);
        let mine = h.with(Which::Record, s("Ctrl+Shift+R"));
        assert_eq!(unbound(&[b(RECORD_ID, "")], &mine), ["Ctrl+Shift+R"], "names the current keys");
    }

    #[test]
    fn current_keys_and_no_backend() {
        let c = Control::none();
        let mine = Hotkeys::DEFAULT.with(Which::Voice, s("Ctrl+Alt+N"));
        assert_eq!(c.apply(Hotkeys::DEFAULT, mine), Ok(Applied::Done));
        assert_eq!(current(), mine, "every place that names a shortcut reads this");
        set_current(Hotkeys::DEFAULT);
    }
}
