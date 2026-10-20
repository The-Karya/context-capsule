// context-capsule — linux: Capsules — every capsule by project (the growing "Whole project" one first, then
// days), each with Open, and Copy file / Copy text / Save a copy… / Show in folder; "Make today's capsules".

use super::widgets::{self, hbox, label, line, row, vbox};
use super::Ctx;
use crate::ui;
use adw::prelude::*;
use context_capsule_core::capsules::{groups, today, CapsuleSummary};
use context_capsule_core::window as words;
use context_capsule_core::Projects;
use gtk::{gio, glib};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

pub struct Capsules {
    ctx: Rc<Ctx>,
    pub root: gtk::Box,
    list: gtk::Box,
}

impl Capsules {
    pub fn new(ctx: &Rc<Ctx>) -> Rc<Capsules> {
        let root = vbox(24);
        let top = hbox(12);
        let title = label(words::Section::Capsules.title(), &["title-1"]);
        title.set_hexpand(true);
        let make = gtk::Button::with_label(words::MAKE_TODAYS);
        make.add_css_class("suggested-action");
        make.set_valign(gtk::Align::Center);
        let c = ctx.clone();
        make.connect_clicked(move |_| c.make_capsules());
        top.append(&title);
        top.append(&make);
        root.append(&top);
        let list = vbox(24);
        root.append(&list);
        Rc::new(Capsules { ctx: ctx.clone(), root, list })
    }

    /// Read the capsules folder again.
    pub fn refresh(&self) {
        widgets::clear(&self.list);
        let all = CapsuleSummary::all(&self.ctx.paths.capsules());
        if all.is_empty() {
            self.list.append(&label(&words::no_capsules(&self.ctx.keys), &["dim-label"]));
            return;
        }
        let projects = Projects::load(&self.ctx.paths.projects());
        let day = today();
        for g in groups(&all, &projects) {
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
    add("copy-file", Box::new(|cx, p| {
        super::copy_file(&cx.win, p);
        cx.toast(words::COPIED_FILE);
    }));
    add("copy-text", Box::new(|cx, p| match std::fs::read_to_string(p) {
        Ok(text) => {
            cx.copy_text(&text);
            cx.toast(words::COPIED_TEXT);
        }
        Err(_) => cx.toast(ui::CANT_OPEN),
    }));
    add("save", Box::new(save_copy));
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

/// Save a copy where the user picks.
fn save_copy(ctx: &Rc<Ctx>, path: &Path) {
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
                match std::fs::copy(&from, &to) {
                    Ok(_) => cx.toast(ui::SAVED_COPY),
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
