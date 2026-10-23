// context-capsule — linux: Capsules — every capsule by project (the growing "Whole project" one first, then
// days), each with Open, and Copy file / Copy text / Save a copy… (after "Check before sharing") / Show in
// folder; "Make today's capsules", "Open a capsule…" (one someone sent), a search box with "Ask your AI" (or
// "Ask Claude"), and Copy my day / Save from clipboard for any other AI.

use super::share::{check_then_share, How};
use super::widgets::{self, hbox, label, line, row, vbox};
use super::Ctx;
use crate::{ai, ui};
use adw::prelude::*;
use context_capsule_core::anyai;
use context_capsule_core::capsules::{groups, search, today, CapsuleSummary};
use context_capsule_core::claude::search_prompt;
use context_capsule_core::window as words;
use context_capsule_core::{Project, Projects};
use gtk::{gio, glib};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

pub struct Capsules {
    ctx: Rc<Ctx>,
    pub root: gtk::Box,
    list: gtk::Box,
    search: gtk::SearchEntry,
    /// Every capsule and project, as last read (the search box filters these as you type).
    all: RefCell<(Vec<CapsuleSummary>, Vec<Project>)>,
    /// "Ask Claude" or "Ask your AI".
    ask: gtk::Button,
    /// After Copy my day, what to do next; else "Using an AI on the web…".
    day_line: gtk::Label,
    copy_day: gtk::Button,
    save: gtk::Button,
}

impl Capsules {
    pub fn new(ctx: &Rc<Ctx>) -> Rc<Capsules> {
        let root = vbox(24);
        let top = hbox(12);
        let title = label(words::Section::Capsules.title(), &["title-1"]);
        title.set_hexpand(true);
        let open = gtk::Button::with_label(words::OPEN_CAPSULE);
        open.set_valign(gtk::Align::Center);
        let c = ctx.clone();
        open.connect_clicked(move |_| choose_and_open(&c));
        let make = gtk::Button::with_label(words::MAKE_TODAYS);
        make.set_valign(gtk::Align::Center);
        let c = ctx.clone();
        make.connect_clicked(move |_| c.make_capsules());
        top.append(&title);
        top.append(&open);
        top.append(&make);
        root.append(&top);

        // Search as you type, or ask your AI (it reads inside every capsule).
        let ask_row = hbox(8);
        let search = gtk::SearchEntry::new();
        search.set_placeholder_text(Some(words::SEARCH_CAPSULES));
        search.set_hexpand(true);
        let ask = gtk::Button::with_label(anyai::ASK_YOUR_AI);
        ask.set_sensitive(false);
        ask_row.append(&search);
        ask_row.append(&ask);
        root.append(&ask_row);

        // Any other AI: Copy my day, then Save from clipboard.
        let copy_row = hbox(8);
        let day_line = label("", &["dim-label", "caption"]);
        day_line.set_hexpand(true);
        day_line.set_valign(gtk::Align::Center);
        let copy_day = gtk::Button::with_label(anyai::COPY_MY_DAY);
        copy_day.set_tooltip_text(Some(anyai::COPY_MY_DAY_HELP));
        copy_day.set_valign(gtk::Align::Center);
        let save = gtk::Button::with_label(anyai::SAVE_FROM_CLIPBOARD);
        save.set_tooltip_text(Some(anyai::SAVE_HELP));
        save.set_valign(gtk::Align::Center);
        copy_row.append(&day_line);
        copy_row.append(&copy_day);
        copy_row.append(&save);
        root.append(&copy_row);
        let c = ctx.clone();
        copy_day.connect_clicked(move |_| c.copy_my_day());
        let c = ctx.clone();
        save.connect_clicked(move |_| c.save_from_clipboard());

        let list = vbox(24);
        root.append(&list);
        let page = Rc::new(Capsules {
            ctx: ctx.clone(),
            root,
            list,
            search: search.clone(),
            all: RefCell::new((vec![], vec![])),
            ask: ask.clone(),
            day_line,
            copy_day,
            save,
        });
        page.set_ai();

        let weak = Rc::downgrade(&page);
        let ask_button = ask.clone();
        search.connect_search_changed(move |s| {
            ask_button.set_sensitive(!s.text().trim().is_empty());
            if let Some(p) = weak.upgrade() {
                p.show_list();
            }
        });
        let c = ctx.clone();
        let ask_now = move |s: &gtk::SearchEntry| {
            let q = s.text().trim().to_string();
            if !q.is_empty() {
                c.open_prompt(&search_prompt(&q), ai::WHY_SEARCH, ai::AGAIN_SEARCH);
            }
        };
        let (s, a) = (search.clone(), ask_now.clone());
        ask.connect_clicked(move |_| a(&s));
        search.connect_activate(move |s| ask_now(s));
        page
    }

