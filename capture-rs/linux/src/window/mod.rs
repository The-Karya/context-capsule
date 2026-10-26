// context-capsule — linux: the Context Capsule window — the shared page (ui/ at the repo root, the same as on the
// Mac and Windows) in WebKitGTK 6.0 (`web.rs`), with GTK 4 + libadwaita for its own dialogs. Without WebKitGTK
// 6.0 a message says which package to install; the tray and the hotkeys work either way.
//! Threads: GTK runs on the main thread (`Gtk::run`); the tray and the engine reach it through a
//! [`WindowLink`], which queues each message on GTK's main context. The window is built the first time it
//! opens; closing it only hides it (the app lives in the tray).

mod native;
mod web;
mod wk;
mod widgets;
mod yourai;

use crate::link::{UiMsg, WindowLink};
use crate::ui;
use adw::prelude::*;
use context_capsule_core::anyai::YourAi;
use context_capsule_core::claude::new_chat_url;
use context_capsule_core::settings::Appearance;
use context_capsule_core::window::{self as words, Section};
use context_capsule_core::{Msg, Paths, Projects, Status};
use gtk::{gdk, gio, glib};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::mpsc::Sender;

/// GTK, started on the main thread.
pub struct Gtk {
    app: adw::Application,
}

/// What the window needs from the rest of the app.
pub struct Setup {
    pub paths: Paths,
    pub tx: Sender<Msg>,
    /// Open the window at once, on this section (the first launch; no tray).
    pub open: Option<Section>,
    pub locale: String,
    /// Changes the hotkeys (Settings → Shortcuts).
    pub hotkeys: crate::hotkeys::Control,
}

thread_local! {
    static UI: RefCell<Option<Rc<Ui>>> = const { RefCell::new(None) };
    /// Messages that came before the window's app was ready.
    static PENDING: RefCell<Vec<UiMsg>> = const { RefCell::new(vec![]) };
}

impl Gtk {
    /// Start GTK. None: there is no display (or GTK can't start); the app then runs without a window.
    /// `open`: a capsule this launch was asked to open (Open with); a running Context Capsule is handed it.
    pub fn start(open: Option<&std::path::Path>) -> Option<Gtk> {
        if let Err(e) = gtk::init() {
            eprintln!("Context Capsule: no window ({e}); the tray and hotkeys still work");
            return None;
        }
        let _ = adw::init();
        glib::set_application_name(words::APP_NAME); // the app's name where the desktop shows one
        let app = adw::Application::builder().application_id(crate::APP_ID).flags(gio::ApplicationFlags::HANDLES_OPEN).build();
        if let Err(e) = app.register(None::<&gio::Cancellable>) {
            eprintln!("Context Capsule: {e}");
        }
        if app.is_remote() {
            match open {
                Some(path) => app.open(&[gio::File::for_path(path)], ""), // the running one opens the capsule
                None => app.activate(),                                    // the running one opens its window
            }
        }
        Some(Gtk { app })
    }

    /// Another Context Capsule already runs (and was asked to open its window).
    pub fn is_remote(&self) -> bool {
        self.app.is_remote()
    }

    /// How other threads reach the window.
    pub fn link(&self) -> WindowLink {
        WindowLink::new(|msg| glib::MainContext::default().invoke(move || handle(msg)))
    }

    /// GTK's loop, until `WindowLink::quit`.
    pub fn run(self, setup: Setup) {
        let Gtk { app } = self;
        let first = Rc::new(Cell::new(true));
        let setup = Rc::new(RefCell::new(Some(setup)));
        app.connect_activate(move |app| {
            if let Some(setup) = setup.borrow_mut().take() {
                let open = setup.open;
                let ui = Ui::new(app, setup);
                UI.with(|u| *u.borrow_mut() = Some(ui.clone()));
                for msg in PENDING.with(|p| p.take()) {
                    ui.handle(msg);
                }
                if let Some(section) = open {
                    ui.open(Some(section));
                }
            }
            // Launched again (the app menu): open the window.
            if !first.replace(false) {
                handle(UiMsg::Open(None));
            }
        });
        // Launched again with a capsule (Open with → Context Capsule): GApplication hands us the file.
        app.connect_open(|_, files, _| {
            for path in files.iter().filter_map(|f| f.path()) {
                handle(UiMsg::OpenCapsule(path));
            }
        });
        let _hold = app.hold(); // a tray app: no window is fine
        app.run_with_args(&["context-capsule"]);
        UI.with(|u| u.borrow_mut().take());
    }
}

fn handle(msg: UiMsg) {
    match UI.with(|u| u.borrow().clone()) {
        Some(ui) => ui.handle(msg),
        None => PENDING.with(|p| p.borrow_mut().push(msg)),
    }
}

