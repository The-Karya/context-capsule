// context-capsule — linux: the line from the tray and the engine's thread to the window (which lives on the
// GTK thread): open it, show the latest status, quit. Without the window (no display, or built without the
// "window" feature) the link is off and every call does nothing.
#![cfg_attr(not(feature = "window"), allow(dead_code))]

use context_capsule_core::window::Section;
use context_capsule_core::Status;
use std::sync::Arc;

/// What the window is told.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiMsg {
    /// Show the window and bring it to the front (on this section, else where it was).
    Open(Option<Section>),
    /// The engine's status changed.
    Status(Box<Status>),
    /// The engine stopped: close everything.
    Quit,
}

/// Cheap to clone, safe to use from any thread.
#[derive(Clone, Default)]
pub struct WindowLink {
    send: Option<Arc<dyn Fn(UiMsg) + Send + Sync>>,
}

impl WindowLink {
    /// No window.
    pub fn off() -> WindowLink {
        WindowLink { send: None }
    }

    pub fn new(send: impl Fn(UiMsg) + Send + Sync + 'static) -> WindowLink {
        WindowLink { send: Some(Arc::new(send)) }
    }

    /// There is a window to open (the tray shows "Open Context Capsule").
    pub fn is_on(&self) -> bool {
        self.send.is_some()
    }

    pub fn open(&self, section: Option<Section>) {
        self.send(UiMsg::Open(section));
    }

    pub fn status(&self, status: &Status) {
        self.send(UiMsg::Status(Box::new(status.clone())));
    }

    pub fn quit(&self) {
        self.send(UiMsg::Quit);
    }

    fn send(&self, msg: UiMsg) {
        if let Some(send) = &self.send {
            send(msg);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn off_does_nothing_on_forwards() {
        let off = WindowLink::off();
        assert!(!off.is_on());
        off.open(None);
        let got = Arc::new(Mutex::new(vec![]));
        let sink = got.clone();
        let on = WindowLink::new(move |m| sink.lock().unwrap().push(m));
        assert!(on.is_on());
        on.open(Some(Section::Settings));
        on.status(&Status { recording: true, ..Default::default() });
        on.quit();
        let got = got.lock().unwrap();
        assert_eq!(got[0], UiMsg::Open(Some(Section::Settings)));
        assert!(matches!(&got[1], UiMsg::Status(s) if s.recording));
        assert_eq!(got[2], UiMsg::Quit);
    }
}
