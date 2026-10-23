// context-capsule — linux: Settings, in the Mac's order and look — You (your name); Set up (voice notes, Connect
// your AI with its list of AI apps, open at login), each a check mark when done, else its button; Your AI (what
// Make today's capsules uses); your own shortcuts; Screens (when you start); Look (light or dark); Every day
// (reminder, its time, deleting old recordings, sounds); the voice notes' language; apps never recorded; updates.
// Every change is saved at once (config.json, one key at a time).

use super::widgets::{self, hbox, label, line, note, row, vbox};
use super::Ctx;
use crate::{ai, autostart, ui};
use adw::prelude::*;
use context_capsule_core::anyai::{self, AiClient, Choice};
use context_capsule_core::settings::{self as cfg, Appearance, ShareScreens};
use context_capsule_core::window::{self as words, SetupRow, ROW_AI, ROW_LOGIN};
use context_capsule_core::Config;
use gtk::{gio, glib};
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

fn env(k: &str) -> Option<String> {
    std::env::var(k).ok()
}

/// One "Set up" row: a check mark when done, else its button.
struct Setup {
    row: adw::ActionRow,
    button: gtk::Button,
    check: gtk::Image,
    done: Box<dyn Fn() -> bool>,
    /// What to say instead of the help line right now (a download running…), and whether the button works.
    busy: Box<dyn Fn() -> Option<(String, bool)>>,
}

pub struct Settings {
    ctx: Rc<Ctx>,
    pub root: gtk::Box,
    body: gtk::Box,
    setup: RefCell<Vec<Setup>>,
    /// The never-record rows (inside their group).
    never: RefCell<gtk::Box>,
    me: RefCell<Weak<Settings>>,
    /// Shortcuts (kept across refreshes: it may be listening for keys).
    keys: Rc<super::shortcuts::Shortcuts>,
    /// Connect your AI (made again with the page).
    connect: RefCell<Option<ConnectRows>>,
    /// The not-installed apps show (under "More").
    more: Cell<bool>,
    /// Your AI's choice (made again with the page).
    choice: RefCell<Option<ChoiceRow>>,
    #[cfg(feature = "voice")]
    download: voice::Download,
}

/// "Connect your AI": a ✓ when one is connected (a spinner until the list is read); under it, inside the same
/// panel, one line per app, "› More" and the notes.
struct ConnectRows {
    check: gtk::Image,
    spinner: gtk::Spinner,
    extra: gtk::Box,
}

/// "Make today's capsules with" and the line under it.
struct ChoiceRow {
    combo: gtk::DropDown,
    line: gtk::Label,
    shown: Rc<RefCell<Vec<Choice>>>,
    /// Set while the list is changed here (not a pick).
    quiet: Rc<Cell<bool>>,
}

impl Settings {
    pub fn new(ctx: &Rc<Ctx>) -> Rc<Settings> {
        let root = vbox(24);
        root.append(&label(words::Section::Settings.title(), &["title-1"]));
        let body = vbox(24);
        root.append(&body);
        let s = Rc::new(Settings {
            ctx: ctx.clone(),
            root,
            body,
            setup: RefCell::new(vec![]),
            never: RefCell::new(vbox(0)),
            me: RefCell::new(Weak::new()),
            keys: super::shortcuts::Shortcuts::new(ctx),
            connect: RefCell::new(None),
            more: Cell::new(false),
            choice: RefCell::new(None),
            #[cfg(feature = "voice")]
            download: voice::Download::default(),
        });
        *s.me.borrow_mut() = Rc::downgrade(&s);
        s
    }

    /// Build the page from config.json and what's set up right now.
    pub fn refresh(&self) {
        widgets::clear(&self.body);
        self.setup.borrow_mut().clear();
        let config = Config::load(&self.ctx.paths.config());
        // The Mac's order.
        self.body.append(&self.you(&config));
        self.body.append(&self.set_up());
        self.body.append(&self.your_ai());
        self.keys.refresh();
        self.body.append(&self.keys.root);
        self.body.append(&self.screens(&config));
        self.body.append(&self.appearance());
        self.body.append(&self.every_day(&config));
        #[cfg(feature = "voice")]
        self.body.append(&self.language(&config));
        let never = vbox(0);
        *self.never.borrow_mut() = never.clone();
        let (outer, list) = widgets::group(words::NEVER_RECORD);
        list.set_visible(false);
        outer.append(&never);
        self.body.append(&outer);
        self.refresh_never();
        self.body.append(&self.updates());
        self.refresh_setup();
        self.set_ai();
    }