    /// The Ask button and the Copy my day row, from Your AI.
    pub fn set_ai(&self) {
        let s = ai::state();
        self.ask.set_label(anyai::ask_button(&s.kind()));
        self.day_line.set_text(s.day_line.as_deref().unwrap_or(anyai::ON_THE_WEB));
        self.copy_day.set_sensitive(!s.working);
        self.save.set_sensitive(!s.working);
    }

    /// Read the capsules folder again.
    pub fn refresh(&self) {
        let all = CapsuleSummary::all(&self.ctx.paths.capsules());
        let projects = Projects::load(&self.ctx.paths.projects());
        *self.all.borrow_mut() = (all, projects);
        self.show_list();
    }

    /// The capsules that match the search box, by project.
    fn show_list(&self) {
        widgets::clear(&self.list);
        let (all, projects) = &*self.all.borrow();
        if all.is_empty() {
            self.list.append(&label(&words::no_capsules(&self.ctx.keys()), &["dim-label"]));
            return;
        }
        let shown = search(all, &self.search.text());
        if shown.is_empty() {
            self.list.append(&label(words::NO_MATCHES, &["dim-label"]));
            return;
        }
        let day = today();
        for g in groups(&shown, projects) {
            let group = adw::PreferencesGroup::new();
            group.set_title(&glib::markup_escape_text(&g.title));
            for c in &g.items {
                group.add(&capsule_row(&self.ctx, c, day));
            }
            self.list.append(&group);
        }
    }
}

/// "Draft", small.
pub fn draft_tag() -> gtk::Label {
    let t = line(words::DRAFT, &["tag", "caption"]);
    t.set_valign(gtk::Align::Center);
    t
}

/// What a menu item does with a capsule file.
type Action = Box<dyn Fn(&Rc<Ctx>, &Path)>;

fn capsule_row(ctx: &Rc<Ctx>, c: &CapsuleSummary, day: chrono::NaiveDate) -> adw::ActionRow {
    let r = row(&c.label(day), &c.in_short);
    r.set_subtitle_lines(2);
    r.add_css_class("cc-strong");
    // As on the Mac: a page for a day's capsule, a folder for the whole project's.
    let icon = gtk::Image::from_icon_name(if c.kind == "project" { "folder-symbolic" } else { "text-x-generic-symbolic" });
    icon.add_css_class("dim-label");
    icon.set_valign(gtk::Align::Start);
    icon.set_margin_top(14);
    r.add_prefix(&icon);
    let when = c.when(day);
    if !when.is_empty() {
        r.add_suffix(&line(&when, &["dim-label", "caption"]));
    }
    if c.draft {
        r.add_suffix(&draft_tag());
    }
    let open = widgets::flat_button(words::OPEN);
    let (cx, path) = (ctx.clone(), c.path.clone());
    open.connect_clicked(move |_| open_file(&cx, &path));
    r.add_suffix(&open);

    // The rest in a small menu.
    let actions = gio::SimpleActionGroup::new();
    let add = |name: &str, f: Action| {
        let a = gio::SimpleAction::new(name, None);
        let (cx, path) = (ctx.clone(), c.path.clone());
        a.connect_activate(move |_, _| f(&cx, &path));
        actions.add_action(&a);
    };
    // Sharing first checks for what might be private.
    add("copy-file", Box::new(|cx, p| check_then_share(cx, p, How::CopyFile)));
    add("copy-text", Box::new(|cx, p| check_then_share(cx, p, How::CopyText)));
    add("save", Box::new(|cx, p| check_then_share(cx, p, How::SaveCopy)));
    add("show", Box::new(show_in_folder));
    r.insert_action_group("capsule", Some(&actions));
    let menu = gio::Menu::new();
    menu.append(Some(words::COPY_FILE), Some("capsule.copy-file"));
    menu.append(Some(words::COPY_TEXT), Some("capsule.copy-text"));
    menu.append(Some(words::SAVE_COPY), Some("capsule.save"));
    menu.append(Some(ui::SHOW_IN_FOLDER), Some("capsule.show"));
    let more = gtk::MenuButton::new();
    more.set_icon_name("view-more-symbolic");
    more.add_css_class("flat");
    more.set_valign(gtk::Align::Center);
    more.set_menu_model(Some(&menu));
    more.set_tooltip_text(Some("More"));
    r.add_suffix(&more);
    r.set_activatable_widget(Some(&open));
    r
}

