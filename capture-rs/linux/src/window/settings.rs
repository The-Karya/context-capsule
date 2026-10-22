// context-capsule — linux: Settings — your name; set up (voice notes, Connect your AI with its list of AI apps,
// open at login), each a check mark when done; Your AI (what Make today's capsules uses); every day (reminder,
// its time, deleting old recordings, sounds); your own shortcuts; the voice notes' language; apps never recorded;
// light or dark. Every change is saved at once (config.json, one key at a time).

use super::widgets::{self, label, row, vbox};
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
    help: &'static str,
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

/// "Connect your AI": a ✓ when one is connected (a spinner until the list is read); one row per app under it.
struct ConnectRows {
    expander: adw::ExpanderRow,
    check: gtk::Image,
    spinner: gtk::Spinner,
    rows: Vec<gtk::Widget>,
}

/// "Make today's capsules with" and the line under it.
struct ChoiceRow {
    combo: adw::ComboRow,
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
        self.body.append(&self.you(&config));
        self.body.append(&self.set_up());
        self.body.append(&self.your_ai());
        self.body.append(&self.every_day(&config));
        self.keys.refresh();
        self.body.append(&self.keys.root);
        #[cfg(feature = "voice")]
        self.body.append(&self.language(&config));
        let never = adw::PreferencesGroup::new();
        never.set_title(words::NEVER_RECORD);
        never.set_description(Some(words::NEVER_RECORD_HELP));
        let list = vbox(0);
        never.add(&list);
        *self.never.borrow_mut() = list;
        self.body.append(&never);
        self.refresh_never();
        self.body.append(&self.appearance());
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
            match (s.busy)() {
                Some((line, enabled)) if !done => {
                    s.row.set_subtitle(&line);
                    s.button.set_sensitive(enabled);
                }
                _ => {
                    s.row.set_subtitle(s.help);
                    s.button.set_sensitive(true);
                }
            }
        }
    }

    fn you(&self, config: &Config) -> adw::PreferencesGroup {
        let group = adw::PreferencesGroup::new();
        group.set_title(words::YOU);
        group.set_description(Some(words::NAME_HELP));
        let name = adw::EntryRow::new();
        name.set_title(words::YOUR_NAME);
        let shown = config.author.clone().unwrap_or_else(|| {
            ui::account_name(&env, &std::fs::read_to_string("/etc/passwd").unwrap_or_default())
        });
        name.set_text(&shown);
        let ctx = self.ctx.clone();
        name.connect_changed(move |e| ctx.set(cfg::KEY_AUTHOR, cfg::author_value(&e.text())));
        group.add(&name);
        group
    }

    fn set_up(&self) -> adw::PreferencesGroup {
        let group = adw::PreferencesGroup::new();
        group.set_title(words::SET_UP);
        #[cfg(feature = "voice")]
        {
            let root = self.ctx.paths.root.clone();
            let me = self.me.borrow().clone();
            let state = self.download.clone();
            let row = self.setup_row(
                &group,
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
        let login = self.setup_row(&group, ROW_LOGIN, Box::new(|| autostart::is_enabled(&env)), Box::new(|| None));
        let me = self.me.borrow().clone();
        login.connect_clicked(move |_| {
            let Some(s) = me.upgrade() else { return };
            if autostart::enable(&env).is_err() {
                s.ctx.toast(ui::CANT_SAVE);
            }
            s.refresh_setup();
        });
        group
    }

    /// "Connect your AI", in Set up (its rows come with `set_ai`).
    fn connect_rows(&self, group: &adw::PreferencesGroup) {
        let expander = adw::ExpanderRow::new();
        expander.set_use_markup(false);
        expander.set_title(ROW_AI.title);
        expander.set_subtitle(ROW_AI.help);
        expander.set_expanded(true);
        let check = gtk::Image::from_icon_name("object-select-symbolic");
        check.add_css_class("success");
        check.set_tooltip_text(Some("Done"));
        check.set_valign(gtk::Align::Center);
        let spinner = gtk::Spinner::new();
        spinner.set_valign(gtk::Align::Center);
        expander.add_action(&check);
        expander.add_action(&spinner);
        group.add(&expander);
        *self.connect.borrow_mut() = Some(ConnectRows { expander, check, spinner, rows: vec![] });
    }

    /// Settings → Your AI: what Make today's capsules uses.
    fn your_ai(&self) -> gtk::Box {
        let group = adw::PreferencesGroup::new();
        group.set_title(anyai::YOUR_AI);
        let combo = adw::ComboRow::new();
        combo.set_use_markup(false);
        combo.set_title(anyai::MAKE_WITH);
        let shown: Rc<RefCell<Vec<Choice>>> = Rc::default();
        let quiet: Rc<Cell<bool>> = Rc::default();
        let (list, still, ctx) = (shown.clone(), quiet.clone(), self.ctx.clone());
        combo.connect_selected_notify(move |r| {
            if still.get() {
                return;
            }
            let picked = list.borrow().get(r.selected() as usize).map(|c| c.id.clone());
            if let Some(id) = picked.filter(|id| *id != ai::state().choice) {
                if !ai::set_choice(&id) {
                    ctx.toast(ui::CANT_SAVE);
                }
            }
        });
        group.add(&combo);
        let line = label("", &["dim-label", "caption"]);
        line.set_margin_start(2);
        let b = vbox(8);
        b.append(&group);
        b.append(&line);
        *self.choice.borrow_mut() = Some(ChoiceRow { combo, line, shown, quiet });
        b
    }

    /// Your AI changed: Connect your AI's rows, and the choice.
    pub fn set_ai(&self) {
        let s = ai::state();
        if let Some(c) = self.connect.borrow_mut().as_mut() {
            c.check.set_visible(s.any_connected());
            c.spinner.set_visible(!s.loaded);
            c.spinner.set_spinning(!s.loaded);
            for w in c.rows.drain(..) {
                c.expander.remove(&w);
            }
            let mut rows: Vec<gtk::Widget> = s.clients.iter().filter(|a| a.installed).map(|a| self.client_row(a, &s)).collect();
            let missing: Vec<&AiClient> = s.clients.iter().filter(|a| !a.installed).collect();
            if !missing.is_empty() {
                rows.push(self.more_row());
                if self.more.get() {
                    for a in missing {
                        let r = row(&a.name, "");
                        r.add_css_class("dim-label");
                        r.add_suffix(&widgets::line(anyai::NOT_INSTALLED, &["caption"]));
                        rows.push(r.upcast());
                    }
                }
            }
            if let Some(note) = &s.note {
                rows.push(note_row(note));
            }
            rows.push(note_row(anyai::ANY_OTHER_LINE));
            for w in &rows {
                c.expander.add_row(w);
            }
            c.rows = rows;
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

    /// One AI app: Connect, or a ✓ and a small menu with Disconnect; a spinner while it's busy.
    fn client_row(&self, c: &AiClient, s: &ai::AiState) -> gtk::Widget {
        let r = row(&c.name, "");
        if s.busy.as_deref() == Some(c.id.as_str()) {
            let spinner = gtk::Spinner::new();
            spinner.set_spinning(true);
            spinner.set_valign(gtk::Align::Center);
            r.add_suffix(&spinner);
        } else if c.connected {
            let check = gtk::Image::from_icon_name("object-select-symbolic");
            check.add_css_class("success");
            check.set_tooltip_text(Some("Connected"));
            r.add_suffix(&check);
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
            r.add_suffix(&more);
        } else {
            let button = gtk::Button::with_label(anyai::CONNECT);
            button.set_valign(gtk::Align::Center);
            button.set_sensitive(s.busy.is_none());
            let (ctx, client) = (self.ctx.clone(), c.clone());
            button.connect_clicked(move |_| ctx.connect_ai(&client));
            r.add_suffix(&button);
        }
        r.upcast()
    }

    /// "More": shows or hides the apps that aren't installed.
    fn more_row(&self) -> gtk::Widget {
        let r = row(anyai::MORE, "");
        r.add_prefix(&gtk::Image::from_icon_name(if self.more.get() { "pan-down-symbolic" } else { "pan-end-symbolic" }));
        r.set_activatable(true);
        let me = self.me.borrow().clone();
        r.connect_activated(move |_| {
            let me = me.clone();
            // Not while the row is handling its own click: it is made again.
            glib::idle_add_local_once(move || {
                if let Some(s) = me.upgrade() {
                    s.more.set(!s.more.get());
                    s.set_ai();
                }
            });
        });
        r.upcast()
    }

    fn setup_row(
        &self,
        group: &adw::PreferencesGroup,
        what: SetupRow,
        done: Box<dyn Fn() -> bool>,
        busy: Box<dyn Fn() -> Option<(String, bool)>>,
    ) -> gtk::Button {
        let r = row(what.title, what.help);
        r.set_subtitle_lines(3);
        let check = gtk::Image::from_icon_name("object-select-symbolic");
        check.add_css_class("success");
        check.set_tooltip_text(Some("Done"));
        let button = gtk::Button::with_label(what.button);
        button.set_valign(gtk::Align::Center);
        r.add_suffix(&check);
        r.add_suffix(&button);
        group.add(&r);
        self.setup.borrow_mut().push(Setup { row: r, button: button.clone(), check, help: what.help, done, busy });
        button
    }

    fn every_day(&self, config: &Config) -> gtk::Box {
        let group = adw::PreferencesGroup::new();
        group.set_title(words::EVERY_DAY);

        let shares: Vec<String> = ShareScreens::ALL.iter().map(|s| s.label().to_string()).collect();
        let share_row = combo_row(
            words::WHEN_YOU_START,
            &shares,
            ShareScreens::ALL.iter().position(|s| *s == config.share_screens).unwrap_or(0),
        );
        let ctx = self.ctx.clone();
        share_row.connect_selected_notify(move |r| {
            if let Some(share) = ShareScreens::ALL.get(r.selected() as usize) {
                ctx.set(cfg::KEY_SHARE_SCREENS, Some(share.value().into()));
            }
        });

        let remind = switch_row(words::REMIND, config.remind_at.is_some());
        let times = cfg::remind_options(config.remind_at.as_deref());
        let current = cfg::normalize_time(config.remind_at.as_deref().unwrap_or(Config::REMIND_AT));
        let at = combo_row(words::REMIND_AT, &times, times.iter().position(|t| *t == current).unwrap_or(0));
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
        let keep_row = combo_row(words::KEEP, &labels, keep.iter().position(|(d, _)| *d == config.keep_days).unwrap_or(0));
        let ctx = self.ctx.clone();
        keep_row.connect_selected_notify(move |r| {
            if let Some((days, _)) = keep.get(r.selected() as usize) {
                ctx.set(cfg::KEY_KEEP_DAYS, Some((*days).into()));
            }
        });

        let sounds = switch_row(words::SOUNDS, config.sounds);
        let ctx = self.ctx.clone();
        sounds.1.connect_active_notify(move |sw| ctx.set(cfg::KEY_SOUNDS, Some(sw.is_active().into())));

        group.add(&share_row);
        group.add(&remind.0);
        group.add(&at);
        group.add(&keep_row);
        group.add(&sounds.0);
        let b = vbox(8);
        b.append(&group);
        let kept = label(words::KEPT, &["dim-label", "caption"]);
        kept.set_margin_start(2);
        b.append(&kept);
        b
    }

    #[cfg(feature = "voice")]
    fn language(&self, config: &Config) -> adw::PreferencesGroup {
        let group = adw::PreferencesGroup::new();
        group.set_title(words::VOICE_NOTES);
        let current = config.voice_locale.as_deref();
        let options = cfg::language_options(current);
        let names: Vec<String> = options.iter().map(|(_, n)| n.clone()).collect();
        let r = combo_row(words::LANGUAGE, &names, cfg::language_index(current));
        let ctx = self.ctx.clone();
        r.connect_selected_notify(move |r| {
            if let Some((id, _)) = options.get(r.selected() as usize) {
                ctx.set(cfg::KEY_VOICE_LOCALE, ui::language_value(id));
            }
        });
        group.add(&r);
        group
    }

    /// The never-record list, then "Add an app".
    fn refresh_never(&self) {
        let never = self.never.borrow().clone();
        widgets::clear(&never);
        let list = widgets::boxed_list();
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
    fn updates(&self) -> adw::PreferencesGroup {
        use context_capsule_core::update::{AUTO_LABEL, CHECK_LABEL};
        let group = adw::PreferencesGroup::new();
        group.set_title(words::UPDATES);
        let (auto, sw) = switch_row(AUTO_LABEL, crate::update::auto_update());
        let win = self.ctx.win.clone();
        sw.connect_active_notify(move |sw| {
            if !crate::update::set_auto_update(sw.is_active()) {
                widgets::message(&win, words::UPDATES, words::UPDATES_NOT_SAVED);
            }
        });
        group.add(&auto);
        let version = row(&words::version_line(env!("CARGO_PKG_VERSION")), "");
        let check = gtk::Button::with_label(CHECK_LABEL);
        check.set_valign(gtk::Align::Center);
        check.connect_clicked(|_| crate::update::check_now());
        version.add_suffix(&check);
        group.add(&version);
        group
    }

    fn appearance(&self) -> adw::PreferencesGroup {
        let group = adw::PreferencesGroup::new();
        group.set_title(words::APPEARANCE);
        let labels: Vec<String> = Appearance::ALL.iter().map(|a| a.label().to_string()).collect();
        let current = Appearance::load(&self.ctx.paths.config());
        let r = combo_row(words::APPEARANCE, &labels, Appearance::ALL.iter().position(|a| *a == current).unwrap_or(0));
        let ctx = self.ctx.clone();
        r.connect_selected_notify(move |r| {
            if let Some(a) = Appearance::ALL.get(r.selected() as usize) {
                ctx.set(cfg::KEY_APPEARANCE, Some(a.value().into()));
                super::apply_appearance(*a);
            }
        });
        group.add(&r);
        group
    }
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

/// A row with a drop-down list.
fn combo_row(title: &str, options: &[String], selected: usize) -> adw::ComboRow {
    let r = adw::ComboRow::new();
    r.set_title(title);
    let refs: Vec<&str> = options.iter().map(String::as_str).collect();
    r.set_model(Some(&gtk::StringList::new(&refs)));
    r.set_selected(selected as u32);
    r
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
    l.set_margin_start(12);
    l.set_margin_end(12);
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
