// context-capsule — linux: the shared window (ui/ at the repo root, the same page as on the Mac and Windows) in
// WebKitGTK 6.0. The default window; the GTK one only with `--old-window` or config.json "new_window": false.
//! The page comes from inside the program (`capsule://app/…`, never the network); a script put in before it loads
//! gives it `window.capsuleHost`. Its calls run on worker threads (`bridge::handle`); what needs GTK (dialogs, the
//! clipboard, opening things) is handed to GTK's thread and waited for. Events go back with `evaluate_javascript`.
//! The window's buttons sit over the page's top band, which also moves the window.

use super::{apply_appearance, Ctx};
use crate::{autostart, connect, icon_match, ui};
use adw::prelude::*;
use context_capsule_core::anyai::Launch;
use context_capsule_core::app_icons;
use context_capsule_core::bridge::{self, Hello, Host, Transport};
use context_capsule_core::claude::new_chat_url;
use context_capsule_core::settings::Appearance;
use context_capsule_core::shortcuts::Refusal;
use context_capsule_core::window::{self as words, HomeNumbers, Keys, Section};
use context_capsule_core::{Hotkeys, Msg, Paths, Status};
use gtk::{gdk, gio, glib};
use serde_json::{json, Map, Value};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};
use super::wk::{self, ORIGIN};

/// The page's top band (`#toolbar` in ui/css/app.css): the window moves by it.
const BAND: i32 = 52;
/// Files are looked at this often while the window shows; recordings are said to change at most every `SESSIONS_EVERY`.
const WATCH_EVERY: u32 = 2;
const SESSIONS_EVERY: Duration = Duration::from_secs(4);

thread_local! {
    static WEB: RefCell<Option<Rc<WebWindow>>> = const { RefCell::new(None) };
}

/// The shared window.
pub struct WebWindow {
    pub ctx: Rc<Ctx>,
    view: gtk::Widget,
    host: Arc<LinuxHost>,
    styles: wk::Content,
    /// The page asked `host.hello` (events before that would be lost).
    ready: Cell<bool>,
    watch: RefCell<Stamps>,
    sessions_told: Cell<Option<Instant>>,
    sessions_due: Cell<bool>,
}

fn env(k: &str) -> Option<String> {
    std::env::var(k).ok()
}

impl WebWindow {
    pub fn get() -> Option<Rc<WebWindow>> {
        WEB.with(|w| w.borrow().clone())
    }

