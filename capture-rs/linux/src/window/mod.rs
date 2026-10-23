// context-capsule — linux: the Context Capsule window (GTK 4 + libadwaita) — a sidebar (Your work: Home,
// Timeline, Capsules, Projects; Context Capsule: Settings, Help), a header with the section's name and a small
// status pill, and the page, centered and scrolling, at any window size. Raw recordings show only on the Timeline;
// elsewhere, time per project, counts and capsules.
//! Threads: GTK runs on the main thread (`Gtk::run`); the tray and the engine reach it through a
//! [`WindowLink`], which queues each message on GTK's main context. The window is built the first time it
//! opens; closing it only hides it (the app lives in the tray).

mod capsules;
mod help;
mod home;
mod projects;
mod settings;
mod share;
mod shortcuts;
mod timeline;
#[cfg(feature = "webview")]
mod web;
#[cfg(feature = "webview")]
mod wk;
mod widgets;
mod yourai;

use crate::link::{UiMsg, WindowLink};
use crate::ui;
use adw::prelude::*;
use context_capsule_core::anyai::YourAi;
use context_capsule_core::claude::{self, new_chat_url, Opened};
use context_capsule_core::settings::{Appearance, ConfigFile};
use context_capsule_core::stats::sunday_first;
use context_capsule_core::window::{self as words, HomeNumbers, Keys, Section};
use context_capsule_core::{Msg, Paths, Projects, Status};
use gtk::{gdk, gio, glib};
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::sync::mpsc::Sender;
use std::time::{Duration, SystemTime};

/// Home's numbers are read again this often while the window is open.
const NUMBERS_EVERY: Duration = Duration::from_secs(60);
/// While Settings shows, the AI apps are read again every this many seconds (as on the Mac).
const AI_EVERY: u32 = 10;
/// While today's Timeline shows and recording goes on, it's read again every this many seconds.
const TIMELINE_EVERY: u32 = 5;

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
    /// The shared window (ui/, in WebKitGTK) instead of this GTK one: `--new-window` or config "new_window".
    pub web: bool,
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
pub fn run_alone(paths: Paths, section: Section, locale: String, web: bool) -> i32 {
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
    let setup = Rc::new(RefCell::new(Some((Setup { paths, tx, open: Some(section), locale, hotkeys, web }, status))));
    app.connect_activate(move |app| {
        if let Some((setup, status)) = setup.borrow_mut().take() {
            let ui = Ui::new(app, setup);
            *ui.status.borrow_mut() = status;
            ui.open(Some(section));
            // Closing the window ends this mode.
            if let Some(w) = ui.window.borrow().as_ref() {
                w.ctx.win.set_hide_on_close(false);
            }
            #[cfg(feature = "webview")]
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

/// The app side of the window: the latest status, and the window once it was opened.
struct Ui {
    app: adw::Application,
    paths: Paths,
    tx: Sender<Msg>,
    locale: String,
    hotkeys: crate::hotkeys::Control,
    status: RefCell<Status>,
    window: RefCell<Option<Rc<Window>>>,
    /// Show the shared window (ui/) instead.
    #[cfg_attr(not(feature = "webview"), allow(dead_code))]
    web: bool,
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
            window: RefCell::new(None),
            web: setup.web && web_available(),
        })
    }

    fn handle(self: &Rc<Self>, msg: UiMsg) {
        #[cfg(feature = "webview")]
        if self.web {
            return self.handle_web(msg);
        }
        match msg {
            UiMsg::Open(section) => self.open(section),
            UiMsg::Status(status) => {
                *self.status.borrow_mut() = (*status).clone();
                if let Some(w) = self.window.borrow().clone() {
                    w.set_status(*status);
                }
            }
            UiMsg::OpenCapsule(path) => self.window().ctx.open_received(&path),
            // From the tray or the reminder: the window comes to the front (its clipboard and messages need it),
            // except for Claude, which opens by itself.
            UiMsg::MakeCapsules => {
                if crate::ai::state().kind() != YourAi::Claude {
                    self.open(None);
                }
                self.window().ctx.make_capsules();
            }
            UiMsg::SaveFromClipboard => {
                self.open(None);
                self.window().ctx.save_from_clipboard();
            }
            UiMsg::AiChanged => {
                if let Some(w) = self.window.borrow().clone() {
                    w.set_ai();
                }
            }
            UiMsg::Quit => self.app.quit(),
        }
    }

    /// The window, built the first time (not shown).
    fn window(self: &Rc<Self>) -> Rc<Window> {
        let existing = self.window.borrow().clone();
        existing.unwrap_or_else(|| {
            let w = Window::build(&self.app, &self.paths, &self.tx, &self.locale, &self.hotkeys, self.status.borrow().clone());
            *self.window.borrow_mut() = Some(w.clone());
            w
        })
    }

    /// The shared window, built the first time (not shown).
    #[cfg(feature = "webview")]
    fn web_window(self: &Rc<Self>, section: Option<Section>) -> Rc<web::WebWindow> {
        web::WebWindow::get().unwrap_or_else(|| {
            let section = section.unwrap_or(Section::Home);
            web::WebWindow::build(&self.app, &self.paths, &self.tx, &self.locale, &self.hotkeys, self.status.borrow().clone(), section)
        })
    }

    #[cfg(feature = "webview")]
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
            UiMsg::Quit => self.app.quit(),
        }
    }

    /// Show the window (built the first time), on `section` if given, with fresh numbers.
    fn open(self: &Rc<Self>, section: Option<Section>) {
        #[cfg(feature = "webview")]
        if self.web {
            let built = web::WebWindow::get().is_some();
            let w = self.web_window(section);
            w.open(if built { section } else { None });
            return;
        }
        let w = self.window();
        w.show(section.unwrap_or(w.section.get()));
        w.reload_numbers();
        w.ctx.win.present();
    }
}

