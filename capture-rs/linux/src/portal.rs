// context-capsule — linux: the picker = consent. The xdg-desktop-portal ScreenCast dialog (one monitor
// or window, cursor hidden, nothing remembered), its PipeWire stream, and what was picked, placed on screen.
// "Add a screen" opens the dialog again: one portal session and one stream per shared source, up to 4. A
// monitor that's already shared is refused (the dialog can't hide it).

use crate::pipewire::{self, FrameSlot};
use ashpd::desktop::screencast::{CursorMode, Screencast, SelectSourcesOptions, SourceType, Stream};
use ashpd::desktop::{PersistMode, ResponseError};
use context_capsule_core::{Frame, Membership, Msg, PickOutcome, PickedContent, Rect, MAX_SOURCES};
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

/// Is a newly picked source already shared? Monitors are told apart by their place; a window is known
/// only by its size here (two windows can share one), so it is never refused.
pub fn already_shared(new: &Source, shared: &[Source]) -> bool {
    let pick = |s: &Source| match (s.kind, s.position) {
        (SourceKind::Monitor, Some((x, y))) => {
            let size = s.size_rect();
            PickedContent::display(if size.is_empty() { Rect::ZERO } else { Rect::new(x as f64, y as f64, size.width, size.height) })
        }
        (SourceKind::Monitor, None) => PickedContent::display(Rect::ZERO),
        (SourceKind::Window, _) => PickedContent::window(s.size_rect(), Some(vec![])),
    };
    let shared: Vec<PickedContent> = shared.iter().map(pick).collect();
    Membership::already_shared(&pick(new), &shared)
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

/// One shared monitor or window: its own portal session (ended through `cancel`), stream and latest frame.
struct Live {
    id: u32,
    source: Source,
    frames: Arc<FrameSlot>,
    cancel: oneshot::Sender<()>,
    stream: pipewire::Stream,
}

#[derive(Default)]
struct Inner {
    /// Bumped by every stop: outcomes of an older capture are dropped.
    generation: u64,
    /// The dialog is open; it ends with this.
    dialog: Option<oneshot::Sender<()>>,
    sources: Vec<Live>,
    next_id: u32,
}

impl Inner {
    /// Take one source out (it ended on its own) and end its session. None if it was already gone.
    fn remove(&mut self, id: u32) -> Option<Live> {
        let at = self.sources.iter().position(|l| l.id == id)?;
        let live = self.sources.remove(at);
        Some(live)
    }
}

pub struct Capture {
    rt: Handle,
    tx: Sender<Msg>,
    inner: Arc<Mutex<Inner>>,
}

impl Capture {
    pub fn new(rt: Handle, tx: Sender<Msg>) -> Capture {
        Capture { rt, tx, inner: Arc::default() }
    }

    /// Open the portal dialog. Its result comes back as `Msg::Pick`.
    pub fn present(&self) {
        self.open(false);
    }

    /// While capturing: the dialog again, for one more monitor or window (its own session).
    pub fn present_add(&self) {
        self.open(true);
    }

    fn open(&self, adding: bool) {
        let mut inner = self.inner.lock().unwrap();
        if inner.dialog.is_some() {
            eprintln!("Context Capsule: the screen-sharing dialog is already open");
            return;
        }
        let ready = if adding { !inner.sources.is_empty() && inner.sources.len() < MAX_SOURCES } else { inner.sources.is_empty() };
        if !ready {
            return;
        }
        if !adding {
            inner.generation += 1;
        }
        let (cancel, cancelled) = oneshot::channel();
        inner.dialog = Some(cancel);
        inner.next_id += 1;
        let job = Job { generation: inner.generation, id: inner.next_id, adding, inner: self.inner.clone(), tx: self.tx.clone() };
        self.rt.spawn(job.run(cancelled));
    }

    /// Close the dialog and end every capture. Reports nothing.
    pub fn stop(&self) {
        let sources = {
            let mut inner = self.inner.lock().unwrap();
            inner.generation += 1;
            if let Some(c) = inner.dialog.take() {
                let _ = c.send(());
            }
            std::mem::take(&mut inner.sources)
        };
        for live in sources {
            let _ = live.cancel.send(());
            drop(live.stream); // stops its PipeWire thread
        }
    }

    /// Every shared source, with its id.
    pub fn sources(&self) -> Vec<(u32, Source)> {
        self.inner.lock().unwrap().sources.iter().map(|l| (l.id, l.source.clone())).collect()
    }

    /// The latest frame of one source.
    pub fn latest_frame(&self, id: u32) -> Option<Frame> {
        let frames = self.inner.lock().unwrap().sources.iter().find(|l| l.id == id)?.frames.clone();
        frames.latest()
    }
}

struct Job {
    generation: u64,
    /// This source's id.
    id: u32,
    /// Adding to a running recording (outcomes are Added / AddCancelled).
    adding: bool,
    inner: Arc<Mutex<Inner>>,
    tx: Sender<Msg>,
}

impl Job {
    /// The dialog ended without a new source: say so, unless everything was stopped meanwhile.
    fn send(&self, outcome: PickOutcome) {
        let mut inner = self.inner.lock().unwrap();
        if inner.generation != self.generation {
            return;
        }
        inner.dialog = None;
        let outcome = match outcome {
            PickOutcome::Cancelled | PickOutcome::Failed(_) if self.adding => {
                if let PickOutcome::Failed(why) = &outcome {
                    eprintln!("Context Capsule: add a screen failed: {why}");
                }
                PickOutcome::AddCancelled
            }
            other => other,
        };
        let _ = self.tx.send(Msg::Pick(outcome));
    }

    /// This source ended on its own: the others go on; the last one ends the recording.
    fn ended(inner: &Mutex<Inner>, tx: &Sender<Msg>, generation: u64, id: u32, why: Option<String>) {
        let (live, left) = {
            let mut inner = inner.lock().unwrap();
            if inner.generation != generation {
                return;
            }
            let Some(live) = inner.remove(id) else { return };
            (live, inner.sources.len())
        };
        let _ = live.cancel.send(());
        // This may run on the stream's own thread: stop it from another one.
        std::thread::spawn(move || drop(live.stream));
        let _ = tx.send(Msg::Pick(if left > 0 { PickOutcome::Removed(left) } else { PickOutcome::Stopped(why) }));
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
        if self.adding {
            let shared: Vec<Source> = self.inner.lock().unwrap().sources.iter().map(|l| l.source.clone()).collect();
            if already_shared(&source, &shared) {
                let _ = session.close().await;
                return self.send(PickOutcome::AlreadyShared);
            }
        }
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
            let (ended, inner, tx, generation, id) = (ended.clone(), self.inner.clone(), self.tx.clone(), self.generation, self.id);
            Box::new(move |why: Option<String>| {
                if !ended.swap(true, Ordering::SeqCst) {
                    Job::ended(&inner, &tx, generation, id, why);
                }
            })
        };
        let frames = Arc::new(FrameSlot::default());
        let pw = match pipewire::start(fd, node, frames.clone(), on_end) {
            Ok(pw) => pw,
            Err(e) => {
                let _ = session.close().await;
                return self.send(PickOutcome::Failed(format!("capture: {e}")));
            }
        };
        // None: kept. Some: stopped meanwhile, so the stream is dropped (outside the lock).
        let stale = {
            let mut inner = self.inner.lock().unwrap();
            let cancel = if inner.generation == self.generation { inner.dialog.take() } else { None };
            match cancel {
                Some(cancel) => {
                    inner.sources.push(Live { id: self.id, source, frames, cancel, stream: pw });
                    let n = inner.sources.len();
                    let _ = self.tx.send(Msg::Pick(if self.adding { PickOutcome::Added(n) } else { PickOutcome::Picked }));
                    None
                }
                None => Some(pw),
            }
        };
        if let Some(pw) = stale {
            drop(pw);
            let _ = session.close().await;
            return;
        }

        // Until the user stops sharing from the desktop (the session closes) or we stop.
        let closed = session.receive_closed().await;
        match closed {
            Ok(mut closed) => {
                tokio::select! {
                    _ = closed.next() => {
                        if !ended.swap(true, Ordering::SeqCst) {
                            Job::ended(&self.inner, &self.tx, self.generation, self.id, None);
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
    fn already_shared_monitors_are_refused() {
        let left = Source::from_props(Some(SourceType::Monitor), Some((0, 0)), Some((1920, 1080)));
        let right = Source::from_props(Some(SourceType::Monitor), Some((1920, 0)), Some((1920, 1080)));
        let shared = [left.clone()];
        assert!(already_shared(&left, &shared), "the same monitor");
        assert!(!already_shared(&right, &shared), "another monitor of the same size");
        let unplaced = Source::from_props(Some(SourceType::Monitor), None, Some((1920, 1080)));
        assert!(!already_shared(&unplaced, std::slice::from_ref(&unplaced)), "no place: can't tell, allowed");
        let win = Source::from_props(Some(SourceType::Window), None, Some((800, 600)));
        assert!(!already_shared(&win, std::slice::from_ref(&win)), "a window is known by size only: allowed");
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