    /// Another section shows: stop listening for new shortcut keys.
    pub fn leave(&self) {
        self.keys.leave();
    }

    /// Check marks or buttons (every second while Settings shows).
    pub fn refresh_setup(&self) {
        for s in self.setup.borrow().iter() {
            let done = (s.done)();
            s.check.set_visible(done);
            s.button.set_visible(!done);
            // As on the Mac, the title only; a line under it only while something runs (a download).
            match (s.busy)() {
                Some((line, enabled)) if !done => {
                    s.row.set_subtitle(&line);
                    s.button.set_sensitive(enabled);
                }
                _ => {
                    s.row.set_subtitle("");
                    s.button.set_sensitive(true);
                }
            }
        }
    }

    /// You: "Your name" on the left, its field on the right; the note under the panel.
    fn you(&self, config: &Config) -> gtk::Box {
        let (outer, list) = widgets::group(words::YOU);
        let r = row(words::YOUR_NAME, "");
        let name = gtk::Entry::new();
        name.set_valign(gtk::Align::Center);
        name.set_size_request(220, -1);
        let shown = config.author.clone().unwrap_or_else(|| {
            ui::account_name(&env, &std::fs::read_to_string("/etc/passwd").unwrap_or_default())
        });
        name.set_text(&shown);
        let ctx = self.ctx.clone();
        name.connect_changed(move |e| ctx.set(cfg::KEY_AUTHOR, cfg::author_value(&e.text())));
        r.add_suffix(&name);
        r.set_activatable_widget(Some(&name));
        list.append(&r);
        outer.append(&note(words::NAME_HELP));
        outer
    }

    fn set_up(&self) -> gtk::Box {
        let (outer, group) = widgets::group(words::SET_UP);
        #[cfg(feature = "voice")]
        {
            let root = self.ctx.paths.root.clone();
            let me = self.me.borrow().clone();
            let state = self.download.clone();
            let row = self.setup_row(
                &group,
                "audio-input-microphone-symbolic",
                words::ROW_VOICE_MODEL,
                Box::new(move || crate::model::is_present(&crate::model::path(&root))),
                Box::new(move || state.line()),
            );
            row.connect_clicked(move |_| {
                if let Some(s) = me.upgrade() {
                    voice::ask(&s.ctx, &s.download);
                }
            });
        }
        self.connect_rows(&group);
        let login = self.setup_row(&group, "system-shutdown-symbolic", ROW_LOGIN, Box::new(|| autostart::is_enabled(&env)), Box::new(|| None));
        let me = self.me.borrow().clone();
        login.connect_clicked(move |_| {
            let Some(s) = me.upgrade() else { return };
            if autostart::enable(&env).is_err() {
                s.ctx.toast(ui::CANT_SAVE);
            }
            s.refresh_setup();
        });
        outer
    }

    /// "Connect your AI", in Set up: its row, then (inside the same panel, under its title) the apps and notes,
    /// which come with `set_ai`.
    fn connect_rows(&self, group: &gtk::ListBox) {
        let r = row(ROW_AI.title, "");
        r.set_tooltip_text(Some(ROW_AI.help));
        r.add_prefix(&setup_icon("insert-link-symbolic"));
        let check = done_check();
        let spinner = gtk::Spinner::new();
        spinner.set_valign(gtk::Align::Center);
        r.add_suffix(&check);
        r.add_suffix(&spinner);
        group.append(&r);
        let extra = vbox(6);
        extra.set_margin_start(SETUP_INDENT);
        extra.set_margin_end(14);
        extra.set_margin_bottom(10);
        let holder = gtk::ListBoxRow::new();
        holder.set_activatable(false);
        holder.set_selectable(false);
        holder.add_css_class("cc-sub");
        holder.set_child(Some(&extra));
        group.append(&holder);
        *self.connect.borrow_mut() = Some(ConnectRows { check, spinner, extra });
    }