    /// Build the window (not shown), on `section`.
    pub fn build(
        app: &adw::Application,
        paths: &Paths,
        tx: &Sender<Msg>,
        locale: &str,
        hotkeys: &crate::hotkeys::Control,
        status: Status,
        section: Section,
    ) -> Rc<WebWindow> {
        let win = adw::ApplicationWindow::builder()
            .application(app)
            .title(words::APP_NAME)
            .default_width(words::START_WIDTH as i32)
            .default_height(words::START_HEIGHT as i32)
            .hide_on_close(true)
            .build();
        win.set_size_request(words::MIN_WIDTH as i32, words::MIN_HEIGHT as i32);
        if let Some((width, height)) = env("CONTEXT_CAPSULE_WINDOW_SIZE").as_deref().and_then(ui::parse_size) {
            win.set_default_size(width, height);
        }
        let toasts = adw::ToastOverlay::new();
        let ctx = Rc::new(Ctx {
            paths: paths.clone(),
            tx: tx.clone(),
            locale: locale.to_string(),
            keys: RefCell::new(Keys::linux(&crate::hotkeys::current())),
            hotkeys: hotkeys.clone(),
            status: RefCell::new(status),
            numbers: RefCell::new(HomeNumbers::default()),
            win: win.clone(),
            toasts: toasts.clone(),
            nav: RefCell::new(None),
            timeline_today: Cell::new(false),
        });

        let host = Arc::new(LinuxHost {
            paths: paths.clone(),
            tx: Mutex::new(tx.clone()),
            hotkeys: hotkeys.clone(),
            locale: locale.to_string(),
            page: Mutex::new(Some(section.id().to_string())),
        });

        // The page's files, from inside the program; nothing else loads.
        let content = wk::Content::new();
        content.add_script(&bridge::shim_js(Transport::WebKit));
        let calls = host.clone();
        content.on_message("capsule", move |text| {
            let (host, paths) = (calls.clone(), calls.paths.clone());
            std::thread::spawn(move || {
                if let Some(answer) = bridge::handle(&paths, &*host, &text) {
                    glib::MainContext::default().invoke(move || {
                        if let Some(w) = WebWindow::get() {
                            w.run_js(&format!("capsuleHost._receive({answer})"));
                        }
                    });
                }
            });
        });
        let devtools = env("CONTEXT_CAPSULE_DEVTOOLS").is_some();
        let view = wk::web_view(&wk::context(), &content, &wk::settings(devtools));
        view.set_hexpand(true);
        view.set_vexpand(true);
        wk::on_navigation(&view, |uri, new_window| {
            if !new_window && uri.starts_with(ORIGIN) {
                return true;
            }
            // A link out of the page: the browser, never here.
            if uri.starts_with("https://") || uri.starts_with("http://") {
                if let Some(w) = WebWindow::get() {
                    w.ctx.open_uri(uri);
                }
            }
            false
        });
        // The page's own menus: no Reload / Back / Inspect (text fields keep Cut, Copy, Paste).
        wk::on_context_menu(&view, move |in_text| in_text || devtools);

        // The window's buttons over the page's top band, which moves the window.
        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(&view));
        let handle = gtk::WindowHandle::new();
        handle.set_valign(gtk::Align::Start);
        handle.set_height_request(BAND);
        overlay.add_overlay(&handle);
        let controls = gtk::WindowControls::new(gtk::PackType::End);
        controls.set_halign(gtk::Align::End);
        controls.set_valign(gtk::Align::Start);
        controls.set_margin_top(10);
        controls.set_margin_end(10);
        overlay.add_overlay(&controls);
        toasts.set_child(Some(&overlay));
        win.set_content(Some(&toasts));

