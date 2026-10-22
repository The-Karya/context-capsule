// context-capsule — linux: Timeline — one day as it was recorded (Today, Yesterday or another day with a
// recording): a block per recording ("10:02 – 11:30 · 12 moments · Website", Make a capsule from this, ⋯ Delete),
// its moments (app, title, time, app · where · project, the text folded to three lines with Show all; "Looked at"
// for a window under the mouse), voice and typed notes as speech bubbles, the stretches that weren't read, and
// "Off" between recordings. Search filters the day. Today follows the recording. Nothing leaves the computer.
//! The words and the reading of the files are core's (`context_capsule_core::timeline`), the same as Windows.

use super::widgets::{self, boxed_list, flat_button, hbox, label, line, vbox};
use super::Ctx;
use crate::ai;
use adw::prelude::*;
use chrono::NaiveDate;
use context_capsule_core::capsules::today;
use context_capsule_core::timeline::{self as tl, Block, Day, Gap, Item, Moment, MomentKey, Row};
use gtk::{gio, glib};
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::{Rc, Weak};

/// The day menu shows at most this many days.
const DAYS_IN_MENU: usize = 30;

pub struct Timeline {
    ctx: Rc<Ctx>,
    pub root: gtk::Box,
    days: gtk::DropDown,
    days_model: gtk::StringList,
    /// The days in the menu, in its order.
    day_list: RefCell<Vec<NaiveDate>>,
    /// The menu is being filled (its "selected" changes are ours, not the user's).
    filling: Cell<bool>,
    search: gtk::SearchEntry,
    list: gtk::Box,
    date: Cell<NaiveDate>,
    day: RefCell<Option<Day>>,
    /// `day` with the search applied, while searching (kept, so a big day isn't searched again for each Show all).
    shown: RefCell<Option<Day>>,
    expanded: RefCell<HashSet<MomentKey>>,
    limit: Cell<usize>,
    me: RefCell<Weak<Timeline>>,
}

impl Timeline {
    pub fn new(ctx: &Rc<Ctx>) -> Rc<Timeline> {
        let root = vbox(18);
        root.append(&label(tl::TIMELINE, &["title-1"]));

        let top = hbox(8);
        let days_model = gtk::StringList::new(&[]);
        let days = gtk::DropDown::builder().model(&days_model).build();
        days.set_tooltip_text(Some(tl::PICK_A_DAY));
        days.set_valign(gtk::Align::Center);
        let search = gtk::SearchEntry::new();
        search.set_placeholder_text(Some(tl::SEARCH_DAY));
        search.set_hexpand(true);
        top.append(&days);
        top.append(&search);
        root.append(&top);
        root.append(&label(&tl::intro(ctx.keys().this_computer), &["dim-label", "caption"]));

        let list = vbox(18);
        root.append(&list);
        let page = Rc::new(Timeline {
            ctx: ctx.clone(),
            root,
            days: days.clone(),
            days_model,
            day_list: RefCell::new(vec![]),
            filling: Cell::new(false),
            search: search.clone(),
            list,
            date: Cell::new(today()),
            day: RefCell::new(None),
            shown: RefCell::new(None),
            expanded: RefCell::new(HashSet::new()),
            limit: Cell::new(tl::PAGE),
            me: RefCell::new(Weak::new()),
        });
        *page.me.borrow_mut() = Rc::downgrade(&page);
        page.fill_days(&[]);

        let weak = Rc::downgrade(&page);
        days.connect_selected_notify(move |dd| {
            let Some(p) = weak.upgrade() else { return };
            if p.filling.get() {
                return;
            }
            let picked = p.day_list.borrow().get(dd.selected() as usize).copied();
            if let Some(date) = picked {
                p.go_to(date);
            }
        });
        let weak = Rc::downgrade(&page);
        search.connect_search_changed(move |s| {
            let Some(p) = weak.upgrade() else { return };
            let q = s.text().to_string();
            *p.shown.borrow_mut() = if searching(&q) { p.day.borrow().as_ref().map(|d| d.search(&q)) } else { None };
            p.limit.set(tl::PAGE);
            p.show_list();
        });
        page
    }

    /// Showing the page: Home's "See today's timeline" asks for today; otherwise the same day, read again.
    pub fn refresh(&self) {
        if self.ctx.timeline_today.replace(false) {
            self.go_to(today());
        } else {
            self.load();
        }
    }

    /// Today is shown (it follows the recording).
    pub fn is_today(&self) -> bool {
        self.date.get() == today()
    }

