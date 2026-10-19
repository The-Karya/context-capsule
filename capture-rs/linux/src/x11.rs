// context-capsule — linux: X11 — the real window stack (front to back, with frames, pids and window
// types), the active window, monitors (also through Xwayland), and a watcher thread for app switches
// and the Ctrl+Alt+Shift+C / V key grabs.

use crate::apps::ProcApp;
use context_capsule_core::{ListedWindow, Msg, Rect};
use std::collections::HashMap;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;
use x11rb::connection::Connection;
use x11rb::protocol::randr::ConnectionExt as _;
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

/// Ctrl+Alt+Shift + a key.
const HOTKEY_MODS: u16 = 0x4 | 0x8 | 0x1; // Control | Mod1 (Alt) | Shift
/// Caps Lock and Num Lock (Mod2): the grab must work with them on too.
const LOCK_MODS: [u16; 4] = [0, 0x2, 0x10, 0x2 | 0x10];
const XK_C: u32 = 0x63;
const XK_V: u32 = 0x76;

/// A thread on its own X connection: records app and desktop switches in `changed`, and (if
/// `grab`) turns Ctrl+Alt+Shift+C / V into messages. Returns false if X is not there.
pub fn spawn_watcher(changed: Arc<Mutex<Option<SystemTime>>>, tx: Sender<Msg>, grab: bool) -> bool {
    let Some(d) = Desktop::connect() else { return false };
    std::thread::Builder::new().name("x11-watch".into()).spawn(move || watch(d, changed, tx, grab)).is_ok()
}

fn watch(d: Desktop, changed: Arc<Mutex<Option<SystemTime>>>, tx: Sender<Msg>, grab: bool) {
    let aux = ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE);
    let _ = d.conn.change_window_attributes(d.root, &aux);
    let keys = if grab { grab_keys(&d) } else { (None, None) };
    let _ = d.conn.flush();
    while let Ok(event) = d.conn.wait_for_event() {
        match event {
            Event::PropertyNotify(e)
                if e.atom == d.atoms._NET_ACTIVE_WINDOW || e.atom == d.atoms._NET_CURRENT_DESKTOP =>
            {
                *changed.lock().unwrap() = Some(SystemTime::now());
            }
            Event::KeyPress(e) => {
                let msg = if Some(e.detail) == keys.0 {
                    Msg::ToggleRecord
                } else if Some(e.detail) == keys.1 {
                    Msg::ToggleVoice
                } else {
                    continue;
                };
                if tx.send(msg).is_err() {
                    return;
                }
            }
            _ => {}
        }
    }
}

/// Grab both hotkeys on the root window (also with Caps Lock / Num Lock on). Logs a taken key.
fn grab_keys(d: &Desktop) -> (Option<u8>, Option<u8>) {
    let setup = d.conn.setup();
    let (min, max) = (setup.min_keycode, setup.max_keycode);
    let Ok(Ok(map)) = d.conn.get_keyboard_mapping(min, max - min + 1).map(|c| c.reply()) else { return (None, None) };
    let per = map.keysyms_per_keycode.max(1) as usize;
    let code_of = |sym: u32| map.keysyms.chunks(per).position(|syms| syms.contains(&sym)).map(|i| min + i as u8);
    let mut grabbed = [None, None];
    for (slot, (sym, label)) in [(XK_C, "Ctrl+Alt+Shift+C"), (XK_V, "Ctrl+Alt+Shift+V")].into_iter().enumerate() {
        let Some(code) = code_of(sym) else { continue };
        let mut ok = true;
        for extra in LOCK_MODS {
            let cookie = d.conn.grab_key(
                false,
                d.root,
                ModMask::from(HOTKEY_MODS | extra),
                code,
                GrabMode::ASYNC,
                GrabMode::ASYNC,
            );
            if cookie.map(|c| c.check().is_err()).unwrap_or(true) {
                ok = false;
            }
        }
        if ok {
            grabbed[slot] = Some(code);
        } else {
            eprintln!("Context Capsule: {label} is taken by another app; use the tray menu instead");
        }
    }
    (grabbed[0], grabbed[1])
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
