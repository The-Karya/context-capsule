// context-capsule — linux: X11 — the real window stack (front to back, with frames, pids and window
// types), the active window, monitors (also through Xwayland), the mouse position, and a watcher thread
// for app switches, mouse scrolls (XInput 2 raw events: no permission, no grab) and the hotkey grabs (your own
// shortcuts, grabbed again when they change).

use crate::apps::ProcApp;
use context_capsule_core::shortcuts::{Hotkeys, Key, Shortcut, Which};
use context_capsule_core::{ListedWindow, Msg, Rect};
use std::collections::HashMap;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;
use x11rb::connection::Connection;
use x11rb::protocol::randr::ConnectionExt as _;
use x11rb::protocol::xinput::{self, ConnectionExt as _};
use x11rb::protocol::xproto::{
    AtomEnum, ChangeWindowAttributesAux, ConnectionExt as _, EventMask, GrabMode, MapState, ModMask, Window,
};
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;

x11rb::atom_manager! {
    pub Atoms: AtomsCookie {
        _NET_ACTIVE_WINDOW,
        _NET_CURRENT_DESKTOP,
        _NET_WM_PID,
        _NET_WM_NAME,
        _NET_WM_WINDOW_TYPE,
        _NET_WM_WINDOW_TYPE_NORMAL,
        _NET_WM_WINDOW_TYPE_DIALOG,
        _NET_WM_WINDOW_TYPE_UTILITY,
        _NET_WM_WINDOW_TYPE_NOTIFICATION,
        _NET_WM_WINDOW_TYPE_DOCK,
        _NET_WM_WINDOW_TYPE_DESKTOP,
        _NET_WM_WINDOW_TYPE_TOOLBAR,
        _NET_WM_WINDOW_TYPE_MENU,
        _NET_WM_WINDOW_TYPE_DROPDOWN_MENU,
        _NET_WM_WINDOW_TYPE_POPUP_MENU,
        _NET_WM_WINDOW_TYPE_TOOLTIP,
        _NET_WM_WINDOW_TYPE_SPLASH,
        _NET_WM_WINDOW_TYPE_COMBO,
        _NET_WM_WINDOW_TYPE_DND,
        _GTK_FRAME_EXTENTS,
        WM_STATE,
        UTF8_STRING,
    }
}

/// An `_NET_WM_WINDOW_TYPE`, as far as we care.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WinType {
    Normal,
    Dialog,
    Utility,
    Notification,
    Panel,
    Popup,
    Other,
}

/// How a window counts for the engine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WinKind {
    /// An app window (layer 0).
    Normal,
    /// A notification bubble: never read, painted out when on top.
    Notification,
    /// Docks, menus, tooltips, splash screens, desktop.
    Other,
}

/// Notification daemons that draw plain override-redirect popups without a window type.
const NOTIFIERS: &[&str] = &[
    "dunst",
    "xfce4-notifyd",
    "notify-osd",
    "mate-notification-daemon",
    "notification-daemon",
    "lxqt-notificationd",
    "deadd-notification-center",
    "swaync",
    "mako",
];

/// The window-type rules. The first type the window lists wins (EWMH: most preferred first).
pub fn kind(types: &[WinType], override_redirect: bool, class: &str) -> WinKind {
    let class = class.to_lowercase();
    if types.contains(&WinType::Notification) || NOTIFIERS.iter().any(|n| class == *n) {
        return WinKind::Notification;
    }
    match types.first() {
        None if override_redirect => WinKind::Other, // menus and tooltips that set nothing
        None | Some(WinType::Normal | WinType::Dialog | WinType::Utility) => WinKind::Normal,
        Some(_) => WinKind::Other,
    }
}

/// A client-side-decorated (GTK) window's visible frame: its geometry minus the shadow margins in
/// `_GTK_FRAME_EXTENTS` (left, right, top, bottom).
pub fn visible_frame(geometry: Rect, gtk_extents: Option<[u32; 4]>) -> Rect {
    let Some([l, r, t, b]) = gtk_extents else { return geometry };
    let (l, r, t, b) = (l as f64, r as f64, t as f64, b as f64);
    if l + r >= geometry.width || t + b >= geometry.height {
        return geometry;
    }
    Rect::new(geometry.x + l, geometry.y + t, geometry.width - l - r, geometry.height - t - b)
}