    /// Settings → Your AI: what Make today's capsules uses (a pop-up), and the line under it.
    fn your_ai(&self) -> gtk::Box {
        let (outer, list) = widgets::group(anyai::YOUR_AI);
        let (r, combo) = combo_row(anyai::MAKE_WITH, &[], 0);
        let shown: Rc<RefCell<Vec<Choice>>> = Rc::default();
        let quiet: Rc<Cell<bool>> = Rc::default();
        let (choices, still, ctx) = (shown.clone(), quiet.clone(), self.ctx.clone());
        combo.connect_selected_notify(move |d| {
            if still.get() {
                return;
            }
            let picked = choices.borrow().get(d.selected() as usize).map(|c| c.id.clone());
            if let Some(id) = picked.filter(|id| *id != ai::state().choice) {
                if !ai::set_choice(&id) {
                    ctx.toast(ui::CANT_SAVE);
                }
            }
        });
        list.append(&r);
        let line = note("");
        outer.append(&line);
        *self.choice.borrow_mut() = Some(ChoiceRow { combo, line, shown, quiet });
        outer
    }

    /// Your AI changed: Connect your AI's rows, and the choice.
    pub fn set_ai(&self) {
        let s = ai::state();
        if let Some(c) = self.connect.borrow().as_ref() {
            c.check.set_visible(s.any_connected());
            c.spinner.set_visible(!s.loaded);
            c.spinner.set_spinning(!s.loaded);
            widgets::clear(&c.extra);
            for a in s.clients.iter().filter(|a| a.installed) {
                c.extra.append(&self.client_row(a, &s));
            }
            let missing: Vec<&AiClient> = s.clients.iter().filter(|a| !a.installed).collect();
            if !missing.is_empty() {
                c.extra.append(&self.more_row());
                if self.more.get() {
                    for a in missing {
                        let r = hbox(8);
                        r.add_css_class("dim-label");
                        let name = line(&a.name, &[]);
                        name.set_hexpand(true);
                        r.append(&name);
                        r.append(&line(anyai::NOT_INSTALLED, &["caption"]));
                        c.extra.append(&r);
                    }
                }
            }
            if let Some(n) = &s.note {
                c.extra.append(&label(n, &["dim-label", "caption"]));
            }
            c.extra.append(&label(anyai::ANY_OTHER_LINE, &["dim-label", "caption"]));
        }
        if let Some(c) = self.choice.borrow().as_ref() {
            let list = anyai::choices(&s.clients, &s.choice);
            c.quiet.set(true);
            if *c.shown.borrow() != list {
                let labels: Vec<&str> = list.iter().map(|c| c.label.as_str()).collect();
                c.combo.set_model(Some(&gtk::StringList::new(&labels)));
                *c.shown.borrow_mut() = list;
            }
            if let Some(i) = c.shown.borrow().iter().position(|x| x.id == s.choice) {
                if c.combo.selected() != i as u32 {
                    c.combo.set_selected(i as u32);
                }
            }
            c.quiet.set(false);
            c.line.set_text(&anyai::settings_line(&s.kind(), &s.name()));
        }
    }

    /// One AI app (a line under Connect your AI): Connect, or a ✓ and a small menu with Disconnect; a spinner
    /// while it's busy.
    fn client_row(&self, c: &AiClient, s: &ai::AiState) -> gtk::Widget {
        let r = hbox(8);
        let name = line(&c.name, &[]);
        name.set_hexpand(true);
        r.append(&name);
        if s.busy.as_deref() == Some(c.id.as_str()) {
            let spinner = gtk::Spinner::new();
            spinner.set_spinning(true);
            spinner.set_valign(gtk::Align::Center);
            r.append(&spinner);
        } else if c.connected {
            let check = done_check();
            check.set_tooltip_text(Some("Connected"));
            r.append(&check);
            let actions = gio::SimpleActionGroup::new();
            let a = gio::SimpleAction::new("disconnect", None);
            let (ctx, client) = (self.ctx.clone(), c.clone());
            a.connect_activate(move |_, _| ctx.disconnect_ai(&client));
            actions.add_action(&a);
            r.insert_action_group("ai", Some(&actions));
            let menu = gio::Menu::new();
            menu.append(Some(anyai::DISCONNECT), Some("ai.disconnect"));
            let more = gtk::MenuButton::new();
            more.set_icon_name("view-more-symbolic");
            more.add_css_class("flat");
            more.set_valign(gtk::Align::Center);
            more.set_menu_model(Some(&menu));
            more.set_tooltip_text(Some(anyai::MORE));
            r.append(&more);
        } else {
            let button = gtk::Button::with_label(anyai::CONNECT);
            button.set_valign(gtk::Align::Center);
            button.add_css_class("cc-small");
            button.set_sensitive(s.busy.is_none());
            let (ctx, client) = (self.ctx.clone(), c.clone());
            button.connect_clicked(move |_| ctx.connect_ai(&client));
            r.append(&button);
        }
        r.upcast()
    }

