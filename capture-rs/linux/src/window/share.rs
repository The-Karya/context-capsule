// context-capsule — linux: "Before you share" — before Copy file, Copy text or Save a copy…, a dialog lists
// what in the capsule might be private (People, Emails, Phone numbers, Private links), each with a check box
// (all ticked). Share uses a copy with the ticked items replaced by [removed]; the saved capsule never changes.
// Nothing found: straight through.

use super::widgets::vbox;
use super::Ctx;
use crate::ui;
use adw::prelude::*;
use context_capsule_core::share_check::{self, Kind};
use context_capsule_core::window as words;
use gtk::glib;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// How a capsule is shared.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum How {
    CopyFile,
    CopyText,
    SaveCopy,
}

/// Check, then share `path` as `how`.
pub fn check_then_share(ctx: &Rc<Ctx>, path: &Path, how: How) {
    let Ok(text) = std::fs::read_to_string(path) else {
        ctx.toast(ui::CANT_OPEN);
        return;
    };
    let items = share_check::find(&text).items();
    if items.is_empty() {
        share(ctx, path, how, None);
        return;
    }
    let d = adw::MessageDialog::new(Some(&ctx.win), Some(share_check::TITLE), Some(share_check::LINE));
    let list = vbox(18);
    let mut boxes: Vec<(gtk::CheckButton, String)> = vec![];
    for kind in Kind::ALL {
        let group_items: Vec<&String> = items.iter().filter(|(k, _)| *k == kind).map(|(_, s)| s).collect();
        if group_items.is_empty() {
            continue;
        }
        let group = adw::PreferencesGroup::new();
        group.set_title(kind.title());
        for item in group_items {
            let check = gtk::CheckButton::new();
            check.set_active(true);
            check.set_valign(gtk::Align::Center);
            let row = adw::ActionRow::new();
            row.set_use_markup(false);
            row.set_title(item);
            row.add_prefix(&check);
            row.set_activatable_widget(Some(&check));
            group.add(&row);
            boxes.push((check, item.clone()));
        }
        list.append(&group);
    }
    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .propagate_natural_height(true)
        .max_content_height(360)
        .min_content_width(360)
        .child(&list)
        .build();
    d.set_extra_child(Some(&scroll));
    d.add_response("cancel", share_check::CANCEL);
    d.add_response("share", share_check::SHARE);
    d.set_response_appearance("share", adw::ResponseAppearance::Suggested);
    d.set_default_response(Some("share"));
    d.set_close_response("cancel");
    let (cx, path) = (ctx.clone(), path.to_path_buf());
    d.connect_response(None, move |_, response| {
        if response != "share" {
            return;
        }
        let chosen: Vec<String> = boxes.iter().filter(|(b, _)| b.is_active()).map(|(_, s)| s.clone()).collect();
        share(&cx, &path, how, Some(share_check::remove(&text, &chosen)));
    });
    d.present();
}

/// Share the capsule: `text` is the copy with the checked items removed; None: the file as it is.
fn share(ctx: &Rc<Ctx>, path: &Path, how: How, text: Option<String>) {
    match how {
        How::CopyFile => {
            let file = match &text {
                None => Some(path.to_path_buf()),
                Some(t) => temp_copy(path, t),
            };
            match file {
                Some(f) => {
                    super::copy_file(&ctx.win, &f);
                    ctx.toast(words::COPIED_FILE);
                }
                None => ctx.toast(ui::CANT_SAVE),
            }
        }
        How::CopyText => match text.map(Ok).unwrap_or_else(|| std::fs::read_to_string(path)) {
            Ok(t) => {
                ctx.copy_text(&t);
                ctx.toast(words::COPIED_TEXT);
            }
            Err(_) => ctx.toast(ui::CANT_OPEN),
        },
        How::SaveCopy => super::capsules::save_copy(ctx, path, text),
    }
}

/// The shared copy as a file with the capsule's name, in a temporary folder of our own (it stays to be pasted;
/// the next share replaces it).
fn temp_copy(path: &Path, text: &str) -> Option<PathBuf> {
    let dir = glib::tmp_dir().join("context-capsule-shared");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).ok()?;
    let file = dir.join(path.file_name().unwrap_or_else(|| "capsule.capsule.md".as_ref()));
    std::fs::write(&file, text).ok()?;
    Some(file)
}