/// One mapped top-level window.
#[derive(Clone, Debug, PartialEq)]
pub struct XWindow {
    /// The client window (the one with WM_STATE), or the top-level itself.
    pub client: Window,
    pub pid: u32,
    pub class: String,
    pub name: Option<String>,
    pub frame: Rect,
    pub kind: WinKind,
}

/// A query connection to the X server (a real X11 session, or Xwayland for monitors).
pub struct Desktop {
    conn: RustConnection,
    root: Window,
    atoms: Atoms,
}

impl Desktop {
    pub fn connect() -> Option<Desktop> {
        let (conn, screen) = RustConnection::connect(None).ok()?;
        let root = conn.setup().roots.get(screen)?.root;
        let atoms = Atoms::new(&conn).ok()?.reply().ok()?;
        Some(Desktop { conn, root, atoms })
    }

    fn prop(
        &self,
        w: Window,
        atom: u32,
        ty: impl Into<u32>,
        len: u32,
    ) -> Option<x11rb::protocol::xproto::GetPropertyReply> {
        let reply = self.conn.get_property(false, w, atom, ty, 0, len).ok()?.reply().ok()?;
        (reply.format != 0).then_some(reply)
    }

    fn u32s(&self, w: Window, atom: u32, ty: impl Into<u32>) -> Vec<u32> {
        self.prop(w, atom, ty, 64).and_then(|r| r.value32().map(|v| v.collect())).unwrap_or_default()
    }

    /// The mouse position on the root window (global, top-left origin).
    pub fn pointer(&self) -> Option<(f64, f64)> {
        let p = self.conn.query_pointer(self.root).ok()?.reply().ok()?;
        p.same_screen.then_some((p.root_x as f64, p.root_y as f64))
    }

    pub fn active_window(&self) -> Option<Window> {
        self.u32s(self.root, self.atoms._NET_ACTIVE_WINDOW, AtomEnum::WINDOW).first().copied().filter(|w| *w != 0)
    }

    /// The monitors in global coordinates (RandR), else the whole screen.
    pub fn monitors(&self) -> Vec<Rect> {
        if let Ok(Ok(reply)) = self.conn.randr_get_monitors(self.root, true).map(|c| c.reply()) {
            let list: Vec<Rect> = reply
                .monitors
                .iter()
                .map(|m| Rect::new(m.x as f64, m.y as f64, m.width as f64, m.height as f64))
                .filter(|r| !r.is_empty())
                .collect();
            if !list.is_empty() {
                return list;
            }
        }
        let s = &self.conn.setup().roots[0];
        vec![Rect::new(0.0, 0.0, s.width_in_pixels as f64, s.height_in_pixels as f64)]
    }

    fn types(&self, w: Window) -> Vec<WinType> {
        let a = &self.atoms;
        self.u32s(w, a._NET_WM_WINDOW_TYPE, AtomEnum::ATOM)
            .into_iter()
            .map(|t| match t {
                t if t == a._NET_WM_WINDOW_TYPE_NORMAL => WinType::Normal,
                t if t == a._NET_WM_WINDOW_TYPE_DIALOG => WinType::Dialog,
                t if t == a._NET_WM_WINDOW_TYPE_UTILITY => WinType::Utility,
                t if t == a._NET_WM_WINDOW_TYPE_NOTIFICATION => WinType::Notification,
                t if t == a._NET_WM_WINDOW_TYPE_DOCK
                    || t == a._NET_WM_WINDOW_TYPE_DESKTOP
                    || t == a._NET_WM_WINDOW_TYPE_TOOLBAR =>
                {
                    WinType::Panel
                }
                t if t == a._NET_WM_WINDOW_TYPE_MENU
                    || t == a._NET_WM_WINDOW_TYPE_DROPDOWN_MENU
                    || t == a._NET_WM_WINDOW_TYPE_POPUP_MENU
                    || t == a._NET_WM_WINDOW_TYPE_TOOLTIP
                    || t == a._NET_WM_WINDOW_TYPE_COMBO
                    || t == a._NET_WM_WINDOW_TYPE_DND
                    || t == a._NET_WM_WINDOW_TYPE_SPLASH =>
                {
                    WinType::Popup
                }
                _ => WinType::Other,
            })
            .collect()
    }