        let w = Rc::new(WebWindow {
            ctx,
            view: view.clone(),
            host,
            styles: content,
            ready: Cell::new(false),
            watch: RefCell::new(Stamps::default()),
            sessions_told: Cell::new(None),
            sessions_due: Cell::new(false),
        });
        WEB.with(|cell| *cell.borrow_mut() = Some(w.clone()));
        // The page's pill moves left of the window's buttons (as many as this desktop shows).
        let weak = Rc::downgrade(&w);
        controls.connect_notify_local(Some("empty"), move |c, _| {
            if let Some(w) = weak.upgrade() {
                w.make_room(c);
            }
        });
        let weak = Rc::downgrade(&w);
        win.connect_map(move |_| {
            let weak = weak.clone();
            glib::idle_add_local_once(move || {
                if let Some(w) = weak.upgrade() {
                    w.make_room_now();
                }
            });
        });
        // light / dark from the system (the page puts config "appearance" over it).
        let style = adw::StyleManager::default();
        let weak = Rc::downgrade(&w);
        style.connect_dark_notify(move |s| {
            if let Some(w) = weak.upgrade() {
                w.paint_behind(s.is_dark());
                if Appearance::load(&w.ctx.paths.config()) == Appearance::System {
                    w.emit("appearance", json!({ "dark": s.is_dark() }));
                }
            }
        });
        w.paint_behind(style.is_dark());
        w.ctx.win.connect_is_active_notify(|win| {
            // Coming back to the window: what changed meanwhile shows at once.
            if win.is_active() {
                if let Some(w) = WebWindow::get() {
                    w.look_at_files();
                }
            }
        });
        w.watch_files();
        wk::load_uri(&view, &format!("{ORIGIN}index.html"));
        w
    }

    /// Room for the window's buttons on the right of the page's top band.
    fn make_room_now(&self) {
        let mut child = self.ctx.win.content().and_then(|c| c.first_child());
        while let Some(c) = child {
            if let Some(o) = c.downcast_ref::<gtk::Overlay>() {
                let mut kid = o.first_child();
                while let Some(k) = kid {
                    if let Some(controls) = k.downcast_ref::<gtk::WindowControls>() {
                        return self.make_room(controls);
                    }
                    kid = k.next_sibling();
                }
            }
            child = c.next_sibling();
        }
    }

    fn make_room(&self, controls: &gtk::WindowControls) {
        let (_, natural, _, _) = controls.measure(gtk::Orientation::Horizontal, -1);
        let room = if controls.is_empty() { 0 } else { natural + 10 };
        self.styles.set_style(&format!("#pill {{ right: {}px !important; }}", room + 9));
    }

    /// The colour behind the page while it loads (no white flash in dark).
    fn paint_behind(&self, dark: bool) {
        let dark = match Appearance::load(&self.ctx.paths.config()) {
            Appearance::Dark => true,
            Appearance::Light => false,
            Appearance::System => dark,
        };
        let c = if dark { gdk::RGBA::new(0.118, 0.118, 0.118, 1.0) } else { gdk::RGBA::new(1.0, 1.0, 1.0, 1.0) };
        wk::set_background(&self.view, &c);
    }

    fn run_js(&self, script: &str) {
        wk::run_js(&self.view, script);
    }

    /// An event for the page (GTK's thread).
    pub fn emit(&self, event: &str, payload: Value) {
        if self.ready.get() || event == "state" {
            self.run_js(&format!("window.capsuleHost && capsuleHost._receive({})", bridge::event_json(event, payload)));
        }
    }

    /// Show and bring to the front; on `section` (if the page is up; else it opens there).
    pub fn open(&self, section: Option<Section>) {
        if let Some(s) = section {
            if self.ready.get() {
                self.emit("navigate", json!({ "page": s.id() }));
            } else {
                *self.host.page.lock().unwrap() = Some(s.id().to_string());
            }
        }
        self.ctx.win.present();
        self.look_at_files();
    }

    pub fn set_status(&self, status: Status) {
        let was = std::mem::replace(&mut *self.ctx.status.borrow_mut(), status);
        if was.projects != self.ctx.status.borrow().projects {
            self.emit("data-changed", json!({ "what": ["projects"] }));
        }
        self.emit_state();
    }

    fn emit_state(&self) {
        let several = WidgetExt::display(&self.ctx.win).monitors().n_items() > 1;
        let state = bridge::state_json(&self.ctx.status.borrow(), several, "", false, false);
        self.emit("state", state);
    }

    /// A capsule someone sent: the page checks it and opens your AI.
    pub fn open_capsule(&self, path: &Path) {
        let text = std::fs::read(path).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
        let payload = json!({ "path": path.to_string_lossy(), "text": text });
        self.ctx.win.present();
        if self.ready.get() {
            self.emit("open-capsule", payload);
        } else {
            // Once the page is up.
            let path = path.to_path_buf();
            glib::timeout_add_local_once(Duration::from_millis(500), move || {
                if let Some(w) = WebWindow::get() {
                    w.open_capsule(&path);
                }
            });
        }
    }

    /// Every few seconds while the window shows: tell the page which files changed.
    fn watch_files(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        glib::timeout_add_seconds_local(WATCH_EVERY, move || {
            let Some(w) = weak.upgrade() else { return glib::ControlFlow::Break };
            if w.ctx.win.is_visible() {
                w.look_at_files();
            }
            glib::ControlFlow::Continue
        });
    }

    fn look_at_files(&self) {
        let paths = self.ctx.paths.clone();
        glib::spawn_future_local(async move {
            let Ok(now) = gio::spawn_blocking(move || Stamps::read(&paths)).await else { return };
            let Some(w) = WebWindow::get() else { return };
            let before = w.watch.replace(now.clone());
            if before == Stamps::default() {
                return; // the first look: nothing to compare with
            }
            let mut what = before.changed(&now);
            if what.contains(&"sessions") || w.sessions_due.get() {
                // While recording, at most every few seconds.
                let recent = w.sessions_told.get().is_some_and(|t| t.elapsed() < SESSIONS_EVERY);
                what.retain(|x| *x != "sessions");
                if recent {
                    w.sessions_due.set(true);
                } else {
                    w.sessions_due.set(false);
                    w.sessions_told.set(Some(Instant::now()));
                    what.push("sessions");
                }
            }
            if !what.is_empty() {
                w.emit("data-changed", json!({ "what": what }));
            }
        });
    }
}

