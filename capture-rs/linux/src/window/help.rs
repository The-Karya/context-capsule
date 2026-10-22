// context-capsule — linux: Help — the shortcuts (the current ones; making capsules follows Your AI), how it works,
// and what stays private. Short.

use super::widgets::{boxed_list, card, clear_list, heading, keycap, label, line, row, vbox};
use super::Ctx;
use adw::prelude::*;
use context_capsule_core::window::{self as words, privacy, shortcuts, HOW};

pub struct Help {
    pub root: gtk::Box,
    keys: gtk::ListBox,
    privacy: gtk::Box,
}

impl Help {
    pub fn new(ctx: &Ctx) -> Help {
        let root = vbox(24);
        root.append(&label(words::Section::Help.title(), &["title-1"]));

        let keys_box = vbox(8);
        keys_box.append(&heading(words::SHORTCUTS));
        let keys = boxed_list();
        keys_box.append(&keys);
        root.append(&keys_box);

        let how = vbox(8);
        how.append(&heading(words::HOW_IT_WORKS));
        let text = vbox(10);
        for l in HOW {
            text.append(&label(l, &[]));
        }
        how.append(&card(&text, 16));
        root.append(&how);

        let private = vbox(8);
        private.append(&heading(words::PRIVACY));
        let privacy = vbox(10);
        private.append(&card(&privacy, 16));
        root.append(&private);

        let help = Help { root, keys, privacy };
        help.refresh(ctx);
        help
    }

    /// The words that name the shortcuts, with the current ones.
    pub fn refresh(&self, ctx: &Ctx) {
        let keys = ctx.keys();
        clear_list(&self.keys);
        for (what, k) in shortcuts(&keys, &crate::ai::state().kind()) {
            let r = row(what, "");
            if k.contains('+') || k.starts_with('F') {
                r.add_suffix(&keycap(&k));
            } else {
                r.add_suffix(&line(&k, &["dim-label"]));
            }
            self.keys.append(&r);
        }
        super::widgets::clear(&self.privacy);
        for l in privacy(&keys) {
            self.privacy.append(&label(&l, &[]));
        }
    }
}