    fn name(&self, w: Window) -> Option<String> {
        let utf8 = self.prop(w, self.atoms._NET_WM_NAME, self.atoms.UTF8_STRING, 1024);
        let raw = utf8.or_else(|| self.prop(w, AtomEnum::WM_NAME.into(), AtomEnum::ANY, 1024))?;
        let s = String::from_utf8_lossy(&raw.value).trim().to_string();
        (!s.is_empty()).then_some(s)
    }

    fn class(&self, w: Window) -> String {
        // WM_CLASS is "instance\0Class\0": the class names the app.
        let Some(raw) = self.prop(w, AtomEnum::WM_CLASS.into(), AtomEnum::STRING, 256) else { return String::new() };
        let parts: Vec<String> =
            raw.value.split(|b| *b == 0).filter(|p| !p.is_empty()).map(|p| String::from_utf8_lossy(p).into()).collect();
        parts.get(1).or(parts.first()).cloned().unwrap_or_default()
    }

    fn has_wm_state(&self, w: Window) -> bool {
        self.prop(w, self.atoms.WM_STATE, self.atoms.WM_STATE, 2).is_some()
    }

    /// The client inside a window-manager frame (two levels deep at most).
    fn client_of(&self, top: Window) -> Window {
        if self.has_wm_state(top) {
            return top;
        }
        let Ok(Ok(tree)) = self.conn.query_tree(top).map(|c| c.reply()) else { return top };
        for child in &tree.children {
            if self.has_wm_state(*child) {
                return *child;
            }
            if let Ok(Ok(inner)) = self.conn.query_tree(*child).map(|c| c.reply()) {
                if let Some(c) = inner.children.iter().find(|c| self.has_wm_state(**c)) {
                    return *c;
                }
            }
        }
        top
    }

    /// Every mapped top-level window, front to back (override-redirect popups and notifications too).
    pub fn windows(&self) -> Vec<XWindow> {
        let Ok(Ok(tree)) = self.conn.query_tree(self.root).map(|c| c.reply()) else { return vec![] };
        let mut out = vec![];
        for &top in tree.children.iter().rev() {
            let Ok(Ok(attrs)) = self.conn.get_window_attributes(top).map(|c| c.reply()) else { continue };
            if attrs.map_state != MapState::VIEWABLE || attrs.class == x11rb::protocol::xproto::WindowClass::INPUT_ONLY
            {
                continue;
            }
            let Ok(Ok(g)) = self.conn.get_geometry(top).map(|c| c.reply()) else { continue };
            let client = self.client_of(top);
            let geometry = Rect::new(
                g.x as f64,
                g.y as f64,
                g.width as f64 + 2.0 * g.border_width as f64,
                g.height as f64 + 2.0 * g.border_width as f64,
            );
            let gtk = self.u32s(client, self.atoms._GTK_FRAME_EXTENTS, AtomEnum::CARDINAL);
            let gtk = (gtk.len() == 4 && client == top).then(|| [gtk[0], gtk[1], gtk[2], gtk[3]]);
            let class = self.class(client);
            let types = self.types(client);
            out.push(XWindow {
                client,
                pid: self.u32s(client, self.atoms._NET_WM_PID, AtomEnum::CARDINAL).first().copied().unwrap_or(0),
                name: self.name(client),
                kind: kind(&types, attrs.override_redirect, &class),
                class,
                frame: visible_frame(geometry, gtk),
            });
        }
        out
    }
}

/// The engine's window list, with never-record apps marked. App ids come from /proc (cached per pid).
pub fn listed(windows: &[XWindow], is_excluded: &dyn Fn(Option<&str>, &str) -> bool) -> Vec<ListedWindow> {
    let mut apps: HashMap<u32, Option<String>> = HashMap::new();
    windows
        .iter()
        .map(|w| {
            let id = apps.entry(w.pid).or_insert_with(|| ProcApp::of(w.pid).app_id(is_excluded, &w.class)).clone();
            ListedWindow {
                id: w.client as u64,
                pid: w.pid,
                owner: w.class.clone(),
                layer: if w.kind == WinKind::Normal { 0 } else { 1 },
                alpha: 1.0,
                frame: w.frame,
                name: w.name.clone(),
                excluded: is_excluded(id.as_deref(), &w.class),
                notification: w.kind == WinKind::Notification,
            }
        })
        .collect()
}