/// Open the capsule with the app for Markdown files.
pub fn open_file(ctx: &Ctx, path: &Path) {
    if !ctx.open_uri(&gio::File::for_path(path).uri()) {
        ctx.toast(ui::CANT_OPEN);
    }
}

/// "Open a capsule…": pick a .md file someone sent; Claude carries on from it.
fn choose_and_open(ctx: &Rc<Ctx>) {
    let chooser = gtk::FileChooserNative::new(Some(words::OPEN_CAPSULE.trim_end_matches('…')), Some(&ctx.win), gtk::FileChooserAction::Open, Some(words::OPEN), Some(words::CANCEL));
    let filter = gtk::FileFilter::new();
    filter.set_name(Some("Capsule (*.md)"));
    filter.add_pattern("*.md");
    filter.add_pattern("*.MD");
    filter.add_mime_type("text/markdown");
    chooser.add_filter(&filter);
    let keep: Rc<RefCell<Option<gtk::FileChooserNative>>> = Rc::new(RefCell::new(Some(chooser.clone())));
    let cx = ctx.clone();
    chooser.connect_response(move |d, response| {
        if response == gtk::ResponseType::Accept {
            if let Some(path) = d.file().and_then(|f| f.path()) {
                cx.open_received(&path);
            }
        }
        keep.borrow_mut().take();
    });
    chooser.show();
}

/// Save a copy where the user picks: `text` (the checked items removed), else the file as it is.
pub fn save_copy(ctx: &Rc<Ctx>, path: &Path, text: Option<String>) {
    let chooser = gtk::FileChooserNative::new(Some(words::SAVE_COPY.trim_end_matches('…')), Some(&ctx.win), gtk::FileChooserAction::Save, Some("Save"), Some(words::CANCEL));
    if let Some(name) = path.file_name() {
        chooser.set_current_name(&name.to_string_lossy());
    }
    // The dialog lives until it answers.
    let keep: Rc<RefCell<Option<gtk::FileChooserNative>>> = Rc::new(RefCell::new(Some(chooser.clone())));
    let (cx, from) = (ctx.clone(), path.to_path_buf());
    chooser.connect_response(move |d, response| {
        if response == gtk::ResponseType::Accept {
            if let Some(to) = d.file().and_then(|f| f.path()) {
                let saved = match &text {
                    Some(t) => std::fs::write(&to, t),
                    None => std::fs::copy(&from, &to).map(|_| ()),
                };
                match saved {
                    Ok(()) => cx.toast(ui::SAVED_COPY),
                    Err(_) => cx.toast(ui::CANT_SAVE),
                }
            }
        }
        keep.borrow_mut().take();
    });
    chooser.show();
}

/// Show the file in the file manager (selected); else open its folder.
fn show_in_folder(ctx: &Rc<Ctx>, path: &Path) {
    let uri = gio::File::for_path(path).uri().to_string();
    let folder: PathBuf = path.parent().map(Path::to_path_buf).unwrap_or_default();
    let fallback = {
        let cx = ctx.clone();
        move || {
            if !cx.open_uri(&gio::File::for_path(&folder).uri()) {
                cx.toast(ui::CANT_OPEN);
            }
        }
    };
    let Ok(bus) = gio::bus_get_sync(gio::BusType::Session, None::<&gio::Cancellable>) else {
        fallback();
        return;
    };
    let args = (vec![uri], String::new()).to_variant();
    bus.call(
        Some("org.freedesktop.FileManager1"),
        "/org/freedesktop/FileManager1",
        "org.freedesktop.FileManager1",
        "ShowItems",
        Some(&args),
        None,
        gio::DBusCallFlags::NONE,
        3000,
        None::<&gio::Cancellable>,
        move |result| {
            if result.is_err() {
                fallback();
            }
        },
    );
}