/// What the watch compares: when each thing the page reads last changed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stamps {
    config: Option<SystemTime>,
    projects: Option<SystemTime>,
    capsules: Vec<(String, Option<SystemTime>)>,
    sessions: Vec<String>,
    live: Option<SystemTime>,
    read: bool,
}

fn modified(p: &Path) -> Option<SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

impl Stamps {
    pub fn read(paths: &Paths) -> Stamps {
        let names = |dir: &Path| -> Vec<String> {
            let mut v: Vec<String> =
                std::fs::read_dir(dir).map(|it| it.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect()).unwrap_or_default();
            v.sort();
            v
        };
        let capsules = names(&paths.capsules()).into_iter().map(|n| (n.clone(), modified(&paths.capsules().join(&n)))).collect();
        let live = context_capsule_core::timeline::recording_session(paths)
            .and_then(|id| modified(&paths.session_dir(&id).join("events.jsonl")));
        Stamps {
            config: modified(&paths.config()),
            projects: modified(&paths.projects()),
            capsules,
            sessions: names(&paths.sessions()),
            live,
            read: true,
        }
    }

    /// What the page should read again ("config", "projects", "capsules", "sessions").
    pub fn changed(&self, now: &Stamps) -> Vec<&'static str> {
        let mut what = vec![];
        if self.config != now.config {
            what.push("config");
        }
        if self.projects != now.projects {
            what.push("projects");
        }
        if self.capsules != now.capsules {
            what.push("capsules");
        }
        if self.sessions != now.sessions || self.live != now.live {
            what.push("sessions");
        }
        what
    }
}

// MARK: the host

/// What the page asks of the app. Lives on worker threads; GTK work goes to GTK's thread (`on_main`).
pub struct LinuxHost {
    paths: Paths,
    tx: Mutex<Sender<Msg>>,
    hotkeys: crate::hotkeys::Control,
    locale: String,
    /// The page to open on (taken by `host.hello`).
    page: Mutex<Option<String>>,
}

type Reply<R> = Box<dyn FnOnce(R)>;

/// Run `f` on GTK's thread with the window; it answers through `reply` (now, or when a dialog closes). None: the
/// window is gone, or it never answered.
fn on_main<R: Send + 'static>(f: impl FnOnce(&Rc<WebWindow>, Reply<R>) + Send + 'static) -> Option<R> {
    let (tx, rx) = std::sync::mpsc::channel();
    glib::MainContext::default().invoke(move || {
        if let Some(w) = WebWindow::get() {
            f(&w, Box::new(move |r| {
                let _ = tx.send(r);
            }));
        }
    });
    rx.recv().ok()
}

/// The same, for what answers at once.
fn on_main_now<R: Send + 'static>(f: impl FnOnce(&Rc<WebWindow>) -> R + Send + 'static) -> Option<R> {
    on_main(move |w, reply| reply(f(w)))
}

/// Is the system's clock 12-hour (as `formats` was set from the locale)?
fn hour12() -> bool {
    context_capsule_core::formats::time(13, 0) != "13:00"
}

impl LinuxHost {
    fn send(&self, msg: Msg) -> Result<(), String> {
        self.tx.lock().unwrap().send(msg).map_err(|_| "Context Capsule is stopping.".to_string())
    }
}

impl Host for LinuxHost {
    fn hello(&self) -> Hello {
        let passwd = std::fs::read_to_string("/etc/passwd").unwrap_or_default();
        let dark = on_main_now(|w| {
            w.ready.set(true);
            w.emit_state();
            adw::StyleManager::default().is_dark()
        })
        .unwrap_or(false);
        Hello {
            platform: "linux",
            locale: self.locale.clone(),
            hour12: Some(hour12()),
            first_weekday: None,
            user_name: ui::account_name(&env, &passwd),
            dark,
            first_run: false,
            page: self.page.lock().unwrap().take(),
            glass: false,
        }
    }