/// `--window-only`: the window by itself, with an idle status and this computer's data (no tray, engine or
/// portal). Buttons that would talk to the engine do nothing. Returns the exit code.
pub fn run_alone(paths: Paths, section: Section, locale: String) -> i32 {
    if let Err(e) = gtk::init() {
        eprintln!("Context Capsule: cannot open a window: {e}");
        return 1;
    }
    let _ = adw::init();
    glib::set_application_name(words::APP_NAME);
    let app = adw::Application::builder()
        .application_id(format!("{}.WindowOnly", crate::APP_ID))
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    let (tx, rx) = std::sync::mpsc::channel::<Msg>();
    let status = Status { projects: Projects::load(&paths.projects()), ..Default::default() };
    let hotkeys = crate::hotkeys::Control::none();
    let setup = Rc::new(RefCell::new(Some((Setup { paths, tx, open: Some(section), locale, hotkeys }, status))));
    app.connect_activate(move |app| {
        if let Some((setup, status)) = setup.borrow_mut().take() {
            let ui = Ui::new(app, setup);
            *ui.status.borrow_mut() = status;
            ui.open(Some(section));
            // Closing the window ends this mode.
            if let Some(w) = web::WebWindow::get() {
                w.ctx.win.set_hide_on_close(false);
            }
            UI.with(|u| *u.borrow_mut() = Some(ui));
        }
    });
    app.run_with_args(&["context-capsule"]);
    drop(rx);
    0
}

/// The app side of the window: the latest status; the window is built the first time it opens.
struct Ui {
    app: adw::Application,
    paths: Paths,
    tx: Sender<Msg>,
    locale: String,
    hotkeys: crate::hotkeys::Control,
    status: RefCell<Status>,
    /// The shared window (ui/) can open (WebKitGTK 6.0 is installed).
    web: bool,
    /// The "needs WebKitGTK" message, while it shows.
    webkit_message: Rc<RefCell<Option<adw::MessageDialog>>>,
}

impl Ui {
    fn new(app: &adw::Application, setup: Setup) -> Rc<Ui> {
        widgets::load_css();
        apply_appearance(Appearance::load(&setup.paths.config()));
        // Your AI changed (on any thread): show it here, on GTK's.
        crate::ai::on_change(|| glib::MainContext::default().invoke(|| handle(UiMsg::AiChanged)));
        Rc::new(Ui {
            app: app.clone(),
            paths: setup.paths,
            tx: setup.tx,
            locale: setup.locale,
            hotkeys: setup.hotkeys,
            status: RefCell::new(Status::default()),
            web: web_available(),
            webkit_message: Rc::default(),
        })
    }

    fn handle(self: &Rc<Self>, msg: UiMsg) {
        if self.web {
            return self.handle_web(msg);
        }
        // Without WebKitGTK: what would open the window says what to install.
        match msg {
            UiMsg::Open(_) | UiMsg::OpenCapsule(_) | UiMsg::MakeCapsules | UiMsg::SaveFromClipboard => self.needs_webkit(),
            UiMsg::Status(status) => *self.status.borrow_mut() = *status,
            UiMsg::AiChanged | UiMsg::Event(..) => {}
            UiMsg::Quit => self.app.quit(),
        }
    }

    /// The shared window, built the first time (not shown).
    fn web_window(self: &Rc<Self>, section: Option<Section>) -> Rc<web::WebWindow> {
        web::WebWindow::get().unwrap_or_else(|| {
            let section = section.unwrap_or(Section::Home);
            web::WebWindow::build(&self.app, &self.paths, &self.tx, &self.locale, &self.hotkeys, self.status.borrow().clone(), section)
        })
    }

    fn handle_web(self: &Rc<Self>, msg: UiMsg) {
        match msg {
            UiMsg::Open(section) => self.open(section),
            UiMsg::Status(status) => {
                *self.status.borrow_mut() = (*status).clone();
                if let Some(w) = web::WebWindow::get() {
                    w.set_status(*status);
                }
            }
            UiMsg::OpenCapsule(path) => self.web_window(None).open_capsule(&path),
            // From the tray or the reminder: the window comes to the front (its clipboard and messages need it),
            // except for Claude, which opens by itself.
            UiMsg::MakeCapsules => {
                let w = self.web_window(None);
                if crate::ai::state().kind() != YourAi::Claude {
                    w.open(None);
                }
                w.ctx.make_capsules();
            }
            UiMsg::SaveFromClipboard => {
                let w = self.web_window(None);
                w.open(None);
                w.ctx.save_from_clipboard();
            }
            UiMsg::AiChanged => {
                if let Some(w) = web::WebWindow::get() {
                    w.emit("data-changed", serde_json::json!({ "what": ["clients"] }));
                }
            }
            UiMsg::Event(event, payload) => {
                if let (Some(w), Ok(payload)) = (web::WebWindow::get(), serde_json::from_str(&payload)) {
                    w.emit(&event, payload);
                }
            }
            UiMsg::Quit => self.app.quit(),
        }
    }

