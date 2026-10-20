// context-capsule — linux: Help — the shortcuts, how it works, and what stays private. Short.

use super::widgets::{boxed_list, card, heading, keycap, label, line, row, vbox};
use super::Ctx;
use adw::prelude::*;
use context_capsule_core::window::{self as words, privacy, shortcuts, HOW};

pub fn page(ctx: &Ctx) -> gtk::Box {
    let root = vbox(24);
    root.append(&label(words::Section::Help.title(), &["title-1"]));

    let keys = vbox(8);
    keys.append(&heading(words::SHORTCUTS));
    let list = boxed_list();
    for (what, k) in shortcuts(&ctx.keys) {
        let r = row(what, "");
        if k.contains('+') {
            r.add_suffix(&keycap(k));
        } else {
            r.add_suffix(&line(k, &["dim-label"]));
        }
        list.append(&r);
    }
    keys.append(&list);
    root.append(&keys);

    for (title, lines) in [(words::HOW_IT_WORKS, HOW.map(String::from).to_vec()), (words::PRIVACY, privacy(&ctx.keys).to_vec())] {
        let section = vbox(8);
        section.append(&heading(title));
        let text = vbox(10);
        for l in lines {
            text.append(&label(&l, &[]));
        }
        section.append(&card(&text, 16));
        root.append(&section);
    }
    root
}