    fn config_changed(&self, key: &str, value: &Value) {
        match key {
            "appearance" => {
                let a = Appearance::load(&self.paths.config());
                on_main_now(move |w| {
                    apply_appearance(a);
                    let s = adw::StyleManager::default();
                    w.paint_behind(s.is_dark());
                    // Back to the system's look: the page needs to know what it is now.
                    if a == Appearance::System {
                        w.emit("appearance", json!({ "dark": s.is_dark() }));
                    }
                });
            }
            "ai" => {
                if let Some(id) = value.as_str() {
                    crate::ai::set_choice(id);
                }
            }
            _ => {}
        }
    }

    fn recorder(&self, action: &str, id: Option<&str>) -> Result<(), String> {
        match action {
            "toggle" => self.send(Msg::ToggleRecord),
            "voice" => self.send(Msg::ToggleVoice),
            "add-screen" => self.send(Msg::AddScreen),
            "project" => self.send(Msg::PickProject(id.map(String::from))),
            "new-project" => {
                on_main_now(|w| super::home::ask_new_project(&w.ctx));
                Ok(())
            }
            other => Err(format!("unknown action {other}")),
        }
    }

    fn set_hotkeys(&self, keys: Hotkeys) -> Result<(), String> {
        let old = crate::hotkeys::current();
        match self.hotkeys.apply(old, keys) {
            Ok(_) => {
                on_main_now(move |w| w.ctx.set_keys(&keys));
                Ok(())
            }
            Err((_, Refusal::Taken)) => Err("taken".into()),
            Err((_, r)) => Err(r.message(false)),
        }
    }

    fn listen_keys(&self, on: bool) {
        if on {
            self.hotkeys.pause();
        } else {
            self.hotkeys.resume(crate::hotkeys::current());
        }
    }

    fn setup_status(&self) -> Vec<(&'static str, bool)> {
        #[cfg(feature = "voice")]
        let voice = Some(("voice", crate::model::is_present(&crate::model::path(&self.paths.root))));
        #[cfg(not(feature = "voice"))]
        let voice = None;
        voice
            .into_iter()
            .chain([("ai", connect::is_connected(&env) || crate::ai::state().any_connected()), ("login", autostart::is_enabled(&env))])
            .collect()
    }

    fn setup_do(&self, id: &str) -> Result<(), String> {
        match id {
            #[cfg(feature = "voice")]
            "voice" => {
                on_main_now(|w| {
                    thread_local! {
                        static DOWNLOAD: super::settings::voice::Download = super::settings::voice::Download::default();
                    }
                    DOWNLOAD.with(|d| super::settings::voice::ask(&w.ctx, d));
                });
                Ok(())
            }
            "ai" => {
                on_main_now(|w| w.ctx.connect_claude());
                Ok(())
            }
            "login" => autostart::enable(&env).map_err(|_| ui::CANT_SAVE.to_string()),
            _ => Ok(()),
        }
    }

    fn mcp(&self) -> Option<Launch> {
        crate::ai::launch()
    }

    fn auto(&self) -> Option<Arc<context_capsule_core::auto::Auto>> {
        crate::auto::get()
    }

    fn ai_open(&self, id: &str, prompt: &str) -> Result<bool, String> {
        let (id, prompt) = (id.to_string(), prompt.to_string());
        on_main_now(move |w| {
            w.ctx.copy_text(&prompt);
            if id != "claude" {
                return Ok(false);
            }
            if w.ctx.open_uri(&new_chat_url(&prompt)) {
                Ok(true)
            } else {
                Err(ui::cant_open_claude_body(&prompt))
            }
        })
        .unwrap_or(Ok(false))
    }