    fn go_to(&self, date: NaiveDate) {
        if date != self.date.get() {
            self.date.set(date);
            *self.day.borrow_mut() = None;
            *self.shown.borrow_mut() = None;
            self.expanded.borrow_mut().clear();
            self.limit.set(tl::PAGE);
            self.show_list();
        }
        self.load();
    }

    /// The day menu: Today, Yesterday, then the other days with a recording; the shown one selected.
    fn fill_days(&self, recorded: &[NaiveDate]) {
        let now = today();
        let mut days = tl::picker_days(recorded, now, DAYS_IN_MENU);
        if !days.contains(&self.date.get()) {
            days.push(self.date.get());
        }
        let labels: Vec<String> = days.iter().map(|d| tl::day_label(*d, now)).collect();
        let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
        self.filling.set(true);
        if *self.day_list.borrow() != days {
            self.days_model.splice(0, self.days_model.n_items(), &refs);
            *self.day_list.borrow_mut() = days.clone();
        }
        if let Some(i) = days.iter().position(|d| *d == self.date.get()) {
            self.days.set_selected(i as u32);
        }
        self.filling.set(false);
    }

    /// Read the day off the GTK thread, then show it.
    pub fn load(&self) {
        let (paths, date) = (self.ctx.paths.clone(), self.date.get());
        let weak = self.me.borrow().clone();
        glib::spawn_future_local(async move {
            let read = gio::spawn_blocking(move || (tl::load_day(&paths, date), tl::days_with_recordings(&paths.sessions())));
            let Ok((day, recorded)) = read.await else {
                eprintln!("Context Capsule: couldn't read the timeline");
                return;
            };
            let Some(p) = weak.upgrade() else { return };
            if p.date.get() != date {
                return; // another day was picked meanwhile
            }
            let q = p.search.text().to_string();
            *p.shown.borrow_mut() = if searching(&q) { Some(day.search(&q)) } else { None };
            *p.day.borrow_mut() = Some(day);
            p.fill_days(&recorded);
            p.show_list();
        });
    }

    fn show_list(&self) {
        widgets::clear(&self.list);
        let (all, found) = (self.day.borrow(), self.shown.borrow());
        let is_searching = searching(&self.search.text());
        let shown = if is_searching { found.as_ref() } else { all.as_ref() };
        let Some(day) = shown else {
            self.list.append(&label(tl::LOADING, &["dim-label"]));
            return;
        };
        if day.is_empty() {
            let some = all.as_ref().is_some_and(|d| !d.is_empty());
            let text = tl::empty_line(some, self.is_today(), &self.ctx.keys().record);
            self.list.append(&label(&text, &["dim-label"]));
            return;
        }
        let projects = self.ctx.status.borrow().projects.clone();
        let (rows, more) = day.rows(self.limit.get());
        let mut i = 0;
        while i < rows.len() {
            match &rows[i] {
                Row::Off(text) => {
                    let off = line(text, &["dim-label", "caption", "numeric"]);
                    off.set_xalign(0.5);
                    self.list.append(&off);
                    i += 1;
                }
                Row::Block(b) => {
                    let block = vbox(8);
                    block.append(&self.block_header(b, &projects));
                    let items = boxed_list();
                    i += 1;
                    while let Some(Row::Item(_, item)) = rows.get(i) {
                        let child = match item {
                            Item::Gap(g) => gap_row(g),
                            Item::Moment(m) if m.is_note() => self.note_row(m),
                            Item::Moment(m) => self.moment_row(m, &projects),
                        };
                        let r = gtk::ListBoxRow::new();
                        r.set_activatable(false);
                        r.set_selectable(false);
                        r.set_child(Some(&child));
                        items.append(&r);
                        i += 1;
                    }
                    if items.first_child().is_some() {
                        block.append(&items);
                    } else {
                        let none = label(tl::NOTHING_IN_IT, &["dim-label", "caption"]);
                        none.set_margin_start(4);
                        block.append(&none);
                    }
                    self.list.append(&block);
                }
                Row::Item(..) => i += 1,
            }
        }
        if more {
            let b = gtk::Button::with_label(tl::SHOW_MORE);
            b.set_halign(gtk::Align::Start);
            let weak = self.me.borrow().clone();
            b.connect_clicked(move |_| {
                if let Some(p) = weak.upgrade() {
                    p.limit.set(p.limit.get() + tl::PAGE);
                    p.show_list();
                }
            });
            self.list.append(&b);
        }
    }