    /// "› More": a small disclosure that shows or hides the apps that aren't installed.
    fn more_row(&self) -> gtk::Widget {
        let inner = hbox(4);
        let chevron = gtk::Image::from_icon_name(if self.more.get() { "go-down-symbolic" } else { "go-next-symbolic" });
        chevron.set_pixel_size(10);
        inner.append(&chevron);
        inner.append(&gtk::Label::new(Some(anyai::MORE)));
        let b = gtk::Button::new();
        b.add_css_class("flat");
        b.add_css_class("cc-disclosure");
        b.set_child(Some(&inner));
        b.set_halign(gtk::Align::Start);
        let me = self.me.borrow().clone();
        b.connect_clicked(move |_| {
            let me = me.clone();
            // Not while the button is handling its own click: it is made again.
            glib::idle_add_local_once(move || {
                if let Some(s) = me.upgrade() {
                    s.more.set(!s.more.get());
                    s.set_ai();
                }
            });
        });
        b.upcast()
    }

    /// A Set up row as on the Mac: a small icon, the title (its help as a tooltip), then ✓ when done, else a small
    /// grey button.
    fn setup_row(
        &self,
        group: &gtk::ListBox,
        icon: &str,
        what: SetupRow,
        done: Box<dyn Fn() -> bool>,
        busy: Box<dyn Fn() -> Option<(String, bool)>>,
    ) -> gtk::Button {
        let r = row(what.title, "");
        r.set_subtitle_lines(3);
        r.set_tooltip_text(Some(what.help));
        r.add_prefix(&setup_icon(icon));
        let check = done_check();
        let button = gtk::Button::with_label(what.button);
        button.set_valign(gtk::Align::Center);
        button.add_css_class("cc-small");
        r.add_suffix(&check);
        r.add_suffix(&button);
        group.append(&r);
        self.setup.borrow_mut().push(Setup { row: r, button: button.clone(), check, done, busy });
        button
    }

    /// Screens: what starting shares.
    fn screens(&self, config: &Config) -> gtk::Box {
        let (outer, list) = widgets::group(ui::SCREENS);
        let shares: Vec<String> = ShareScreens::ALL.iter().map(|s| s.label().to_string()).collect();
        let (share_row, share) = combo_row(
            words::WHEN_YOU_START,
            &shares,
            ShareScreens::ALL.iter().position(|s| *s == config.share_screens).unwrap_or(0),
        );
        let ctx = self.ctx.clone();
        share.connect_selected_notify(move |r| {
            if let Some(share) = ShareScreens::ALL.get(r.selected() as usize) {
                ctx.set(cfg::KEY_SHARE_SCREENS, Some(share.value().into()));
            }
        });
        list.append(&share_row);
        outer
    }

