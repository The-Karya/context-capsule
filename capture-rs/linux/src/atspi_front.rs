// context-capsule — linux: the front window from AT-SPI — the app whose frame is Active, its name,
// process id, title and extents — and window:activate events for the time of the last app switch.

use atspi::events::window::ActivateEvent;
use atspi::events::{Event, WindowEvents};
use atspi::proxy::accessible::{AccessibleProxy, ObjectRefExt};
use atspi::proxy::component::ComponentProxy;
use atspi::{AccessibilityConnection, CoordType, ObjectRefOwned, Role, State};
use context_capsule_core::Rect;
use futures_util::StreamExt;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;
use zbus::proxy::CacheProperties;

/// The active top-level window, as its app reports it.
#[derive(Clone, Debug)]
pub struct ActiveWindow {
    pub obj: ObjectRefOwned,
    pub app_name: String,
    pub pid: u32,
    pub title: String,
    /// Screen extents. On Wayland apps do not know where they are: only the size means anything.
    pub extents: Option<Rect>,
}

impl ActiveWindow {
    /// A stable id for this window while it lives (bus name + object path).
    pub fn id(&self) -> u64 {
        window_id(&self.obj)
    }
}

pub fn window_id(obj: &ObjectRefOwned) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    obj.name_as_str().unwrap_or("").hash(&mut h);
    obj.path_as_str().hash(&mut h);
    h.finish() | 1 // never 0
}

/// The desktop shell's own surfaces (top bar menus, calendar with notifications): never an app window.
pub fn is_shell(app_name: &str, exe: Option<&str>) -> bool {
    const SHELLS: &[&str] =
        &["gnome-shell", "plasmashell", "kwin_wayland", "kwin_x11", "xfce4-panel", "mutter-x11-frames"];
    let name = app_name.to_lowercase();
    SHELLS.iter().any(|s| name == *s || exe == Some(*s))
}

/// Top-level roles that are windows.
fn is_window_role(role: Role) -> bool {
    matches!(role, Role::Frame | Role::Window | Role::Dialog | Role::FileChooser | Role::Alert)
}

pub struct Accessibility {
    conn: AccessibilityConnection,
    /// The last window that said it became active.
    active: Mutex<Option<ObjectRefOwned>>,
}

impl Accessibility {
    /// Turns accessibility on for the session (so GTK, Qt, Chromium and Electron expose their trees),
    /// connects to the accessibility bus and listens for window switches.
    pub async fn connect(changed: Arc<Mutex<Option<SystemTime>>>) -> Result<Arc<Accessibility>, String> {
        if let Err(e) = atspi::connection::set_session_accessibility(true).await {
            eprintln!("Context Capsule: cannot turn on accessibility for this session: {e}");
        }
        let conn = AccessibilityConnection::new().await.map_err(|e| e.to_string())?;
        if let Err(e) = conn.register_event::<ActivateEvent>().await {
            eprintln!("Context Capsule: no window:activate events: {e}");
        }
        let me = Arc::new(Accessibility { conn, active: Mutex::new(None) });
        let listener = me.clone();
        tokio::spawn(async move {
            let events = listener.conn.event_stream();
            futures_util::pin_mut!(events);
            while let Some(event) = events.next().await {
                if let Ok(Event::Window(WindowEvents::Activate(e))) = event {
                    *listener.active.lock().unwrap() = Some(e.item);
                    *changed.lock().unwrap() = Some(SystemTime::now());
                }
            }
        });
        Ok(me)
    }

    pub fn connection(&self) -> &zbus::Connection {
        self.conn.connection()
    }

    async fn proxy<'a>(&'a self, obj: &'a ObjectRefOwned) -> Option<AccessibleProxy<'a>> {
        obj.as_accessible_proxy(self.connection()).await.ok()
    }

    async fn is_active(&self, obj: &ObjectRefOwned) -> bool {
        match self.proxy(obj).await {
            Some(p) => p.get_state().await.is_ok_and(|s| s.contains(State::Active)),
            None => false,
        }
    }