/// Caps Lock and Num Lock (Mod2): the grab must work with them on too.
const LOCK_MODS: [u16; 4] = [0, 0x2, 0x10, 0x2 | 0x10];
/// The modifiers a shortcut can hold, as X masks: Shift, Control, Mod1 (Alt), Mod4 (Super).
const SHIFT: u16 = 0x1;
const CONTROL: u16 = 0x4;
const ALT: u16 = 0x8;
const SUPER: u16 = 0x40;
const SHORTCUT_MODS: u16 = SHIFT | CONTROL | ALT | SUPER;

/// The X keysym of a shortcut's key: a–z (the unshifted letter), 0–9, F1–F12.
pub fn keysym(key: Key) -> u32 {
    match key {
        Key::Letter(c) => c.to_ascii_lowercase() as u32,
        Key::Digit(d) => 0x30 + d as u32,
        Key::F(n) => 0xffbd + n as u32, // XK_F1 is 0xffbe
    }
}

/// A shortcut's modifiers as an X modifier mask.
pub fn mod_mask(s: &Shortcut) -> u16 {
    [(s.shift, SHIFT), (s.ctrl, CONTROL), (s.alt, ALT), (s.cmd, SUPER)]
        .iter()
        .filter(|(on, _)| *on)
        .fold(0, |m, (_, bit)| m | bit)
}

/// What the watcher thread shares: app and desktop switches and mouse scrolls (the hotkeys: [`Grabs`]).
pub struct Watch {
    pub changed: Arc<Mutex<Option<SystemTime>>>,
    pub scrolled: Arc<Mutex<Option<SystemTime>>>,
    pub tx: Sender<Msg>,
}

/// The hotkeys held on the watcher's connection; grabbed again when they change (Settings → Shortcuts).
#[derive(Clone)]
pub struct Grabs {
    d: Arc<Desktop>,
    held: Arc<Mutex<Held>>,
}

/// Per shortcut (record, voice): the keycode and modifiers grabbed.
type Held = [Option<(u8, u16)>; 2];

fn slot(which: Which) -> usize {
    match which {
        Which::Record => 0,
        Which::Voice => 1,
    }
}

impl Grabs {
    /// Let go of the keys held, then grab `h`'s on the root window (also with Caps Lock / Num Lock on). Returns
    /// the shortcuts another app already has (BadAccess): those are not held.
    pub fn grab(&self, h: Hotkeys) -> Vec<Which> {
        let mut held = self.held.lock().unwrap();
        ungrab_all(&self.d, &mut held);
        let conn = &self.d.conn;
        let setup = conn.setup();
        let (min, max) = (setup.min_keycode, setup.max_keycode);
        let map = conn.get_keyboard_mapping(min, max - min + 1).ok().and_then(|c| c.reply().ok());
        let mut taken = vec![];
        for which in Which::ALL {
            let s = h.get(which);
            let code = map.as_ref().and_then(|m| {
                let per = m.keysyms_per_keycode.max(1) as usize;
                m.keysyms.chunks(per).position(|syms| syms.contains(&keysym(s.key))).map(|i| min + i as u8)
            });
            let Some(code) = code else {
                eprintln!("Context Capsule: no key for {s} on this keyboard; use the tray menu instead");
                continue;
            };
            let mods = mod_mask(&s);
            let mut ok = true;
            for extra in LOCK_MODS {
                let cookie =
                    conn.grab_key(false, self.d.root, ModMask::from(mods | extra), code, GrabMode::ASYNC, GrabMode::ASYNC);
                if cookie.map(|c| c.check().is_err()).unwrap_or(true) {
                    ok = false;
                }
            }
            if ok {
                held[slot(which)] = Some((code, mods));
            } else {
                ungrab(&self.d, code, mods); // the variants that did work
                eprintln!("Context Capsule: {s} is taken by another app; use the tray menu instead");
                taken.push(which);
            }
        }
        let _ = conn.flush();
        taken
    }

    /// Let go of both (while the window listens for new keys).
    pub fn release(&self) {
        ungrab_all(&self.d, &mut self.held.lock().unwrap());
        let _ = self.d.conn.flush();
    }