    fn clipboard_read(&self) -> Result<String, String> {
        on_main(|w, reply| {
            let clipboard = w.ctx.win.clipboard();
            glib::spawn_future_local(async move {
                let text = clipboard.read_text_future().await.ok().flatten().map(|t| t.to_string());
                reply(text);
            });
        })
        .flatten()
        .or_else(crate::reminder::paste_text)
        .map_or(Ok(String::new()), Ok)
    }

    fn clipboard_write(&self, text: &str) -> Result<(), String> {
        let text = text.to_string();
        on_main_now(move |w| w.ctx.copy_text(&text)).ok_or_else(|| "The window is closed.".to_string())
    }

    fn copy_file(&self, path: &Path) -> Result<(), String> {
        let path = path.to_path_buf();
        on_main_now(move |w| super::copy_file(&w.ctx.win, &path)).ok_or_else(|| "The window is closed.".to_string())
    }

    fn open_url(&self, url: &str) -> Result<(), String> {
        let url = url.to_string();
        if on_main_now(move |w| w.ctx.open_uri(&url)).unwrap_or(false) {
            Ok(())
        } else {
            Err(ui::CANT_OPEN.into())
        }
    }

    fn open_file(&self, path: &Path) -> Result<(), String> {
        let uri = gio::File::for_path(path).uri().to_string();
        if on_main_now(move |w| w.ctx.open_uri(&uri)).unwrap_or(false) {
            Ok(())
        } else {
            Err(ui::CANT_OPEN.into())
        }
    }

    fn reveal(&self, path: &Path) -> Result<(), String> {
        let path = path.to_path_buf();
        on_main_now(move |w| super::capsules::show_in_folder(&w.ctx, &path)).ok_or_else(|| ui::CANT_OPEN.to_string())
    }

    fn save_panel(&self, name: &str) -> Option<PathBuf> {
        let name = name.to_string();
        on_main(move |w, reply| {
            let chooser = gtk::FileChooserNative::new(
                Some(words::SAVE_COPY.trim_end_matches('…')),
                Some(&w.ctx.win),
                gtk::FileChooserAction::Save,
                Some("Save"),
                Some(words::CANCEL),
            );
            chooser.set_current_name(&name);
            answer_once(chooser, reply);
        })
        .flatten()
    }

    fn pick_capsule(&self) -> Option<PathBuf> {
        on_main(|w, reply| {
            let chooser = gtk::FileChooserNative::new(
                Some(words::OPEN_CAPSULE.trim_end_matches('…')),
                Some(&w.ctx.win),
                gtk::FileChooserAction::Open,
                Some(words::OPEN),
                Some(words::CANCEL),
            );
            let filter = gtk::FileFilter::new();
            filter.set_name(Some("Capsule (*.md)"));
            filter.add_pattern("*.md");
            filter.add_pattern("*.MD");
            filter.add_mime_type("text/markdown");
            chooser.add_filter(&filter);
            answer_once(chooser, reply);
        })
        .flatten()
    }

    fn pick_app(&self) -> Option<(String, String)> {
        on_main(|w, reply| choose_app(&w.ctx, reply)).flatten()
    }

    fn app_names(&self, ids: &[String]) -> Map<String, Value> {
        let ids = ids.to_vec();
        on_main_now(move |_| {
            let installed: Vec<(String, String)> =
                super::settings::installed_apps_with_icons().into_iter().map(|(id, name, _)| (id, name)).collect();
            ids.iter()
                .filter(|id| installed.iter().any(|(i, _)| i.eq_ignore_ascii_case(id)))
                .map(|id| (id.clone(), Value::String(ui::excluded_name(id, &installed))))
                .collect()
        })
        .unwrap_or_default()
    }

    fn app_icon(&self, app: &str, bundle: &str) -> Option<Vec<u8>> {
        let (app, bundle, classes) = (app.to_string(), bundle.to_string(), wm_classes());
        on_main_now(move |_| app_icon_png(&app, &bundle, &classes)).flatten()
    }

    fn updates(&self, _action: &str) -> Result<(), String> {
        // Checking; a ready update asks to restart in its own message (update.rs).
        crate::update::check_now();
        Ok(())
    }

