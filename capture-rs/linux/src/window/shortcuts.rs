// context-capsule — linux: Settings → Shortcuts — "Start or stop" and "Speak a note", each a button showing its
// keys. Click it, press the new keys (Esc cancels): checked, grabbed, saved to config.json and used at once.
// Refused (the other one's keys, no Ctrl / Alt, another app has them): a short line under the row, the old keys
// stay. "Reset" puts back Alt+C and Alt+V. On Wayland the desktop binds them and has the last word.

use super::widgets::{self, flat_button, hbox, keycap, label, note, row, vbox};
use super::Ctx;
use crate::hotkeys::{self, Applied};
use crate::ui;
use adw::prelude::*;
use context_capsule_core::shortcuts::{self as words, Hotkeys, Pressed, Which};
use gtk::{gdk, gio, glib};
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

pub struct Shortcuts {
    ctx: Rc<Ctx>,
    pub root: gtk::Box,
    /// The line under the panel (how to change a shortcut, or "Press the new keys…").
    help: gtk::Label,
    rows: Vec<(Which, adw::ActionRow, gtk::Button)>,
    reset: gtk::Button,
    /// Listening for new keys for this one: the window's key controller, while it listens.
    listening: RefCell<Option<(Which, gtk::EventControllerKey)>>,
    /// A change is being applied (Wayland: the desktop may be asking).
    busy: Cell<bool>,
    me: RefCell<Weak<Shortcuts>>,
}

impl Shortcuts {
    pub fn new(ctx: &Rc<Ctx>) -> Rc<Shortcuts> {
        // As on the Mac: the rows in a panel, the help line under it, Reset beside the help line.
        let root = vbox(8);
        root.append(&widgets::heading(words::SHORTCUTS));
        let group = widgets::boxed_list();
        root.append(&group);
        let help = note(words::SHORTCUTS_HELP);
        help.set_hexpand(true);
        let reset = flat_button(words::RESET);
        let under = hbox(8);
        under.append(&help);
        under.append(&reset);
        let mut rows = vec![];
        for which in Which::ALL {
            let r = row(which.title(), "");
            let b = gtk::Button::new();
            b.set_valign(gtk::Align::Center);
            b.add_css_class("cc-keys");
            r.add_suffix(&b);
            r.set_activatable_widget(Some(&b));
            group.append(&r);
            rows.push((which, r, b));
        }
        root.append(&under);
        if ctx.hotkeys.on_portal() {
            let note = label(hotkeys::DESKTOP_NOTE, &["dim-label", "caption"]);
            note.set_margin_start(2);
            root.append(&note);
        }
        let s = Rc::new(Shortcuts {
            ctx: ctx.clone(),
            root,
            help,
            rows,
            reset,
            listening: RefCell::new(None),
            busy: Cell::new(false),
            me: RefCell::new(Weak::new()),
        });
        *s.me.borrow_mut() = Rc::downgrade(&s);
        for (which, _, b) in &s.rows {
            let (me, which) = (Rc::downgrade(&s), *which);
            b.connect_clicked(move |_| {
                if let Some(s) = me.upgrade() {
                    s.clicked(which);
                }
            });
        }
        let me = Rc::downgrade(&s);
        s.reset.connect_clicked(move |_| {
            if let Some(s) = me.upgrade() {
                s.reset();
            }
        });
        // The window went to the back or was closed: stop listening (the old keys work again).
        let me = Rc::downgrade(&s);
        ctx.win.connect_is_active_notify(move |w| {
            if let (false, Some(s)) = (w.is_active(), me.upgrade()) {
                s.stop(true);
            }
        });
        s.refresh();
        s
    }

    /// The current keys on the buttons, no lines under the rows.
    pub fn refresh(&self) {
        self.stop(true);
        self.show_keys();
        for (_, r, _) in &self.rows {
            r.set_subtitle("");
        }
    }

    /// Settings is left: stop listening.
    pub fn leave(&self) {
        self.stop(true);
    }

    fn show_keys(&self) {
        let h = hotkeys::current();
        for (which, _, b) in &self.rows {
            b.set_child(Some(&keycap(&h.get(*which).label())));
            b.set_sensitive(!self.busy.get());
        }
        self.reset.set_sensitive(!self.busy.get());
    }

    /// The short line under a row.
    fn say(&self, which: Which, line: &str) {
        if let Some((_, r, _)) = self.rows.iter().find(|(w, _, _)| *w == which) {
            r.set_subtitle(line);
        }
    }

    fn clicked(&self, which: Which) {
        if self.busy.get() {
            return;
        }
        let again = self.listening.borrow().as_ref().map(|(w, _)| *w) == Some(which);
        self.stop(true);
        if !again {
            self.listen(which);
        }
    }