    /// Which shortcut a key press is (Caps Lock / Num Lock ignored).
    fn pressed(&self, code: u8, state: u16) -> Option<Which> {
        let held = self.held.lock().unwrap();
        Which::ALL.into_iter().find(|w| held[slot(*w)] == Some((code, state & SHORTCUT_MODS)))
    }
}

fn ungrab_all(d: &Desktop, held: &mut Held) {
    for (code, mods) in held.iter_mut().filter_map(Option::take) {
        ungrab(d, code, mods);
    }
}

fn ungrab(d: &Desktop, code: u8, mods: u16) {
    for extra in LOCK_MODS {
        if let Ok(c) = d.conn.ungrab_key(code, d.root, ModMask::from(mods | extra)) {
            let _ = c.check();
        }
    }
}

/// A thread on its own X connection: records app and desktop switches in `changed`, mouse scrolls in
/// `scrolled`, and turns the hotkeys it holds into messages. Returns the hotkeys' grabs (none held yet), or
/// None if X is not there.
pub fn spawn_watcher(w: Watch) -> Option<Grabs> {
    let d = Arc::new(Desktop::connect()?);
    let grabs = Grabs { d: d.clone(), held: Arc::default() };
    let g = grabs.clone();
    std::thread::Builder::new().name("x11-watch".into()).spawn(move || watch(d, w, g)).ok()?;
    Some(grabs)
}

/// Scrolls are buttons 4–7 (an XInput 2.0 client gets wheel and smooth scrolling as those).
pub fn is_scroll(button: u32) -> bool {
    (4..=7).contains(&button)
}

fn watch(d: Arc<Desktop>, w: Watch, grabs: Grabs) {
    let Watch { changed, scrolled, tx } = w;
    let aux = ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE);
    let _ = d.conn.change_window_attributes(d.root, &aux);
    watch_scrolls(&d);
    let _ = d.conn.flush();
    while let Ok(event) = d.conn.wait_for_event() {
        match event {
            Event::PropertyNotify(e)
                if e.atom == d.atoms._NET_ACTIVE_WINDOW || e.atom == d.atoms._NET_CURRENT_DESKTOP =>
            {
                *changed.lock().unwrap() = Some(SystemTime::now());
            }
            Event::XinputRawButtonPress(e) if is_scroll(e.detail) => {
                *scrolled.lock().unwrap() = Some(SystemTime::now());
            }
            Event::KeyPress(e) => {
                let msg = match grabs.pressed(e.detail, u16::from(e.state)) {
                    Some(Which::Record) => Msg::ToggleRecord,
                    Some(Which::Voice) => Msg::ToggleVoice,
                    None => continue,
                };
                if tx.send(msg).is_err() {
                    return;
                }
            }
            _ => {}
        }
    }
}