    /// Every top-level window of every app, with its role and state.
    async fn top_levels(&self) -> Vec<(ObjectRefOwned, atspi::StateSet)> {
        let Ok(root) = self.conn.root_accessible_on_registry().await else { return vec![] };
        let apps = root.get_children().await.unwrap_or_default();
        let per_app = futures_util::future::join_all(apps.iter().map(|app| async move {
            let Some(p) = self.proxy(app).await else { return vec![] };
            let windows = p.get_children().await.unwrap_or_default();
            let mut out = vec![];
            for w in windows {
                let Some(wp) = self.proxy(&w).await else { continue };
                let (role, state) = futures_util::join!(wp.get_role(), wp.get_state());
                if let (Ok(role), Ok(state)) = (role, state) {
                    if is_window_role(role) {
                        out.push((w.clone(), state));
                    }
                }
            }
            out
        }))
        .await;
        per_app.into_iter().flatten().collect()
    }

    /// The active window: the last one that said so, if it still is; else a scan of every app.
    pub async fn active_window(&self) -> Option<ActiveWindow> {
        let cached = self.active.lock().unwrap().clone();
        let obj = match cached {
            Some(obj) if self.is_active(&obj).await => obj,
            _ => {
                let found =
                    self.top_levels().await.into_iter().find(|(_, s)| s.contains(State::Active)).map(|(o, _)| o)?;
                *self.active.lock().unwrap() = Some(found.clone());
                found
            }
        };
        self.describe(obj).await
    }

    async fn describe(&self, obj: ObjectRefOwned) -> Option<ActiveWindow> {
        let p = self.proxy(&obj).await?;
        let (title, app) = futures_util::join!(p.name(), p.get_application());
        let app_name = match app {
            Ok(app) => match self.proxy(&app).await {
                Some(ap) => ap.name().await.unwrap_or_default(),
                None => String::new(),
            },
            Err(_) => String::new(),
        };
        let pid = self.pid(&obj).await.unwrap_or(0);
        let extents = self.extents(&obj).await;
        Some(ActiveWindow { obj, app_name, pid, title: title.unwrap_or_default(), extents })
    }

    /// The process behind an accessible: its connection's pid on the accessibility bus.
    async fn pid(&self, obj: &ObjectRefOwned) -> Option<u32> {
        let name = obj.name()?.clone();
        let dbus = zbus::fdo::DBusProxy::new(self.connection()).await.ok()?;
        dbus.get_connection_unix_process_id(name.into()).await.ok()
    }

    pub async fn extents(&self, obj: &ObjectRefOwned) -> Option<Rect> {
        let c = ComponentProxy::builder(self.connection())
            .destination(obj.name()?.clone())
            .ok()?
            .path(obj.path().clone())
            .ok()?
            .cache_properties(CacheProperties::No)
            .build()
            .await
            .ok()?;
        let (x, y, w, h) = c.get_extents(CoordType::Screen).await.ok()?;
        (w > 0 && h > 0).then(|| Rect::new(x as f64, y as f64, w as f64, h as f64))
    }

    /// Sizes of every on-screen top-level window (to match a picked window by size).
    pub async fn window_sizes(&self) -> Vec<(u64, Rect)> {
        let mut out = vec![];
        for (obj, state) in self.top_levels().await {
            if !state.contains(State::Showing) || state.contains(State::Iconified) {
                continue;
            }
            if let Some(r) = self.extents(&obj).await {
                out.push((window_id(&obj), r));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shells_are_not_apps() {
        assert!(is_shell("gnome-shell", None));
        assert!(is_shell("", Some("plasmashell")));
        assert!(!is_shell("Firefox", Some("firefox")));
    }

    #[test]
    fn ids_are_stable_and_distinct() {
        let a = atspi::ObjectRef::new_owned(
            zbus::names::UniqueName::try_from(":1.42").unwrap(),
            zbus::zvariant::ObjectPath::try_from("/org/a11y/atspi/accessible/1").unwrap(),
        );
        let b = atspi::ObjectRef::new_owned(
            zbus::names::UniqueName::try_from(":1.42").unwrap(),
            zbus::zvariant::ObjectPath::try_from("/org/a11y/atspi/accessible/2").unwrap(),
        );
        assert_eq!(window_id(&a), window_id(&a.clone()));
        assert_ne!(window_id(&a), window_id(&b));
        assert_ne!(window_id(&a), 0);
    }
}
