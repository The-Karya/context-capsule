// context-capsule — linux: the picker = consent. The xdg-desktop-portal ScreenCast dialog (one monitor
// or window, cursor hidden, nothing remembered), its PipeWire stream, and what was picked, placed on screen.

use crate::pipewire::{self, FrameSlot};
use ashpd::desktop::screencast::{CursorMode, Screencast, SelectSourcesOptions, SourceType, Stream};
use ashpd::desktop::{PersistMode, ResponseError};
use context_capsule_core::{Membership, Msg, PickOutcome, PickedContent, Rect};
use futures_util::StreamExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use tokio::runtime::Handle;
use tokio::sync::oneshot;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceKind {
    Monitor,
    Window,
}

/// What the portal says about the picked stream (logical pixels, global coordinates).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source {
    pub kind: SourceKind,
    pub position: Option<(i32, i32)>,
    pub size: Option<(i32, i32)>,
}

impl Source {
    pub fn of(stream: &Stream) -> Source {
        Source::from_props(stream.source_type(), stream.position(), stream.size())
    }

    /// No type (old portals): a stream with a position is a monitor; else a window.
    pub fn from_props(kind: Option<SourceType>, position: Option<(i32, i32)>, size: Option<(i32, i32)>) -> Source {
        let kind = match kind {
            Some(SourceType::Window) => SourceKind::Window,
            Some(_) => SourceKind::Monitor,
            None if position.is_some() => SourceKind::Monitor,
            None => SourceKind::Window,
        };
        let size = size.filter(|(w, h)| *w > 0 && *h > 0);
        Source { kind, position, size }
    }

    /// The size as a rect at the origin.
    pub fn size_rect(&self) -> Rect {
        self.size.map_or(Rect::ZERO, |(w, h)| Rect::new(0.0, 0.0, w as f64, h as f64))
    }
}

/// A picked monitor's global rect: position + size, else the one known monitor of that size.
/// Unknown → empty (the engine then skips: "cannot find the picked screen").
/// (Not `Membership::resolve_display`: it takes an exact match at the origin first, which for a
/// size-only rect would pick the origin monitor when two monitors have the same size.)
pub fn monitor_rect(src: &Source, monitors: &[Rect]) -> Rect {
    let size = src.size_rect();
    if size.is_empty() {
        return Rect::ZERO;
    }
    if let Some((x, y)) = src.position {
        return Rect::new(x as f64, y as f64, size.width, size.height);
    }
    let t = Membership::TOLERANCE;
    let mut same = monitors.iter().filter(|m| (m.width - size.width).abs() <= t && (m.height - size.height).abs() <= t);
    match (same.next(), same.next()) {
        (Some(one), None) => *one,
        _ => Rect::ZERO,
    }
}

/// Which window was picked: the one window whose size matches the stream's. Several (two maximized
/// windows) or none → None: fail closed.
pub fn match_window(size: Rect, candidates: &[(u64, Rect)]) -> Option<u64> {
    if size.is_empty() {
        return None;
    }
    let t = Membership::TOLERANCE;
    let mut hits =
        candidates.iter().filter(|(_, r)| (r.width - size.width).abs() <= t && (r.height - size.height).abs() <= t);
    let first = hits.next()?;
    hits.next().is_none().then_some(first.0)
}

/// The engine's view of the pick. A window is known by the id `match_window` found (none: nothing matches).
pub fn picked_content(src: &Source, monitors: &[Rect], matched: Option<u64>) -> PickedContent {
    match src.kind {
        SourceKind::Monitor => PickedContent::display(monitor_rect(src, monitors)),
        SourceKind::Window => PickedContent::window(src.size_rect(), Some(matched.into_iter().collect())),
    }
}

/// The user-facing reason a portal call failed.
pub fn picker_error(e: &ashpd::Error) -> String {
    let text = e.to_string();
    if text.contains("org.freedesktop.portal.Desktop") || text.contains("ServiceUnknown") {
        return "picker: xdg-desktop-portal is not running (install xdg-desktop-portal and its GNOME or KDE backend)"
            .into();
    }
    match e {
        ashpd::Error::PortalNotFound(_) => {
            "picker: the screen-sharing portal is not available (install xdg-desktop-portal with the GNOME or KDE backend)".into()
        }
        e => format!("picker: {e}"),
    }
}

#[derive(Default)]
struct Inner {
    /// Bumped by every start and stop: outcomes of an older capture are dropped.
    generation: u64,
    picking: bool,
    source: Option<Source>,
    /// Ends the dialog or the running capture.
    cancel: Option<oneshot::Sender<()>>,
    stream: Option<pipewire::Stream>,
}

pub struct Capture {
    rt: Handle,
    tx: Sender<Msg>,
    inner: Arc<Mutex<Inner>>,
    pub frames: Arc<FrameSlot>,
}