    fn every_day(&self, config: &Config) -> gtk::Box {
        let (outer, group) = widgets::group(words::EVERY_DAY);

        let remind = switch_row(words::REMIND, config.remind_at.is_some());
        let times = cfg::remind_options(config.remind_at.as_deref());
        let current = cfg::normalize_time(config.remind_at.as_deref().unwrap_or(Config::REMIND_AT));
        let (at_title, at) = combo_row(words::REMIND_AT, &times, times.iter().position(|t| *t == current).unwrap_or(0));
        at.set_sensitive(config.remind_at.is_some());
        let (ctx, at_row, list) = (self.ctx.clone(), at.clone(), times.clone());
        remind.1.connect_active_notify(move |sw| {
            let time = list.get(at_row.selected() as usize).cloned().unwrap_or_else(|| Config::REMIND_AT.into());
            at_row.set_sensitive(sw.is_active());
            ctx.set(cfg::KEY_REMIND_AT, Some(ui::remind_value(sw.is_active(), &time)));
        });
        let (ctx, sw) = (self.ctx.clone(), remind.1.clone());
        at.connect_selected_notify(move |r| {
            if let Some(time) = times.get(r.selected() as usize) {
                if sw.is_active() {
                    ctx.set(cfg::KEY_REMIND_AT, Some(ui::remind_value(true, time)));
                }
            }
        });

        let keep = cfg::keep_options(config.keep_days);
        let labels: Vec<String> = keep.iter().map(|(_, l)| l.clone()).collect();
        let (keep_row, keep_list) = combo_row(words::KEEP, &labels, keep.iter().position(|(d, _)| *d == config.keep_days).unwrap_or(0));
        let ctx = self.ctx.clone();
        keep_list.connect_selected_notify(move |r| {
            if let Some((days, _)) = keep.get(r.selected() as usize) {
                ctx.set(cfg::KEY_KEEP_DAYS, Some((*days).into()));
            }
        });

        let sounds = switch_row(words::SOUNDS, config.sounds);
        let ctx = self.ctx.clone();
        sounds.1.connect_active_notify(move |sw| ctx.set(cfg::KEY_SOUNDS, Some(sw.is_active().into())));

        group.append(&remind.0);
        group.append(&at_title);
        group.append(&keep_row);
        group.append(&sounds.0);
        outer.append(&note(words::KEPT));
        outer
    }

    #[cfg(feature = "voice")]
    fn language(&self, config: &Config) -> gtk::Box {
        let (outer, group) = widgets::group(words::VOICE_NOTES);
        let current = config.voice_locale.as_deref();
        let options = cfg::language_options(current);
        let names: Vec<String> = options.iter().map(|(_, n)| n.clone()).collect();
        let (r, list) = combo_row(words::LANGUAGE, &names, cfg::language_index(current));
        let ctx = self.ctx.clone();
        list.connect_selected_notify(move |r| {
            if let Some((id, _)) = options.get(r.selected() as usize) {
                ctx.set(cfg::KEY_VOICE_LOCALE, ui::language_value(id));
            }
        });
        group.append(&r);
        outer
    }

    /// The never-record list, then "Add an app".
    fn refresh_never(&self) {
        let never = self.never.borrow().clone();
        widgets::clear(&never);
        let list = widgets::boxed_list();
        // As on the Mac: what's never recorded, as the panel's first line.
        list.append(&note_row(words::NEVER_RECORD_HELP));
        let apps = Config::load(&self.ctx.paths.config()).exclude_apps;
        let installed = installed_apps();
        for app in &apps {
            let name = ui::excluded_name(app, &installed);
            let r = row(&name, if name == *app { "" } else { app });
            let remove = widgets::flat_button(words::REMOVE);
            let (me, app) = (self.me.borrow().clone(), app.clone());
            remove.connect_clicked(move |_| {
                let Some(s) = me.upgrade() else { return };
                let list = cfg::remove_excluded(&Config::load(&s.ctx.paths.config()).exclude_apps, &app);
                s.ctx.set(cfg::KEY_EXCLUDE_APPS, Some(cfg::strings(&list)));
                s.refresh_never();
            });
            r.add_suffix(&remove);
            list.append(&r);
        }
        let add = row(words::ADD_APP, "");
        let choose = gtk::Button::with_label(words::CHOOSE);
        choose.set_valign(gtk::Align::Center);
        let me = self.me.borrow().clone();
        choose.connect_clicked(move |_| {
            if let Some(s) = me.upgrade() {
                s.choose_app();
            }
        });
        add.add_suffix(&choose);
        add.set_activatable_widget(Some(&choose));
        list.append(&add);
        never.append(&list);
    }