    /// Show the window (built the first time), on `section` if given.
    fn open(self: &Rc<Self>, section: Option<Section>) {
        if !self.web {
            return self.needs_webkit();
        }
        let built = web::WebWindow::get().is_some();
        let w = self.web_window(section);
        w.open(if built { section } else { None });
    }

    /// WebKitGTK 6.0 isn't installed: a message says which package to install (one at a time).
    fn needs_webkit(&self) {
        if let Some(d) = self.webkit_message.borrow().as_ref() {
            return d.present();
        }
        let d = adw::MessageDialog::new(None::<&gtk::Window>, Some(ui::NEEDS_WEBKIT_TITLE), Some(ui::NEEDS_WEBKIT_BODY));
        d.set_application(Some(&self.app));
        d.set_title(Some(words::APP_NAME));
        d.add_response("ok", ui::OK);
        d.set_default_response(Some("ok"));
        d.set_close_response("ok");
        let cell = self.webkit_message.clone();
        d.connect_response(None, move |_, _| {
            cell.borrow_mut().take();
        });
        *self.webkit_message.borrow_mut() = Some(d.clone());
        d.present();
    }
}

/// The shared window can open: WebKitGTK 6.0 is installed (else a line why).
fn web_available() -> bool {
    if wk::available() {
        return true;
    }
    eprintln!("Context Capsule: the window needs WebKitGTK 6.0 ({}, see the README); the tray and hotkeys still work", wk::LIBRARY);
    false
}

/// light / dark: like the computer, or always one (Settings → Appearance).
pub fn apply_appearance(a: Appearance) {
    let scheme = match a {
        Appearance::System => adw::ColorScheme::Default,
        Appearance::Light => adw::ColorScheme::ForceLight,
        Appearance::Dark => adw::ColorScheme::ForceDark,
    };
    adw::StyleManager::default().set_color_scheme(scheme);
}

/// What the window's dialogs and Your AI share.
pub struct Ctx {
    pub paths: Paths,
    tx: Sender<Msg>,
    pub status: RefCell<Status>,
    pub win: adw::ApplicationWindow,
    toasts: adw::ToastOverlay,
}

impl Ctx {
    /// Tell the engine (does nothing in `--window-only`).
    pub fn send(&self, msg: Msg) {
        let _ = self.tx.send(msg);
    }

    pub fn toast(&self, text: &str) {
        let t = adw::Toast::new(text);
        t.set_timeout(3);
        self.toasts.add_toast(t);
    }

    /// Put `text` on the clipboard. When the window isn't in front (from the tray), Wayland may not let it take
    /// the clipboard: the clipboard tools (wl-copy, xclip) too, when installed.
    pub fn copy_text(&self, text: &str) {
        self.win.clipboard().set_text(text);
        if !self.win.is_active() {
            let text = text.to_string();
            std::thread::spawn(move || crate::reminder::copy_text(&text));
        }
    }

    pub fn launch_context(&self) -> gio::AppLaunchContext {
        WidgetExt::display(&self.win).app_launch_context().upcast()
    }

    /// Open a link or file with its app. false: nothing opens it.
    pub fn open_uri(&self, uri: &str) -> bool {
        gio::AppInfo::launch_default_for_uri(uri, Some(&self.launch_context())).is_ok()
    }

    /// Claude on a new chat with `prompt` typed in (also on the clipboard). Can't: say how to do it by hand.
    pub fn ask_claude(&self, prompt: &str) {
        self.copy_text(prompt);
        if !self.open_uri(&new_chat_url(prompt)) {
            self.win.present();
            widgets::message(&self.win, ui::CANT_OPEN_CLAUDE, &ui::cant_open_claude_body(prompt));
        }
    }
}

/// The display's clipboard can also hold a file (Capsules → Copy file).
pub fn copy_file(win: &adw::ApplicationWindow, path: &std::path::Path) {
    let file = gio::File::for_path(path);
    let list = gdk::FileList::from_array(std::slice::from_ref(&file));
    let gnome = format!("copy\n{}", file.uri());
    let providers = [
        gdk::ContentProvider::for_value(&list.to_value()),
        gdk::ContentProvider::for_bytes("x-special/gnome-copied-files", &glib::Bytes::from_owned(gnome)),
    ];
    win.clipboard().set_content(Some(&gdk::ContentProvider::new_union(&providers))).ok();
}