    /// "10:02 – 11:30 · 12 moments · Website" (a red dot and "Recording now" while it records), Make a capsule from
    /// this, and ⋯ (Delete this recording…).
    fn block_header(&self, b: &Block, projects: &[context_capsule_core::Project]) -> gtk::Box {
        let top = hbox(8);
        if b.live {
            let dot = line("●", &["rec-dot", "caption"]);
            dot.set_tooltip_text(Some(tl::RECORDING_NOW));
            top.append(&dot);
        }
        let title = line(&b.line(projects), &["heading", "numeric"]);
        top.append(&title);
        if b.live {
            top.append(&line(tl::RECORDING_NOW, &["dim-label", "caption"]));
        }
        let gap = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        gap.set_hexpand(true);
        top.append(&gap);
        let make = gtk::Button::with_label(tl::MAKE_FROM_THIS);
        make.set_tooltip_text(Some(tl::MAKE_FROM_THIS_HELP));
        make.set_valign(gtk::Align::Center);
        let (cx, id) = (self.ctx.clone(), b.session.clone());
        make.connect_clicked(move |_| cx.open_prompt(&tl::session_prompt(&id), ai::WHY_SESSION, ai::AGAIN_SESSION));
        top.append(&make);
        let (weak, id) = (self.me.borrow().clone(), b.session.clone());
        top.append(&more_menu(tl::DELETE_BLOCK_MENU, move || {
            if let Some(p) = weak.upgrade() {
                p.confirm_delete(&id, None);
            }
        }));
        top
    }

    /// Badge, the title (and "Looked at"), the time; app · where · project; the first lines (Show all); ⋯.
    fn moment_row(&self, m: &Moment, projects: &[context_capsule_core::Project]) -> gtk::Widget {
        let row = hbox(12);
        row.set_margin_top(10);
        row.set_margin_bottom(10);
        row.set_margin_start(12);
        row.set_margin_end(6);
        let badge = gtk::Label::new(Some(&initial(&m.app)));
        badge.add_css_class("app-badge");
        badge.set_valign(gtk::Align::Start);
        badge.set_tooltip_text(Some(m.app.as_str()));
        row.append(&badge);

        let body = vbox(3);
        body.set_hexpand(true);
        let head = hbox(8);
        let title = line(&m.heading(), &["heading"]);
        head.append(&title);
        if m.looked_at() {
            let tag = line(tl::LOOKED_AT, &["tag", "caption"]);
            tag.set_tooltip_text(Some(tl::LOOKED_AT_HELP));
            tag.set_valign(gtk::Align::Center);
            head.append(&tag);
        }
        let time = line(&m.time(), &["dim-label", "caption", "numeric"]);
        time.set_hexpand(true);
        time.set_xalign(1.0);
        head.append(&time);
        body.append(&head);
        let details = m.details(projects);
        if !details.is_empty() {
            let d = line(&details, &["dim-label", "caption"]);
            d.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
            body.append(&d);
        }
        let open = self.expanded.borrow().contains(&m.key());
        let text = m.shown_text(open);
        if text.trim().is_empty() {
            body.append(&line(tl::NOTHING_NEW, &["dim-label", "caption"]));
        } else {
            let t = label(&text, &["timeline-text"]);
            t.set_margin_top(2);
            t.set_selectable(true);
            if !open {
                t.set_lines((tl::FOLD_LINES * 2) as i32);
                t.set_ellipsize(gtk::pango::EllipsizeMode::End);
            }
            body.append(&t);
        }
        if open || m.folds() {
            let toggle = flat_button(if open { tl::SHOW_LESS } else { tl::SHOW_ALL });
            toggle.add_css_class("caption");
            toggle.set_halign(gtk::Align::Start);
            let (weak, key) = (self.me.borrow().clone(), m.key());
            toggle.connect_clicked(move |_| {
                let Some(p) = weak.upgrade() else { return };
                let mut ex = p.expanded.borrow_mut();
                if !ex.remove(&key) {
                    ex.insert(key.clone());
                }
                drop(ex);
                p.show_list();
            });
            body.append(&toggle);
        }
        row.append(&body);
        row.append(&self.moment_menu(m));
        row.upcast()
    }