    /// A searchable list of installed apps; picking one adds it to the never-record list.
    fn choose_app(&self) {
        let apps = Rc::new(installed_apps_with_icons());
        let dialog = adw::Window::builder()
            .modal(true)
            .transient_for(&self.ctx.win)
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
            let r = row(name, id);
            let image = icon.as_ref().map_or_else(|| gtk::Image::from_icon_name("application-x-executable"), gtk::Image::from_gicon);
            image.set_pixel_size(32);
            r.add_prefix(&image);
            r.set_activatable(true);
            let (me, id, d) = (self.me.borrow().clone(), id.clone(), dialog.clone());
            r.connect_activated(move |_| {
                if let Some(s) = me.upgrade() {
                    let list = cfg::add_excluded(&Config::load(&s.ctx.paths.config()).exclude_apps, &id);
                    s.ctx.set(cfg::KEY_EXCLUDE_APPS, Some(cfg::strings(&list)));
                    s.refresh_never();
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
        let content = vbox(0);
        content.append(&header);
        content.append(&search);
        content.append(&scroll);
        dialog.set_content(Some(&content));
        dialog.present();
        search.grab_focus();
    }

    /// Updates: "Update automatically", the version, and "Check for updates".
    fn updates(&self) -> gtk::Box {
        use context_capsule_core::update::{AUTO_LABEL, CHECK_LABEL};
        let (outer, group) = widgets::group(words::UPDATES);
        let (auto, sw) = switch_row(AUTO_LABEL, crate::update::auto_update());
        let win = self.ctx.win.clone();
        sw.connect_active_notify(move |sw| {
            if !crate::update::set_auto_update(sw.is_active()) {
                widgets::message(&win, words::UPDATES, words::UPDATES_NOT_SAVED);
            }
        });
        group.append(&auto);
        let version = row(&words::version_line(env!("CARGO_PKG_VERSION")), "");
        let check = gtk::Button::with_label(CHECK_LABEL);
        check.set_valign(gtk::Align::Center);
        check.connect_clicked(|_| crate::update::check_now());
        version.add_suffix(&check);
        group.append(&version);
        outer
    }

    /// Look: Appearance as a segmented control (System / Light / Dark), as on the Mac.
    fn appearance(&self) -> gtk::Box {
        let (outer, group) = widgets::group(ui::LOOK);
        let r = row(words::APPEARANCE, "");
        let current = Appearance::load(&self.ctx.paths.config());
        let segments = hbox(0);
        segments.add_css_class("cc-segmented");
        segments.set_valign(gtk::Align::Center);
        let mut first: Option<gtk::ToggleButton> = None;
        for a in Appearance::ALL {
            let b = gtk::ToggleButton::with_label(a.label());
            if let Some(f) = &first {
                b.set_group(Some(f));
            }
            b.set_active(a == current);
            let ctx = self.ctx.clone();
            b.connect_toggled(move |b| {
                if b.is_active() {
                    ctx.set(cfg::KEY_APPEARANCE, Some(a.value().into()));
                    super::apply_appearance(a);
                }
            });
            segments.append(&b);
            first.get_or_insert(b);
        }
        r.add_suffix(&segments);
        group.append(&r);
        outer
    }
}

/// Set up rows: the icon's width, and where the lines under Connect your AI start (past the icon).
const SETUP_INDENT: i32 = 48;

/// A Set up row's small monochrome icon.
fn setup_icon(name: &str) -> gtk::Image {
    let i = gtk::Image::from_icon_name(name);
    i.add_css_class("dim-label");
    i.set_size_request(22, -1);
    i
}

/// The ✓ of a done row (grey, as on the Mac).
fn done_check() -> gtk::Image {
    let check = gtk::Image::from_icon_name("object-select-symbolic");
    check.add_css_class("success");
    check.set_tooltip_text(Some("Done"));
    check.set_valign(gtk::Align::Center);
    check
}

/// A row with a switch.
fn switch_row(title: &str, on: bool) -> (adw::ActionRow, gtk::Switch) {
    let r = row(title, "");
    let sw = gtk::Switch::new();
    sw.set_active(on);
    sw.set_valign(gtk::Align::Center);
    r.add_suffix(&sw);
    r.set_activatable_widget(Some(&sw));
    (r, sw)
}

/// A row with a compact pop-up on the right, as on the Mac.
fn combo_row(title: &str, options: &[String], selected: usize) -> (adw::ActionRow, gtk::DropDown) {
    let r = row(title, "");
    let refs: Vec<&str> = options.iter().map(String::as_str).collect();
    let d = gtk::DropDown::new(Some(gtk::StringList::new(&refs)), None::<gtk::Expression>);
    d.set_selected(selected as u32);
    d.set_valign(gtk::Align::Center);
    d.add_css_class("cc-popup");
    r.add_suffix(&d);
    r.set_activatable_widget(Some(&d));
    (r, d)
}

/// Installed apps that show in menus: (id for the never-record list, name).
fn installed_apps() -> Vec<(String, String)> {
    installed_apps_with_icons().into_iter().map(|(id, name, _)| (id, name)).collect()
}

fn installed_apps_with_icons() -> Vec<(String, String, Option<gio::Icon>)> {
    let mut apps: Vec<(String, String, Option<gio::Icon>)> = gio::AppInfo::all()
        .into_iter()
        .filter(|a| a.should_show())
        .filter_map(|a| Some((ui::desktop_app_id(a.id()?.as_str()), a.display_name().to_string(), a.icon())))
        .collect();
    apps.sort_by_key(|(_, name, _)| name.to_lowercase());
    apps.dedup_by(|a, b| a.0 == b.0);
    apps
}

/// A line of small text in a list (a note, "Any other AI…").
fn note_row(text: &str) -> gtk::Widget {
    let l = label(text, &["dim-label", "caption"]);
    l.set_margin_start(14);
    l.set_margin_end(14);
    l.set_margin_top(8);
    l.set_margin_bottom(8);
    let r = gtk::ListBoxRow::new();
    r.set_activatable(false);
    r.set_selectable(false);
    r.set_child(Some(&l));
    r.upcast()
}

/// Voice notes: the speech model, downloaded once after the user agrees.
#[cfg(feature = "voice")]
mod voice {
    use super::*;
    use crate::model;
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    enum State {
        #[default]
        Idle,
        Running(u64),
        Failed,
    }

    /// Shared with the download's thread; the Settings row reads it every second.
    #[derive(Clone, Default)]
    pub struct Download(Arc<Mutex<State>>);

    impl Download {
        /// The row's line while downloading (button off), or after it failed (button on).
        pub fn line(&self) -> Option<(String, bool)> {
            match *self.0.lock().unwrap() {
                State::Idle => None,
                State::Running(n) => Some((ui::download_line(n, model::BYTES), false)),
                State::Failed => Some((ui::DOWNLOAD_FAILED.to_string(), true)),
            }
        }

        fn running(&self) -> bool {
            matches!(*self.0.lock().unwrap(), State::Running(_))
        }
    }

    /// Ask first; on yes, download in a thread of its own.
    pub fn ask(ctx: &Ctx, download: &Download) {
        if download.running() {
            return;
        }
        let d = adw::MessageDialog::new(Some(&ctx.win), Some(ui::GET_VOICE_TITLE), Some(ui::GET_VOICE_BODY));
        d.add_response("no", ui::NOT_NOW);
        d.add_response("yes", ui::DOWNLOAD);
        d.set_response_appearance("yes", adw::ResponseAppearance::Suggested);
        d.set_default_response(Some("yes"));
        d.set_close_response("no");
        let (state, path) = (download.0.clone(), model::path(&ctx.paths.root));
        d.connect_response(None, move |_, response| {
            if response != "yes" {
                return;
            }
            *state.lock().unwrap() = State::Running(0);
            let (state, path) = (state.clone(), path.clone());
            std::thread::spawn(move || {
                let progress = state.clone();
                let result = model::download(&path, |n| *progress.lock().unwrap() = State::Running(n));
                if let Err(e) = &result {
                    eprintln!("Context Capsule: speech model: {e}");
                }
                *state.lock().unwrap() = if result.is_ok() { State::Idle } else { State::Failed };
            });
        });
        d.present();
    }
}