    fn emit(&self, event: &str, payload: Value) {
        let event = event.to_string();
        glib::MainContext::default().invoke(move || {
            if let Some(w) = WebWindow::get() {
                w.emit(&event, payload);
            }
        });
    }
}

/// StartupWMClass of every installed .desktop file by desktop id (read off the main thread; kept a minute, as
/// the page asks for many apps at once).
fn wm_classes() -> Arc<HashMap<String, String>> {
    type Kept = Mutex<Option<(Instant, Arc<HashMap<String, String>>)>>;
    static KEPT: std::sync::OnceLock<Kept> = std::sync::OnceLock::new();
    let mut kept = KEPT.get_or_init(Default::default).lock().unwrap();
    if let Some((at, m)) = kept.as_ref() {
        if at.elapsed() < Duration::from_secs(60) {
            return m.clone();
        }
    }
    let var = |k: &str| std::env::var(k).ok();
    let dirs = icon_match::desktop_dirs(var("XDG_DATA_HOME").as_deref(), var("HOME").as_deref(), var("XDG_DATA_DIRS").as_deref());
    let m = Arc::new(icon_match::wm_classes(&dirs));
    *kept = Some((Instant::now(), m.clone()));
    m
}

/// The app's icon as a PNG of `app_icons::SIZE` pixels (main thread): the installed app it is (icon_match), its
/// icon from the icon theme or its file; else an icon in the theme named like the program. None: no icon.
fn app_icon_png(app: &str, bundle: &str, classes: &HashMap<String, String>) -> Option<Vec<u8>> {
    let mut infos = gio::AppInfo::all();
    // Apps shown in menus win a tie over hidden helpers.
    infos.sort_by_key(|a| !a.should_show());
    let installed: Vec<icon_match::Installed> = infos
        .iter()
        .map(|a| {
            let id = a.id().map(|i| ui::desktop_app_id(i.as_str())).unwrap_or_default();
            icon_match::Installed {
                wm_class: classes.get(&id).cloned().unwrap_or_default(),
                exec: a.commandline().map(|c| icon_match::exec_name(&c.to_string_lossy())).unwrap_or_default(),
                name: a.display_name().to_string(),
                id,
            }
        })
        .collect();
    let theme = gtk::IconTheme::for_display(&gdk::Display::default()?);
    let icon: gio::Icon = match icon_match::best(&installed, app, bundle) {
        Some(i) => infos[i].icon()?,
        None => {
            let name = bundle.trim();
            if name.is_empty() || name.contains('/') || !theme.has_icon(name) {
                return None;
            }
            gio::ThemedIcon::new(name).upcast()
        }
    };
    icon_png(&theme, &icon)
}

/// A gio icon as PNG bytes about `app_icons::SIZE` pixels on its long side (SVGs drawn at that size).
fn icon_png(theme: &gtk::IconTheme, icon: &gio::Icon) -> Option<Vec<u8>> {
    let size = app_icons::SIZE as i32;
    let file = match icon.downcast_ref::<gio::FileIcon>() {
        Some(f) => f.file(),
        None => {
            if !theme.has_gicon(icon) {
                return None;
            }
            theme.lookup_by_gicon(icon, size, 1, gtk::TextDirection::None, gtk::IconLookupFlags::empty()).file()?
        }
    };
    let stream = file.read(gio::Cancellable::NONE).ok()?;
    let pixbuf = gtk::gdk_pixbuf::Pixbuf::from_stream_at_scale(&stream, size, size, true, gio::Cancellable::NONE).ok()?;
    pixbuf.save_to_bufferv("png", &[]).ok()
}

/// A file chooser that answers once: the file, or None (cancelled, closed).
fn answer_once(chooser: gtk::FileChooserNative, reply: Reply<Option<PathBuf>>) {
    let reply = RefCell::new(Some(reply));
    let keep: Rc<RefCell<Option<gtk::FileChooserNative>>> = Rc::new(RefCell::new(Some(chooser.clone())));
    chooser.connect_response(move |d, response| {
        let path = (response == gtk::ResponseType::Accept).then(|| d.file().and_then(|f| f.path())).flatten();
        if let Some(r) = reply.borrow_mut().take() {
            r(path);
        }
        keep.borrow_mut().take();
    });
    chooser.show();
}