    /// A voice or typed note: all its words in a speech bubble, then "You said · 10:14 · in Figma".
    fn note_row(&self, m: &Moment) -> gtk::Widget {
        let row = hbox(12);
        row.set_margin_top(10);
        row.set_margin_bottom(10);
        row.set_margin_start(12);
        row.set_margin_end(6);
        let icon = gtk::Image::from_icon_name(if m.kind == "voice" { "audio-input-microphone-symbolic" } else { "input-keyboard-symbolic" });
        icon.set_valign(gtk::Align::Start);
        icon.set_margin_top(8);
        row.append(&icon);
        let body = vbox(4);
        body.set_hexpand(true);
        let text = m.shown_text(true);
        let words = label(if text.is_empty() { "…" } else { text.as_str() }, &["bubble"]);
        words.set_selectable(true);
        words.set_halign(gtk::Align::Start);
        body.append(&words);
        let caption = line(&m.note_caption(), &["dim-label", "caption"]);
        caption.set_margin_start(4);
        body.append(&caption);
        row.append(&body);
        row.append(&self.moment_menu(m));
        row.upcast()
    }

    /// ⋯ → Delete this moment…
    fn moment_menu(&self, m: &Moment) -> gtk::MenuButton {
        let (weak, session, key) = (self.me.borrow().clone(), m.session.clone(), m.key());
        more_menu(tl::DELETE_MOMENT_MENU, move || {
            if let Some(p) = weak.upgrade() {
                p.confirm_delete(&session, Some(key.clone()));
            }
        })
    }

    /// Delete a moment (`key`) or the whole recording, after a confirm. Not while it's recording ("Stop first").
    fn confirm_delete(&self, session: &str, key: Option<MomentKey>) {
        let status = self.ctx.status.borrow().clone();
        if status.recording && status.session_id.as_deref() == Some(session) {
            let (title, body) = tl::Refused::Recording.message();
            widgets::message(&self.ctx.win, &title, &body);
            return;
        }
        let this = self.ctx.keys().this_computer;
        let (moment, heading, detail) = {
            let day = self.day.borrow();
            match &key {
                Some(k) => match day.as_ref().and_then(|d| d.find(k)).cloned() {
                    Some(m) => (Some(m), tl::DELETE_MOMENT, tl::delete_moment_detail(this)),
                    None => return,
                },
                None => match day.as_ref().and_then(|d| d.block(session)) {
                    Some(b) => (None, tl::DELETE_BLOCK, tl::delete_block_detail(b, this)),
                    None => return,
                },
            }
        };
        let d = adw::MessageDialog::new(Some(&self.ctx.win), Some(heading), Some(detail.as_str()));
        d.add_response("cancel", tl::CANCEL);
        d.add_response("delete", tl::DELETE);
        d.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
        d.set_default_response(Some("cancel"));
        d.set_close_response("cancel");
        let (weak, session) = (self.me.borrow().clone(), session.to_string());
        d.connect_response(None, move |_, response| {
            let Some(p) = weak.upgrade() else { return };
            if response != "delete" {
                return;
            }
            let done = match &moment {
                Some(m) => tl::delete_moment(&p.ctx.paths, &m.session, m.index, &m.t_raw, &m.line),
                None => tl::delete_session(&p.ctx.paths, &session),
            };
            if let Err(why) = done {
                let (title, body) = why.message();
                widgets::message(&p.ctx.win, &title, &body);
            }
            p.load();
        });
        d.present();
    }
}

/// Something is typed in the search box.
fn searching(q: &str) -> bool {
    q.split_whitespace().next().is_some()
}

/// "Not read 10:40 – 10:55: a private app was in front"
fn gap_row(g: &Gap) -> gtk::Widget {
    let row = hbox(12);
    row.set_margin_top(8);
    row.set_margin_bottom(8);
    row.set_margin_start(14);
    row.set_margin_end(12);
    let icon = gtk::Image::from_icon_name("view-conceal-symbolic");
    icon.add_css_class("dim-label");
    row.append(&icon);
    row.append(&label(&g.line(), &["dim-label", "caption"]));
    row.upcast()
}

/// A flat ⋯ button with one item.
fn more_menu(item: &str, on_pick: impl Fn() + 'static) -> gtk::MenuButton {
    let actions = gio::SimpleActionGroup::new();
    let a = gio::SimpleAction::new("pick", None);
    a.connect_activate(move |_, _| on_pick());
    actions.add_action(&a);
    let menu = gio::Menu::new();
    menu.append(Some(item), Some("timeline.pick"));
    let b = gtk::MenuButton::new();
    b.set_icon_name("view-more-symbolic");
    b.add_css_class("flat");
    b.set_valign(gtk::Align::Start);
    b.set_menu_model(Some(&menu));
    b.set_tooltip_text(Some("More"));
    b.insert_action_group("timeline", Some(&actions));
    b
}

/// The first letter of an app, for its badge.
pub fn initial(app: &str) -> String {
    app.trim().chars().find(|c| c.is_alphanumeric()).map(|c| c.to_uppercase().collect()).unwrap_or_else(|| "·".into())
}