/// Raw button presses on the root window, for scrolls. Without XInput 2: movement between samples only.
fn watch_scrolls(d: &Desktop) {
    let version = d.conn.xinput_xi_query_version(2, 0).ok().and_then(|c| c.reply().ok());
    if !version.is_some_and(|v| v.major_version >= 2) {
        eprintln!("Context Capsule: no XInput 2; the mouse is noticed between samples only");
        return;
    }
    let mask = xinput::EventMask { deviceid: xinput::Device::ALL_MASTER.into(), mask: vec![xinput::XIEventMask::RAW_BUTTON_PRESS] };
    let _ = d.conn.xinput_xi_select_events(d.root, &[mask]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_type_rules() {
        assert_eq!(kind(&[], false, "Firefox"), WinKind::Normal, "no type: a normal window");
        assert_eq!(kind(&[], true, "Firefox"), WinKind::Other, "override-redirect without a type: a popup");
        assert_eq!(kind(&[WinType::Dialog, WinType::Normal], false, "x"), WinKind::Normal);
        assert_eq!(kind(&[WinType::Notification], true, "x"), WinKind::Notification);
        assert_eq!(
            kind(&[WinType::Popup, WinType::Notification], true, "x"),
            WinKind::Notification,
            "any notification type"
        );
        assert_eq!(kind(&[], true, "Dunst"), WinKind::Notification, "known notifier");
        assert_eq!(kind(&[WinType::Panel], false, "plasmashell"), WinKind::Other);
        assert_eq!(kind(&[WinType::Popup], true, "x"), WinKind::Other);
    }

    #[test]
    fn hotkeys_and_scrolls() {
        let s = |t: &str| Shortcut::parse(t).unwrap();
        assert_eq!(keysym(Key::Letter('C')), 0x63, "XK_c");
        assert_eq!(keysym(Key::Letter('Z')), 0x7a);
        assert_eq!(keysym(Key::Digit(0)), 0x30);
        assert_eq!(keysym(Key::Digit(9)), 0x39);
        assert_eq!(keysym(Key::F(1)), 0xffbe, "XK_F1");
        assert_eq!(keysym(Key::F(12)), 0xffc9, "XK_F12");
        assert_eq!(mod_mask(&Shortcut::RECORD), u16::from(ModMask::M1), "Alt only");
        assert_eq!(mod_mask(&s("Ctrl+Shift+R")), u16::from(ModMask::CONTROL | ModMask::SHIFT));
        assert_eq!(mod_mask(&s("Ctrl+Alt+Shift+Cmd+K")), SHORTCUT_MODS);
        assert_eq!(mod_mask(&s("F5")), 0);
        assert_eq!(SHORTCUT_MODS & LOCK_MODS[3], 0, "Caps Lock and Num Lock are never part of a shortcut");
        assert_eq!(slot(Which::Record), 0);
        assert_eq!(slot(Which::Voice), 1);
        assert!(is_scroll(4) && is_scroll(5) && is_scroll(6) && is_scroll(7));
        assert!(!is_scroll(1) && !is_scroll(3) && !is_scroll(8), "clicks are not scrolls");
    }

    /// Needs an X server (`xvfb-run cargo test -p context-capsule-linux -- --ignored x11_`).
    #[test]
    #[ignore]
    fn x11_keys_another_app_has_are_refused() {
        let grabs = || Grabs { d: Arc::new(Desktop::connect().expect("an X display")), held: Arc::default() };
        let (ours, other) = (grabs(), grabs());
        let mine = Hotkeys::DEFAULT.with(Which::Record, Shortcut::parse("Ctrl+Alt+F7").unwrap());
        assert!(other.grab(mine).is_empty(), "the other app holds Ctrl+Alt+F7 and Alt+V");
        assert_eq!(ours.grab(Hotkeys::DEFAULT), [Which::Voice], "Alt+V is taken; Alt+C is ours");
        let code = ours.held.lock().unwrap()[0].unwrap().0;
        assert_eq!(ours.pressed(code, ALT | 0x10), Some(Which::Record), "Num Lock on");
        assert_eq!(ours.pressed(code, ALT | CONTROL), None, "other modifiers: not ours");
        assert_eq!(ours.grab(mine), [Which::Record, Which::Voice]);
        other.release();
        assert!(ours.grab(mine).is_empty(), "let go of: ours now");
        ours.release();
        assert!(other.grab(Hotkeys::DEFAULT).is_empty(), "ours let go too");
    }

    #[test]
    fn gtk_shadows_trimmed() {
        let g = Rect::new(100.0, 50.0, 800.0, 600.0);
        assert_eq!(visible_frame(g, None), g);
        assert_eq!(visible_frame(g, Some([20, 20, 10, 30])), Rect::new(120.0, 60.0, 760.0, 560.0));
        assert_eq!(visible_frame(g, Some([500, 500, 0, 0])), g, "nonsense extents ignored");
    }

    #[test]
    fn list_marks_private_and_notifications() {
        let w = |client, pid, class: &str, kind| XWindow {
            client,
            pid,
            class: class.into(),
            name: None,
            frame: Rect::new(0.0, 0.0, 100.0, 100.0),
            kind,
        };
        let windows = [
            w(1, 0, "KeePassXC", WinKind::Normal),
            w(2, 0, "Dunst", WinKind::Notification),
            w(3, 0, "Gedit", WinKind::Normal),
        ];
        let is_excluded =
            |id: Option<&str>, name: &str| context_capsule_core::Exclusion::is_excluded(id, Some(name), &[]);
        let list = listed(&windows, &is_excluded);
        assert!(list[0].excluded && list[0].layer == 0);
        assert!(list[1].notification && list[1].layer == 1);
        assert!(!list[2].excluded && !list[2].notification);
        assert_eq!(list[2].id, 3);
    }
}