/// The shared window can open: built with it, and WebKitGTK 6.0 is installed (else the GTK window, and a line why).
fn web_available() -> bool {
    #[cfg(feature = "webview")]
    {
        if wk::available() {
            return true;
        }
        eprintln!("Context Capsule: the new window needs WebKitGTK 6.0 ({}, see the README); showing the usual window", wk::LIBRARY);
    }
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

/// Go to a section (set once the window is built).
type Nav = Box<dyn Fn(Section)>;

/// What every page shares.
pub struct Ctx {
    pub paths: Paths,
    tx: Sender<Msg>,
    pub locale: String,
    /// The words that name the current shortcuts.
    keys: RefCell<Keys>,
    pub hotkeys: crate::hotkeys::Control,
    pub status: RefCell<Status>,
    pub numbers: RefCell<HomeNumbers>,
    pub win: adw::ApplicationWindow,
    toasts: adw::ToastOverlay,
    nav: RefCell<Option<Nav>>,
    /// Home's "See today's timeline": the Timeline shows today when it opens next.
    pub timeline_today: Cell<bool>,
}

impl Ctx {
    /// The words that name the shortcuts (the current ones).
    pub fn keys(&self) -> Keys {
        self.keys.borrow().clone()
    }

    /// The shortcuts changed: name the new ones (Home and Help show them the next time they show).
    pub fn set_keys(&self, h: &context_capsule_core::Hotkeys) {
        *self.keys.borrow_mut() = Keys::linux(h);
    }

    /// Tell the engine (does nothing in `--window-only`).
    pub fn send(&self, msg: Msg) {
        let _ = self.tx.send(msg);
    }

    pub fn toast(&self, text: &str) {
        let t = adw::Toast::new(text);
        t.set_timeout(3);
        self.toasts.add_toast(t);
    }

    pub fn go(&self, section: Section) {
        if let Some(nav) = self.nav.borrow().as_ref() {
            nav(section);
        }
    }

    /// Save one setting at once; say so if it couldn't be saved.
    pub fn set(&self, key: &str, value: Option<serde_json::Value>) {
        if let Err(e) = ConfigFile::set(&self.paths.config(), key, value) {
            eprintln!("Context Capsule: cannot save {key}: {e}");
            self.toast(ui::CANT_SAVE);
        }
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

    /// A capsule someone sent (Open with, or "Open a capsule…"): your AI carries on from it. Not a capsule: say so.
    pub fn open_received(self: &Rc<Self>, path: &std::path::Path) {
        match claude::open_file(path) {
            Opened::Capsule { prompt } => self.open_prompt(&prompt, crate::ai::WHY_CAPSULE, crate::ai::AGAIN_CAPSULE),
            Opened::NotACapsule => {
                self.win.present();
                widgets::message(&self.win, claude::NOT_A_CAPSULE, claude::NOT_A_CAPSULE_BODY);
            }
            Opened::Unreadable => {
                self.win.present();
                widgets::message(&self.win, claude::NOT_A_CAPSULE, claude::CANT_READ);
            }
        }
    }
}

/// The window: sidebar, header, pages.
pub struct Window {
    pub ctx: Rc<Ctx>,
    sidebar: gtk::ListBox,
    stack: gtk::Stack,
    title: adw::WindowTitle,
    pill: gtk::Label,
    pill_box: gtk::Box,
    pill_icon: gtk::Image,
    home: Rc<home::Home>,
    timeline: Rc<timeline::Timeline>,
    capsules: Rc<capsules::Capsules>,
    projects: Rc<projects::ProjectsPage>,
    settings: Rc<settings::Settings>,
    help: help::Help,
    clamps: Vec<adw::Clamp>,
    section: Cell<Section>,
    wide: Cell<Option<bool>>,
    /// `ai::AiState::saves` as last seen: a save from the clipboard worked, read the lists again.
    saves: Cell<u64>,
}

impl Window {
    fn build(
        app: &adw::Application,
        paths: &Paths,
        tx: &Sender<Msg>,
        locale: &str,
        hotkeys: &crate::hotkeys::Control,
        status: Status,
    ) -> Rc<Window> {
        let win = adw::ApplicationWindow::builder()
            .application(app)
            .title(words::APP_NAME)
            .default_width(words::START_WIDTH as i32)
            .default_height(words::START_HEIGHT as i32)
            .hide_on_close(true)
            .build();
        win.set_size_request(words::MIN_WIDTH as i32, words::MIN_HEIGHT as i32);
        win.add_css_class("cc-window");
        // For pictures of the window (see the README): CONTEXT_CAPSULE_WINDOW_SIZE=1100x760.
        if let Some((width, height)) = std::env::var("CONTEXT_CAPSULE_WINDOW_SIZE").ok().as_deref().and_then(ui::parse_size) {
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

        // The sidebar, in two groups.
        let sidebar = gtk::ListBox::new();
        sidebar.add_css_class("navigation-sidebar");
        for s in Section::ALL {
            sidebar.append(&widgets::sidebar_row(s));
        }
        sidebar.set_header_func(|row, before| {
            let group = |r: &gtk::ListBoxRow| Section::ALL.get(r.index().max(0) as usize).map(|s| s.group());
            if before.and_then(group) != group(row) {
                let first = before.is_none();
                row.set_header(group(row).map(|g| widgets::sidebar_heading(g, first)).as_ref());
            } else {
                row.set_header(None::<&gtk::Widget>);
            }
        });
        let side_header = adw::HeaderBar::new();
        side_header.set_show_end_title_buttons(false);
        // As on the Mac, the band above the sidebar stays empty (the window's name is its title).
        side_header.set_title_widget(Some(&gtk::Label::new(None)));
        let side_scroll = gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).vexpand(true).child(&sidebar).build();
        let side = gtk::Box::new(gtk::Orientation::Vertical, 0);
        side.add_css_class("cc-sidebar");
        side.set_size_request(ui::SIDEBAR_WIDTH, -1);
        side.append(&side_header);
        side.append(&side_scroll);

        // The header over the page: its name, and the status pill.
        let title = adw::WindowTitle::new(Section::Home.title(), "");
        let pill = widgets::line("", &["numeric"]);
        pill.set_ellipsize(gtk::pango::EllipsizeMode::None);
        // As on the Mac: a clock before today's time (while recording, the text starts with a red dot).
        let pill_icon = gtk::Image::from_icon_name("document-open-recent-symbolic");
        let pill_box = widgets::hbox(6);
        pill_box.add_css_class("status-pill");
        pill_box.set_valign(gtk::Align::Center);
        pill_box.append(&pill_icon);
        pill_box.append(&pill);
        let header = adw::HeaderBar::new();
        header.set_show_start_title_buttons(false);
        // The page names itself (as on the Mac); the band stays empty but for the pill.
        title.set_visible(false);
        header.set_title_widget(Some(&title));
        header.pack_end(&pill_box);

        let home = home::Home::new(&ctx);
        let timeline = timeline::Timeline::new(&ctx);
        let capsules = capsules::Capsules::new(&ctx);
        let projects = projects::ProjectsPage::new(&ctx);
        let settings = settings::Settings::new(&ctx);
        let help = help::Help::new(&ctx);
        let stack = gtk::Stack::new();
        stack.set_transition_type(gtk::StackTransitionType::None);
        let mut clamps = vec![];
        for (section, content) in [
            (Section::Home, home.root.clone().upcast::<gtk::Widget>()),
            (Section::Timeline, timeline.root.clone().upcast()),
            (Section::Capsules, capsules.root.clone().upcast()),
            (Section::Projects, projects.root.clone().upcast()),
            (Section::Settings, settings.root.clone().upcast()),
            (Section::Help, help.root.clone().upcast()),
        ] {
            let (scroll, clamp) = widgets::page(&content);
            clamps.push(clamp);
            stack.add_named(&scroll, Some(section.id()));
        }
        stack.add_css_class("cc-page");
        let watch = widgets::WidthWatch::new(&stack);
        toasts.set_child(Some(&watch));
        let main = gtk::Box::new(gtk::Orientation::Vertical, 0);
        main.set_hexpand(true);
        main.append(&header);
        main.append(&toasts);
        let all = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        all.append(&side);
        all.append(&main);
        win.set_content(Some(&all));

        let w = Rc::new(Window {
            ctx: ctx.clone(),
            sidebar: sidebar.clone(),
            stack,
            title,
            pill,
            pill_box,
            pill_icon,
            home,
            timeline,
            capsules,
            projects,
            settings,
            help,
            clamps,
            section: Cell::new(Section::Home),
            wide: Cell::new(None),
            saves: Cell::new(crate::ai::state().saves),
        });
        let weak = Rc::downgrade(&w);
        *ctx.nav.borrow_mut() = Some(Box::new(move |s| {
            if let Some(w) = weak.upgrade() {
                w.show(s);
            }
        }));
        let weak = Rc::downgrade(&w);
        sidebar.connect_row_selected(move |_, row| {
            let (Some(w), Some(row)) = (weak.upgrade(), row) else { return };
            if let Some(s) = Section::ALL.get(row.index().max(0) as usize) {
                if *s != w.section.get() {
                    w.show(*s);
                }
            }
        });
        let weak = Rc::downgrade(&w);
        watch.on_width(move |width| {
            if let Some(w) = weak.upgrade() {
                w.fit(width);
            }
        });
        w.start_timers();
        let status = w.ctx.status.borrow().clone();
        w.set_status(status);
        w
    }

    /// Switch to a section: its page, the header and the window title.
    fn show(&self, section: Section) {
        self.section.set(section);
        self.stack.set_visible_child_name(section.id());
        self.title.set_title(section.title());
        self.ctx.win.set_title(Some(section.title()));
        if let Some(row) = self.sidebar.row_at_index(Section::ALL.iter().position(|s| *s == section).unwrap_or(0) as i32) {
            if !row.is_selected() {
                self.sidebar.select_row(Some(&row));
            }
        }
        if section != Section::Settings {
            self.settings.leave();
        }
        match section {
            Section::Home => self.home.refresh(),
            Section::Timeline => self.timeline.refresh(),
            Section::Capsules => self.capsules.refresh(),
            Section::Projects => self.projects.refresh(),
            Section::Settings => {
                self.settings.refresh();
                yourai::refresh();
            }
            Section::Help => self.help.refresh(&self.ctx),
        }
    }

    /// Your AI changed: every place that shows it; after a save, the lists and numbers again.
    fn set_ai(self: &Rc<Self>) {
        self.home.set_ai();
        self.capsules.set_ai();
        self.settings.set_ai();
        if self.section.get() == Section::Help {
            self.help.refresh(&self.ctx);
        }
        let saves = crate::ai::state().saves;
        if self.saves.replace(saves) != saves {
            self.capsules.refresh();
            self.reload_numbers();
        }
    }

    /// The content area is `width` wide: how wide pages may be, and Home's side column.
    fn fit(&self, width: i32) {        let (max, wide) = ui::layout(width as f64);
        for c in &self.clamps {
            c.set_maximum_size(max);
            c.set_tightening_threshold(max);
        }
        if self.wide.replace(Some(wide)) != Some(wide) {
            self.home.set_wide(wide);
        }
    }

    fn set_status(self: &Rc<Self>, status: Status) {
        let was_recording = self.ctx.status.borrow().recording;
        let projects_changed = self.ctx.status.borrow().projects != status.projects;
        *self.ctx.status.borrow_mut() = status;
        self.home.set_status();
        self.update_pill();
        if projects_changed && self.section.get() == Section::Projects {
            self.projects.refresh();
        }
        if was_recording && !self.ctx.status.borrow().recording {
            self.reload_numbers();
            if self.section.get() == Section::Timeline {
                self.timeline.load();
            }
        }
    }

    fn update_pill(&self) {
        let status = self.ctx.status.borrow();
        let text = words::status_pill(&status, SystemTime::now(), self.ctx.numbers.borrow().today.seconds);
        self.pill.set_text(&text);
        self.pill_icon.set_visible(!status.recording);
        if status.recording {
            self.pill_box.add_css_class("recording");
        } else {
            self.pill_box.remove_css_class("recording");
        }
    }

    /// Home's numbers, read off the GTK thread.
    fn reload_numbers(self: &Rc<Self>) {
        let paths = self.ctx.paths.clone();
        let sunday = sunday_first(&self.ctx.locale);
        let weak: Weak<Window> = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let load = gio::spawn_blocking(move || {
                let passwd = std::fs::read_to_string("/etc/passwd").unwrap_or_default();
                let name = ui::account_name(&|k| std::env::var(k).ok(), &passwd);
                HomeNumbers::load(&paths, SystemTime::now(), sunday, &name)
            });
            let Ok(numbers) = load.await else {
                eprintln!("Context Capsule: couldn't read Home's numbers");
                return;
            };
            if let Some(w) = weak.upgrade() {
                *w.ctx.numbers.borrow_mut() = numbers;
                w.home.set_numbers();
                w.update_pill();
            }
        });
    }

    /// Every second: the running time while recording, Settings' setup rows while they show (and every
    /// `AI_EVERY` seconds, the AI apps). Every minute: Home's numbers while the window shows.
    fn start_timers(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        let ticks = Cell::new(0u32);
        let timeline_ticks = Cell::new(0u32);
        glib::timeout_add_seconds_local(1, move || {
            let Some(w) = weak.upgrade() else { return glib::ControlFlow::Break };
            if !w.ctx.win.is_visible() {
                return glib::ControlFlow::Continue;
            }
            if w.ctx.status.borrow().recording {
                w.home.tick();
                w.update_pill();
                // Today's Timeline follows the recording.
                if w.section.get() == Section::Timeline && w.timeline.is_today() {
                    timeline_ticks.set(timeline_ticks.get() + 1);
                    if timeline_ticks.get() >= TIMELINE_EVERY {
                        timeline_ticks.set(0);
                        w.timeline.load();
                    }
                }
            }
            if w.section.get() == Section::Settings {
                w.settings.refresh_setup();
                ticks.set(ticks.get() + 1);
                if ticks.get().is_multiple_of(AI_EVERY) {
                    yourai::refresh();
                }
            }
            glib::ControlFlow::Continue
        });
        let weak = Rc::downgrade(self);
        glib::timeout_add_local(NUMBERS_EVERY, move || {
            let Some(w) = weak.upgrade() else { return glib::ControlFlow::Break };
            if w.ctx.win.is_visible() {
                w.reload_numbers();
            }
            glib::ControlFlow::Continue
        });
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