impl Capture {
    pub fn new(rt: Handle, tx: Sender<Msg>) -> Capture {
        Capture { rt, tx, inner: Arc::default(), frames: Arc::new(FrameSlot::default()) }
    }

    /// Open the portal dialog. Its result comes back as `Msg::Pick`.
    pub fn present(&self) {
        let mut inner = self.inner.lock().unwrap();
        if inner.picking {
            eprintln!("Context Capsule: the screen-sharing dialog is already open");
            return;
        }
        inner.generation += 1;
        inner.picking = true;
        let (cancel, cancelled) = oneshot::channel();
        inner.cancel = Some(cancel);
        let job = Job {
            generation: inner.generation,
            inner: self.inner.clone(),
            frames: self.frames.clone(),
            tx: self.tx.clone(),
        };
        self.rt.spawn(job.run(cancelled));
    }

    /// Close the dialog or end the capture. Reports nothing.
    pub fn stop(&self) {
        let stream = {
            let mut inner = self.inner.lock().unwrap();
            inner.generation += 1;
            inner.picking = false;
            inner.source = None;
            if let Some(c) = inner.cancel.take() {
                let _ = c.send(());
            }
            inner.stream.take()
        };
        drop(stream); // stops the PipeWire thread
        self.frames.clear();
    }

    pub fn source(&self) -> Option<Source> {
        self.inner.lock().unwrap().source.clone()
    }
}

struct Job {
    generation: u64,
    inner: Arc<Mutex<Inner>>,
    frames: Arc<FrameSlot>,
    tx: Sender<Msg>,
}

impl Job {
    /// Hand an outcome to the engine, unless this capture was stopped meanwhile.
    fn send(&self, outcome: PickOutcome) {
        let mut inner = self.inner.lock().unwrap();
        if inner.generation != self.generation {
            return;
        }
        inner.picking = false;
        if !matches!(outcome, PickOutcome::Picked) {
            inner.source = None;
            inner.cancel = None;
        }
        let _ = self.tx.send(Msg::Pick(outcome));
    }

