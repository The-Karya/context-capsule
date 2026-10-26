// context-capsule — linux: what the shared window keeps native — the New project question, the voice model's
// download question, the installed apps (for the never-record list) and showing a file in the file manager.

use super::Ctx;
use crate::ui;
use adw::prelude::*;
use context_capsule_core::window as words;
use context_capsule_core::Msg;
use gtk::gio;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// "New project": its name (the window's title or site it was seen in, to start with); Add tells the engine.
pub fn ask_new_project(ctx: &Rc<Ctx>) {
    let clue = ctx.status.borrow().new_project_clue.clone();
    let d = adw::MessageDialog::new(Some(&ctx.win), Some(ui::NEW_PROJECT_TITLE), Some(&ui::new_project_body(clue.as_deref())));
    let entry = gtk::Entry::new();
    entry.set_text(clue.as_deref().unwrap_or(""));
    entry.set_activates_default(true);
    d.set_extra_child(Some(&entry));
    d.add_response("cancel", words::CANCEL);
    d.add_response("add", ui::ADD);
    d.set_response_appearance("add", adw::ResponseAppearance::Suggested);
    d.set_default_response(Some("add"));
    d.set_close_response("cancel");
    let ctx = ctx.clone();
    d.connect_response(None, move |_, response| {
        let name = entry.text().trim().to_string();
        if response == "add" && !name.is_empty() {
            ctx.send(Msg::NewProject { name, clue: clue.clone() });
        }
    });
    d.present();
}

/// The installed apps people can pick (desktop id, name, icon), by name.
pub fn installed_apps_with_icons() -> Vec<(String, String, Option<gio::Icon>)> {
    let mut apps: Vec<(String, String, Option<gio::Icon>)> = gio::AppInfo::all()
        .into_iter()
        .filter(|a| a.should_show())
        .filter_map(|a| Some((ui::desktop_app_id(a.id()?.as_str()), a.display_name().to_string(), a.icon())))
        .collect();
    apps.sort_by_key(|(_, name, _)| name.to_lowercase());
    apps.dedup_by(|a, b| a.0 == b.0);
    apps
}

/// Show the file in the file manager (selected); else open its folder.
pub fn show_in_folder(ctx: &Rc<Ctx>, path: &Path) {
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

/// Voice notes: the speech model, downloaded once after the user agrees (the page's Set up row shows it done).
#[cfg(feature = "voice")]
pub mod voice {
    use super::*;
    use crate::model;
    use std::sync::atomic::{AtomicBool, Ordering};

    /// A download is on (never two at once).
    static RUNNING: AtomicBool = AtomicBool::new(false);

    /// Ask first; on yes, download in a thread of its own.
    pub fn ask(ctx: &Ctx) {
        if RUNNING.load(Ordering::SeqCst) {
            return;
        }
        let d = adw::MessageDialog::new(Some(&ctx.win), Some(ui::GET_VOICE_TITLE), Some(ui::GET_VOICE_BODY));
        d.add_response("no", ui::NOT_NOW);
        d.add_response("yes", ui::DOWNLOAD);
        d.set_response_appearance("yes", adw::ResponseAppearance::Suggested);
        d.set_default_response(Some("yes"));
        d.set_close_response("no");
        let path = model::path(&ctx.paths.root);
        d.connect_response(None, move |_, response| {
            if response != "yes" || RUNNING.swap(true, Ordering::SeqCst) {
                return;
            }
            let path = path.clone();
            std::thread::spawn(move || {
                if let Err(e) = model::download(&path, |_| {}) {
                    eprintln!("Context Capsule: speech model: {e}");
                }
                RUNNING.store(false, Ordering::SeqCst);
            });
        });
        d.present();
    }
}