/// "Never record" → Choose…: a searchable list of installed apps; (id, name) of the one picked.
fn choose_app(ctx: &Rc<Ctx>, reply: Reply<Option<(String, String)>>) {
    let apps = Rc::new(super::settings::installed_apps_with_icons());
    type Once = Rc<RefCell<Option<Reply<Option<(String, String)>>>>>;
    let reply: Once = Rc::new(RefCell::new(Some(reply)));
    let dialog = adw::Window::builder()
        .modal(true)
        .transient_for(&ctx.win)
        .default_width(420)
        .default_height(560)
        .title(ui::CHOOSE_APP)
        .build();
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&adw::WindowTitle::new(ui::CHOOSE_APP, "")));
    let search = gtk::SearchEntry::new();
    search.set_placeholder_text(Some(ui::SEARCH_APPS));
    search.set_margin_start(12);
    search.set_margin_end(12);
    search.set_margin_top(6);
    search.set_margin_bottom(6);
    let list = gtk::ListBox::new();
    list.add_css_class("navigation-sidebar");
    for (id, name, icon) in apps.iter() {
        let r = adw::ActionRow::builder().title(glib::markup_escape_text(name)).subtitle(glib::markup_escape_text(id)).build();
        let image = icon.as_ref().map_or_else(|| gtk::Image::from_icon_name("application-x-executable"), gtk::Image::from_gicon);
        image.set_pixel_size(32);
        r.add_prefix(&image);
        r.set_activatable(true);
        let (reply, id, name, d) = (reply.clone(), id.clone(), name.clone(), dialog.clone());
        r.connect_activated(move |_| {
            if let Some(r) = reply.borrow_mut().take() {
                r(Some((id.clone(), name.clone())));
            }
            d.close();
        });
        list.append(&r);
    }
    let (query, data) = (search.clone(), apps.clone());
    list.set_filter_func(move |r| {
        let text = query.text();
        data.get(r.index().max(0) as usize).is_none_or(|(id, name, _)| ui::app_matches(&text, name, id))
    });
    let l = list.clone();
    search.connect_search_changed(move |_| l.invalidate_filter());
    let scroll = gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).vexpand(true).child(&list).build();
    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&header);
    content.append(&search);
    content.append(&scroll);
    dialog.set_content(Some(&content));
    dialog.connect_close_request(move |_| {
        if let Some(r) = reply.borrow_mut().take() {
            r(None);
        }
        glib::Propagation::Proceed
    });
    dialog.present();
    search.grab_focus();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_changed() {
        let a = Stamps { read: true, sessions: vec!["20261023-112957".into()], ..Default::default() };
        assert!(a.changed(&a).is_empty());
        let b = Stamps { config: Some(SystemTime::UNIX_EPOCH), capsules: vec![("x.md".into(), None)], ..a.clone() };
        assert_eq!(a.changed(&b), ["config", "capsules"]);
        let c = Stamps { live: Some(SystemTime::UNIX_EPOCH), ..a.clone() };
        assert_eq!(a.changed(&c), ["sessions"]);
    }

    #[test]
    fn stamps_from_a_folder() {
        let root = std::env::temp_dir().join(format!("cc-web-stamps-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("capsules")).unwrap();
        std::fs::create_dir_all(root.join("sessions/20261023-112957")).unwrap();
        let paths = Paths::new(&root);
        let before = Stamps::read(&paths);
        assert_eq!(before.sessions, ["20261023-112957"]);
        std::fs::write(root.join("capsules/a.md"), "x").unwrap();
        std::fs::write(root.join("config.json"), "{}").unwrap();
        assert_eq!(before.changed(&Stamps::read(&paths)), ["config", "capsules"]);
        std::fs::remove_dir_all(&root).unwrap();
    }
}