    /// Wait for the new keys: the window takes every key press first (capture phase) until then.
    fn listen(&self, which: Which) {
        self.ctx.hotkeys.pause(); // X11: the old grab would take the keys before the window sees them
        for (w, r, b) in &self.rows {
            r.set_subtitle("");
            if *w == which {
                b.set_label(words::PRESS_KEYS);
            }
        }
        self.help.set_text(words::PRESS_HELP);
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let me = self.me.borrow().clone();
        keys.connect_key_pressed(move |_, keyval, keycode, state| {
            let Some(s) = me.upgrade() else { return glib::Propagation::Proceed };
            if s.listening.borrow().is_none() {
                return glib::Propagation::Proceed;
            }
            s.pressed(keyval, keycode, state);
            glib::Propagation::Stop
        });
        self.ctx.win.add_controller(keys.clone());
        *self.listening.borrow_mut() = Some((which, keys));
    }

    /// Stop listening. `resume`: hold the current keys again (nothing changed).
    fn stop(&self, resume: bool) {
        let Some((_, keys)) = self.listening.borrow_mut().take() else { return };
        // Not from inside the controller's own signal: on the next turn of the loop.
        let win = self.ctx.win.clone();
        glib::idle_add_local_once(move || win.remove_controller(&keys));
        self.help.set_text(words::SHORTCUTS_HELP);
        self.show_keys();
        if resume {
            self.ctx.hotkeys.resume(hotkeys::current());
        }
    }

    fn pressed(&self, keyval: gdk::Key, keycode: u32, state: gdk::ModifierType) {
        let Some(which) = self.listening.borrow().as_ref().map(|(w, _)| *w) else { return };
        let name = key_name(&self.ctx.win, keyval, keycode);
        let held = |m: gdk::ModifierType| state.contains(m);
        let press = ui::shortcut_press(
            &name,
            held(gdk::ModifierType::CONTROL_MASK),
            held(gdk::ModifierType::ALT_MASK),
            held(gdk::ModifierType::SHIFT_MASK),
            held(gdk::ModifierType::SUPER_MASK),
        );
        match press {
            Pressed::Wait => {}
            Pressed::Cancel => self.stop(true),
            Pressed::Done(keys) => {
                self.stop(false);
                let old = hotkeys::current();
                match old.check(which, keys, false) {
                    Err(refusal) => {
                        self.ctx.hotkeys.resume(old);
                        self.say(which, &refusal.message(false));
                    }
                    Ok(new) => self.change(old, new),
                }
            }
        }
    }

    fn reset(&self) {
        if self.busy.get() {
            return;
        }
        self.refresh();
        let old = hotkeys::current();
        if old != Hotkeys::DEFAULT {
            self.change(old, Hotkeys::DEFAULT);
        }
    }

    /// Grab `new` (off the GTK thread: the desktop may ask), then save it; refused: `old` stays.
    fn change(&self, old: Hotkeys, new: Hotkeys) {
        if new == old {
            self.ctx.hotkeys.resume(old);
            return;
        }
        self.busy.set(true);
        self.show_keys();
        let control = self.ctx.hotkeys.clone();
        let me = self.me.borrow().clone();
        glib::spawn_future_local(async move {
            let result = gio::spawn_blocking(move || control.apply(old, new)).await;
            let Some(s) = me.upgrade() else { return };
            s.busy.set(false);
            match result {
                Ok(Ok(applied)) => s.changed(new, applied),
                Ok(Err((which, refusal))) => s.say(which, &refusal.message(false)),
                Err(_) => s.ctx.hotkeys.resume(old),
            }
            s.show_keys();
        });
    }

    fn changed(&self, new: Hotkeys, applied: Applied) {
        if let Err(e) = new.save(&self.ctx.paths.config()) {
            eprintln!("Context Capsule: cannot save the shortcuts: {e}");
            self.ctx.toast(ui::CANT_SAVE);
        }
        self.ctx.set_keys(&new);
        if let Applied::Desktop(bound) = applied {
            for (which, described) in bound {
                if let Some(line) = hotkeys::desktop_line(&new.get(which), &described) {
                    self.say(which, &line);
                }
            }
        }
    }
}

/// The key's GDK name: its own ("c", "F5", "Escape"), or with Shift held the unshifted key's ("1", not "exclam").
fn key_name(win: &adw::ApplicationWindow, keyval: gdk::Key, keycode: u32) -> String {
    let own = keyval.to_lower().name().map(|n| n.to_string()).unwrap_or_default();
    if ui::key_from_name(&own).is_some() || own == "Escape" {
        return own;
    }
    WidgetExt::display(win)
        .map_keycode(keycode)
        .unwrap_or_default()
        .into_iter()
        .filter(|(k, _)| k.level() == 0)
        .filter_map(|(_, v)| v.name().map(|n| n.to_string()))
        .find(|n| ui::key_from_name(n).is_some())
        .unwrap_or(own)
}