    async fn run(self, mut cancelled: oneshot::Receiver<()>) {
        let proxy = match Screencast::new().await {
            Ok(p) => p,
            Err(e) => return self.send(PickOutcome::Failed(picker_error(&e))),
        };
        let session = match proxy.create_session(Default::default()).await {
            Ok(s) => s,
            Err(e) => return self.send(PickOutcome::Failed(picker_error(&e))),
        };
        let wanted = SourceType::Monitor | SourceType::Window;
        let types =
            proxy.available_source_types().await.map(|t| t & wanted).ok().filter(|t| !t.is_empty()).unwrap_or(wanted);
        let mut options =
            SelectSourcesOptions::default().set_sources(types).set_multiple(false).set_persist_mode(PersistMode::DoNot);
        if proxy.available_cursor_modes().await.is_ok_and(|m| m.contains(CursorMode::Hidden)) {
            options = options.set_cursor_mode(CursorMode::Hidden);
        }
        if let Err(e) = proxy.select_sources(&session, options).await.and_then(|r| r.response()) {
            let _ = session.close().await;
            return self.send(PickOutcome::Failed(picker_error(&e)));
        }
        let started = tokio::select! {
            r = proxy.start(&session, None, Default::default()) => r.and_then(|r| r.response()),
            _ = &mut cancelled => {
                let _ = session.close().await; // the user pressed the hotkey again: close the dialog
                return;
            }
        };
        let streams = match started {
            Ok(s) => s,
            Err(ashpd::Error::Response(ResponseError::Cancelled)) => return self.send(PickOutcome::Cancelled),
            Err(e) => return self.send(PickOutcome::Failed(picker_error(&e))),
        };
        let Some(stream) = streams.streams().first() else {
            let _ = session.close().await;
            return self.send(PickOutcome::Failed("picker: nothing was shared".into()));
        };
        let source = Source::of(stream);
        let node = stream.pipe_wire_node_id();
        let fd = match proxy.open_pipe_wire_remote(&session, Default::default()).await {
            Ok(fd) => fd,
            Err(e) => {
                let _ = session.close().await;
                return self.send(PickOutcome::Failed(format!("capture: {e}")));
            }
        };

        // The capture can end two ways (the stream, or the session); report it once.
        let ended = Arc::new(AtomicBool::new(false));
        let on_end = {
            let (ended, inner, tx, generation) = (ended.clone(), self.inner.clone(), self.tx.clone(), self.generation);
            Box::new(move |why: Option<String>| {
                if !ended.swap(true, Ordering::SeqCst) && inner.lock().unwrap().generation == generation {
                    let _ = tx.send(Msg::Pick(PickOutcome::Stopped(why)));
                }
            })
        };
        let pw = match pipewire::start(fd, node, self.frames.clone(), on_end) {
            Ok(pw) => pw,
            Err(e) => {
                let _ = session.close().await;
                return self.send(PickOutcome::Failed(format!("capture: {e}")));
            }
        };
        let stale = {
            let mut inner = self.inner.lock().unwrap();
            let stale = inner.generation != self.generation;
            if !stale {
                inner.source = Some(source);
                inner.stream = Some(pw);
            }
            stale
        };
        if stale {
            let _ = session.close().await; // stopped meanwhile; `pw` stops when it drops on return
            return;
        }
        self.send(PickOutcome::Picked);

        // Until the user stops sharing from the desktop (the session closes) or we stop.
        let closed = session.receive_closed().await;
        match closed {
            Ok(mut closed) => {
                tokio::select! {
                    _ = closed.next() => {
                        if !ended.swap(true, Ordering::SeqCst) {
                            self.send(PickOutcome::Stopped(None));
                        }
                    }
                    _ = &mut cancelled => {}
                }
            }
            Err(_) => {
                let _ = (&mut cancelled).await;
            }
        }
        let _ = session.close().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use context_capsule_core::{ListedWindow, OcrDecision, PickStyle};

    #[test]
    fn stream_properties() {
        let m = Source::from_props(Some(SourceType::Monitor), Some((1920, 0)), Some((2560, 1440)));
        assert_eq!(m.kind, SourceKind::Monitor);
        assert_eq!(monitor_rect(&m, &[]), Rect::new(1920.0, 0.0, 2560.0, 1440.0));
        let w = Source::from_props(Some(SourceType::Window), None, Some((800, 600)));
        assert_eq!(w.kind, SourceKind::Window);
        assert_eq!(Source::from_props(None, Some((0, 0)), Some((10, 10))).kind, SourceKind::Monitor);
        assert_eq!(Source::from_props(None, None, Some((10, 10))).kind, SourceKind::Window);
        assert_eq!(Source::from_props(Some(SourceType::Virtual), None, None).kind, SourceKind::Monitor);
    }

    #[test]
    fn monitor_without_position() {
        let monitors = [Rect::new(0.0, 0.0, 1920.0, 1080.0), Rect::new(1920.0, 0.0, 2560.0, 1440.0)];
        let m = Source::from_props(Some(SourceType::Monitor), None, Some((2560, 1440)));
        assert_eq!(monitor_rect(&m, &monitors), monitors[1], "matched by size");
        let twins = [Rect::new(0.0, 0.0, 1920.0, 1080.0), Rect::new(1920.0, 0.0, 1920.0, 1080.0)];
        let t = Source::from_props(Some(SourceType::Monitor), None, Some((1920, 1080)));
        assert!(monitor_rect(&t, &twins).is_empty(), "two of that size: unknown");
        assert!(monitor_rect(&Source::from_props(Some(SourceType::Monitor), Some((0, 0)), None), &monitors).is_empty());
        let pick = picked_content(&t, &twins, None);
        assert_eq!(pick.style, PickStyle::Display);
        assert!(pick.rect.is_empty(), "the engine skips with unknown_display");
    }

    #[test]
    fn window_matched_by_size_only_when_unique() {
        let size = Rect::new(0.0, 0.0, 800.0, 600.0);
        let c = [(1, Rect::new(10.0, 10.0, 1280.0, 720.0)), (2, Rect::new(0.0, 0.0, 804.0, 597.0))];
        assert_eq!(match_window(size, &c), Some(2));
        let twins = [(1, Rect::new(0.0, 0.0, 800.0, 600.0)), (2, Rect::new(50.0, 50.0, 800.0, 600.0))];
        assert_eq!(match_window(size, &twins), None, "two of that size: fail closed");
        assert_eq!(match_window(size, &[]), None);
        assert_eq!(match_window(Rect::ZERO, &c), None);
    }

    #[test]
    fn picked_window_membership() {
        let src = Source::from_props(Some(SourceType::Window), None, Some((800, 600)));
        let pick = picked_content(&src, &[], Some(7));
        let front = ListedWindow::new(7, 1, Rect::new(300.0, 200.0, 800.0, 600.0));
        assert!(Membership::contains(&pick, &front));
        assert!(!Membership::contains(&pick, &ListedWindow::new(8, 1, front.frame)), "same size, other window");
        assert_eq!(
            Membership::ocr_decision(&pick, &front, std::slice::from_ref(&front)),
            OcrDecision::Read { region: Rect::UNIT, masks: vec![] }
        );
        let unmatched = picked_content(&src, &[], None);
        assert!(!Membership::contains(&unmatched, &front), "no match: nothing is read");
    }
}
