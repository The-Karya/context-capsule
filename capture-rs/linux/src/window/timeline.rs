// context-capsule — linux: Timeline — one day as it was recorded, short enough to read at a glance: the day
// control (‹ the day › Today; the day opens a calendar with dots on days with a recording), "1 h 50 min recorded
// · 3 recordings", a day bar coloured by project, "Where your time went", then each recording folded to one line
// ("10:00 – 11:40 · 1 h 40 min · Website", Make a capsule, ⋯ Delete; open by itself while it records). Opened:
// its stretches (one window in a row: app · title, time and length, where · project, a peek; "24 reads" and Show
// what was read, folded to three lines with Show all), voice and typed notes as speech bubbles, and the stretches
// that weren't read. Search keeps the matches. Today follows the recording. Nothing leaves the computer.
//! The words, the grouping and the reading of the files are core's (`context_capsule_core::timeline`), the same
//! as Windows.

use super::widgets::{self, boxed_list, flat_button, hbox, heading, label, line, vbox};
use super::Ctx;
use crate::ai;
use adw::prelude::*;
use chrono::{Datelike, NaiveDate};
use context_capsule_core::capsules::{project_name, today};
use context_capsule_core::timeline::{self as tl, Block, Colour, Day, DayBar, Gap, Item, Moment, MomentKey, Stretch, StretchKey};
use context_capsule_core::Project;
use gtk::{cairo, gio, glib};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::{Rc, Weak};

/// The day bar: its height, and the hour labels' line under it.
const BAR_H: f64 = 14.0;
const HOURS_H: f64 = 18.0;
/// A stretch is at least this wide on the day bar (pixels).
const MIN_SEGMENT: f64 = 2.0;

/// What a ⋯ menu deletes.
#[derive(Clone)]
enum Target {
    Read(MomentKey),
    Note(MomentKey),
    Stretch(StretchKey),
    Recording(String),
}

pub struct Timeline {
    ctx: Rc<Ctx>,
    pub root: gtk::Box,
    /// The day control: ‹, the day (a calendar), ›, Today.
    before: gtk::Button,
    day_title: gtk::Label,
    calendar: gtk::Calendar,
    popover: gtk::Popover,
    after: gtk::Button,
    to_today: gtk::Button,
    /// The calendar is being set by us (its "day-selected" isn't the user's), and the day it showed last.
    syncing: Cell<bool>,
    shown_ymd: Cell<(i32, i32, i32)>,
    /// The days with a recording (the calendar's dots).
    recorded: RefCell<Vec<NaiveDate>>,
    summary: gtk::Label,
    search: gtk::SearchEntry,
    bar: gtk::DrawingArea,
    bar_data: RefCell<Option<DayBar>>,
    went: gtk::Box,
    list: gtk::Box,
    date: Cell<NaiveDate>,
    day: RefCell<Option<Day>>,
    /// `day` with the search applied, while searching (kept, so a big day isn't searched again for each Show all).
    shown: RefCell<Option<Day>>,
    /// Recordings the user opened or closed (open by default: the one recording; while searching, those with a match).
    toggled: RefCell<HashSet<String>>,
    /// Stretches whose reads are shown, and how many of them before "Show more".
    reads_open: RefCell<HashSet<StretchKey>>,
    limits: RefCell<HashMap<StretchKey, usize>>,
    /// Reads opened with Show all.
    expanded: RefCell<HashSet<MomentKey>>,
    /// Each shown stretch's row (the day bar scrolls to it).
    rows: RefCell<HashMap<StretchKey, gtk::Widget>>,
    me: RefCell<Weak<Timeline>>,
}

impl Timeline {
    pub fn new(ctx: &Rc<Ctx>) -> Rc<Timeline> {
        let root = vbox(18);
        let top = vbox(4);
        let control = hbox(4);
        let before = gtk::Button::from_icon_name("go-previous-symbolic");
        before.add_css_class("flat");
        before.set_tooltip_text(Some(tl::DAY_BEFORE));
        before.set_valign(gtk::Align::Center);
        let day_title = line("", &["title-2"]);
        let calendar = gtk::Calendar::new();
        let popover = gtk::Popover::new();
        popover.set_child(Some(&calendar));
        let pick = gtk::MenuButton::new();
        pick.set_child(Some(&day_title));
        pick.set_has_frame(false);
        pick.set_popover(Some(&popover));
        pick.set_tooltip_text(Some(tl::PICK_A_DAY));
        pick.set_valign(gtk::Align::Center);
        let after = gtk::Button::from_icon_name("go-next-symbolic");
        after.add_css_class("flat");
        after.set_tooltip_text(Some(tl::DAY_AFTER));
        after.set_valign(gtk::Align::Center);
        let to_today = gtk::Button::with_label(tl::TODAY);
        to_today.set_valign(gtk::Align::Center);
        to_today.set_margin_start(8);
        control.append(&before);
        control.append(&pick);
        control.append(&after);
        control.append(&to_today);
        let summary = label("", &["dim-label"]);
        summary.set_margin_start(6);
        top.append(&control);
        top.append(&summary);
        root.append(&top);
        let search = gtk::SearchEntry::new();
        search.set_placeholder_text(Some(tl::SEARCH_DAY));
        search.set_hexpand(true);
        root.append(&search);

        let bar = gtk::DrawingArea::new();
        bar.set_content_height((BAR_H + HOURS_H) as i32);
        bar.set_hexpand(true);
        bar.set_visible(false);
        root.append(&bar);
        let went = vbox(8);
        root.append(&went);
        let list = vbox(12);
        root.append(&list);
        let page = Rc::new(Timeline {
            ctx: ctx.clone(),
            root,
            before: before.clone(),
            day_title,
            calendar: calendar.clone(),
            popover: popover.clone(),
            after: after.clone(),
            to_today: to_today.clone(),
            syncing: Cell::new(false),
            shown_ymd: Cell::new((0, 0, 0)),
            recorded: RefCell::new(vec![]),
            summary,
            search: search.clone(),
            bar: bar.clone(),
            bar_data: RefCell::new(None),
            went,
            list,
            date: Cell::new(today()),
            day: RefCell::new(None),
            shown: RefCell::new(None),
            toggled: RefCell::new(HashSet::new()),
            reads_open: RefCell::new(HashSet::new()),
            limits: RefCell::new(HashMap::new()),
            expanded: RefCell::new(HashSet::new()),
            rows: RefCell::new(HashMap::new()),
            me: RefCell::new(Weak::new()),
        });
        *page.me.borrow_mut() = Rc::downgrade(&page);
        page.show_control();

        let weak = Rc::downgrade(&page);
        bar.set_draw_func(move |_, cr, w, _| {
            if let Some(p) = weak.upgrade() {
                if let Some(b) = p.bar_data.borrow().as_ref() {
                    draw_bar(cr, w as f64, b);
                }
            }
        });
        // A click on a stretch opens its recording and scrolls to it.
        let click = gtk::GestureClick::new();
        let weak = Rc::downgrade(&page);
        click.connect_pressed(move |_, _, x, _| {
            let Some(p) = weak.upgrade() else { return };
            let key = p.bar_data.borrow().as_ref().and_then(|b| segment_at(b, x, p.bar.width() as f64));
            if let Some(key) = key {
                p.reveal(&key);
            }
        });
        bar.add_controller(click);

        // The day control.
        let weak = Rc::downgrade(&page);
        before.connect_clicked(move |_| {
            if let Some(p) = weak.upgrade() {
                p.go_to(tl::day_before(p.date.get()));
            }
        });
        let weak = Rc::downgrade(&page);
        after.connect_clicked(move |_| {
            if let Some(p) = weak.upgrade() {
                if let Some(d) = tl::day_after(p.date.get(), today()) {
                    p.go_to(d);
                }
            }
        });
        let weak = Rc::downgrade(&page);
        to_today.connect_clicked(move |_| {
            if let Some(p) = weak.upgrade() {
                p.go_to(today());
            }
        });
        // The calendar opens on the shown day, with dots on the days with a recording in the month it shows.
        let weak = Rc::downgrade(&page);
        popover.connect_show(move |_| {
            if let Some(p) = weak.upgrade() {
                p.sync_calendar();
            }
        });
        for prop in ["month", "year"] {
            let weak = Rc::downgrade(&page);
            calendar.connect_notify_local(Some(prop), move |_, _| {
                if let Some(p) = weak.upgrade() {
                    p.mark_days();
                }
            });
        }
        let weak = Rc::downgrade(&page);
        calendar.connect_day_selected(move |cal| {
            let Some(p) = weak.upgrade() else { return };
            if p.syncing.get() {
                return;
            }
            let d = cal.date();
            let now = (d.year(), d.month(), d.day_of_month());
            let was = p.shown_ymd.replace(now);
            // ‹ › in the calendar move to the same day of another month: that's looking, not picking.
            if (now.0, now.1) != (was.0, was.1) && now.2 == was.2 {
                p.mark_days();
                return;
            }
            let picked = NaiveDate::from_ymd_opt(now.0, now.1 as u32, now.2 as u32);
            if let Some(date) = picked.filter(|d| *d <= today()) {
                p.popover.popdown();
                p.go_to(date);
            }
        });
        let weak = Rc::downgrade(&page);
        search.connect_search_changed(move |s| {
            let Some(p) = weak.upgrade() else { return };
            let q = s.text().to_string();
            *p.shown.borrow_mut() = if searching(&q) { p.day.borrow().as_ref().map(|d| d.search(&q)) } else { None };
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
            self.toggled.borrow_mut().clear();
            self.reads_open.borrow_mut().clear();
            self.limits.borrow_mut().clear();
            self.expanded.borrow_mut().clear();
            self.show_day();
        }
        self.load();
    }

    /// The day control: the day's name, › only up to today, Today only on another day.
    fn show_control(&self) {
        let (date, now) = (self.date.get(), today());
        self.day_title.set_text(&tl::day_title(date, now));
        self.after.set_sensitive(tl::day_after(date, now).is_some());
        self.to_today.set_visible(date != now);
        self.before.set_sensitive(tl::day_before(date) != date);
    }

    /// The calendar on the shown day.
    fn sync_calendar(&self) {
        let date = self.date.get();
        if let Ok(d) = glib::DateTime::from_local(date.year(), date.month() as i32, date.day() as i32, 12, 0, 0.0) {
            self.syncing.set(true);
            self.calendar.select_day(&d);
            self.syncing.set(false);
        }
        self.shown_ymd.set((date.year(), date.month() as i32, date.day() as i32));
        self.mark_days();
    }

    /// Dots on the days with a recording, in the month the calendar shows.
    fn mark_days(&self) {
        let d = self.calendar.date();
        self.calendar.clear_marks();
        for day in tl::recorded_in_month(&self.recorded.borrow(), d.year(), d.month() as u32) {
            self.calendar.mark_day(day);
        }
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
            *p.recorded.borrow_mut() = recorded;
            p.show_day();
        });
    }

    /// The header, the day bar and "Where your time went" (the whole day, whatever the search), then the list.
    fn show_day(&self) {
        self.show_control();
        widgets::clear(&self.went);
        {
            let day = self.day.borrow();
            self.summary.set_text(&day.as_ref().map(Day::summary).unwrap_or_default());
            self.summary.set_visible(day.is_some());
            *self.bar_data.borrow_mut() = day.as_ref().and_then(Day::bar);
            self.bar.set_visible(self.bar_data.borrow().is_some());
            self.bar.queue_draw();
            let projects = self.ctx.status.borrow().projects.clone();
            let shares = day.as_ref().map(|d| d.time_went(&projects)).unwrap_or_default();
            if !shares.is_empty() {
                self.went.append(&heading(tl::WHERE_TIME_WENT));
                let grid = gtk::Grid::new();
                grid.set_column_spacing(12);
                grid.set_row_spacing(10);
                for (i, s) in shares.iter().enumerate() {
                    // "Not sorted" or "Other" (named by core): grey.
                    let classes: &[&str] = if s.colour == Colour::Grey { &["dim-label"] } else { &[] };
                    let name = line(&s.name, classes);
                    name.set_width_chars(12);
                    name.set_max_width_chars(22);
                    let time = line(&s.time, &["dim-label", "numeric", "caption"]);
                    time.set_xalign(1.0);
                    time.set_width_chars(10);
                    grid.attach(&name, 0, i as i32, 1, 1);
                    grid.attach(&share_bar(s.share, s.colour), 1, i as i32, 1, 1);
                    grid.attach(&time, 2, i as i32, 1, 1);
                }
                self.went.append(&grid);
            }
        }
        self.show_list();
    }

    /// The recordings (the search applied), keeping the scroll position.
    fn show_list(&self) {
        let scroll = self.scroller();
        let at = scroll.as_ref().map(|s| s.vadjustment().value());
        widgets::clear(&self.list);
        self.rows.borrow_mut().clear();
        self.fill_list();
        if let (Some(s), Some(at)) = (scroll, at) {
            // After GTK has measured the new rows.
            glib::idle_add_local_once(move || s.vadjustment().set_value(at));
        }
    }

    fn fill_list(&self) {
        let (all, found) = (self.day.borrow(), self.shown.borrow());
        let is_searching = searching(&self.search.text());
        let shown = if is_searching { found.as_ref() } else { all.as_ref() };
        let Some(day) = shown else {
            self.list.append(&label(tl::LOADING, &["dim-label"]));
            return;
        };
        if day.is_empty() {
            let text = tl::empty_line(false, self.is_today(), &self.ctx.keys().record);
            self.list.append(&label(&text, &["dim-label"]));
            return;
        }
        let projects = self.ctx.status.borrow().projects.clone();
        self.list.append(&heading(tl::RECORDINGS));
        // Searching keeps every recording; those without a match stay folded.
        if is_searching && day.blocks.iter().all(|b| b.items.is_empty()) {
            self.list.append(&label(tl::NO_MATCHES, &["dim-label"]));
        }
        for b in &day.blocks {
            let open = default_open(b, is_searching) != self.toggled.borrow().contains(&b.session);
            let block = vbox(8);
            block.append(&self.block_header(b, open, &projects));
            if open {
                let items = boxed_list();
                for item in &b.items {
                    let child = match item {
                        Item::Gap(g) => gap_row(g),
                        Item::Note(m) => self.note_row(m),
                        Item::Stretch(s) => {
                            // The colour of the whole day (the same as the day bar's).
                            let colour = all.as_ref().map_or(Colour::Grey, |d| d.colour(s));
                            let w = self.stretch_row(s, colour, &projects);
                            self.rows.borrow_mut().insert(s.key(), w.clone());
                            w
                        }
                    };
                    let r = gtk::ListBoxRow::new();
                    r.set_activatable(false);
                    r.set_selectable(false);
                    r.set_child(Some(&child));
                    items.append(&r);
                }
                if items.first_child().is_some() {
                    block.append(&items);
                } else {
                    let none = label(if is_searching { tl::NO_MATCHES } else { tl::NOTHING_IN_IT }, &["dim-label", "caption"]);
                    none.set_margin_start(4);
                    block.append(&none);
                }
            }
            self.list.append(&block);
        }
    }

    /// The page's scrolled window.
    fn scroller(&self) -> Option<gtk::ScrolledWindow> {
        self.root.ancestor(gtk::ScrolledWindow::static_type()).and_then(|w| w.downcast::<gtk::ScrolledWindow>().ok())
    }

    /// The day bar was clicked: open that stretch's recording and scroll to it.
    fn reveal(&self, key: &StretchKey) {
        let is_searching = searching(&self.search.text());
        let shown = if is_searching { self.shown.borrow() } else { self.day.borrow() };
        let by_default = shown.as_ref().and_then(|d| d.block(&key.0)).is_some_and(|b| default_open(b, is_searching));
        drop(shown);
        {
            let mut toggled = self.toggled.borrow_mut();
            let open = by_default != toggled.contains(&key.0);
            if !open && !toggled.remove(&key.0) {
                toggled.insert(key.0.clone());
            }
        }
        widgets::clear(&self.list);
        self.rows.borrow_mut().clear();
        self.fill_list();
        let (weak, key) = (self.me.borrow().clone(), key.clone());
        glib::idle_add_local_once(move || {
            let Some(p) = weak.upgrade() else { return };
            let (Some(scroll), Some(row)) = (p.scroller(), p.rows.borrow().get(&key).cloned()) else { return };
            let Some(view) = scroll.child() else { return };
            if let Some((_, y)) = row.translate_coordinates(&view, 0.0, 0.0) {
                let adj = scroll.vadjustment();
                adj.set_value(adj.value() + y - 12.0);
            }
        });
    }

    /// Folded: ▸ "10:00 – 11:40 · 1 h 40 min · Website" (a red dot and "Recording now" while it records), Make a
    /// capsule, and ⋯ (Delete this recording…). A click on the line opens or closes it.
    fn block_header(&self, b: &Block, open: bool, projects: &[Project]) -> gtk::Box {
        let top = hbox(8);
        let fold = hbox(8);
        fold.append(&gtk::Image::from_icon_name(if open { "pan-down-symbolic" } else { "pan-end-symbolic" }));
        if b.live {
            let dot = line("●", &["rec-dot", "caption"]);
            dot.set_tooltip_text(Some(tl::RECORDING_NOW));
            fold.append(&dot);
        }
        fold.append(&line(&b.line(projects), &["heading", "numeric"]));
        if b.live {
            fold.append(&line(tl::RECORDING_NOW, &["dim-label", "caption"]));
        }
        let toggle = gtk::Button::new();
        toggle.add_css_class("flat");
        toggle.set_child(Some(&fold));
        toggle.set_hexpand(true);
        toggle.set_halign(gtk::Align::Fill);
        let (weak, id) = (self.me.borrow().clone(), b.session.clone());
        toggle.connect_clicked(move |_| {
            let Some(p) = weak.upgrade() else { return };
            let mut t = p.toggled.borrow_mut();
            if !t.remove(&id) {
                t.insert(id.clone());
            }
            drop(t);
            p.show_list();
        });
        top.append(&toggle);
        let make = gtk::Button::with_label(tl::MAKE_CAPSULE);
        make.set_tooltip_text(Some(tl::MAKE_CAPSULE_HELP));
        make.set_valign(gtk::Align::Center);
        let (cx, id) = (self.ctx.clone(), b.session.clone());
        make.connect_clicked(move |_| cx.open_prompt(&tl::session_prompt(&id), ai::WHY_SESSION, ai::AGAIN_SESSION));
        top.append(&make);
        top.append(&self.menu(tl::DELETE_BLOCK_MENU, &b.session, Target::Recording(b.session.clone())));
        top
    }

    /// Badge, a dot in its project's colour, app · title, time · length; where · project; the peek (2 lines);
    /// "24 reads" and Show what was read (the reads); ⋯.
    fn stretch_row(&self, s: &Stretch, colour: Colour, projects: &[Project]) -> gtk::Widget {
        let outer = vbox(0);
        let row = hbox(12);
        row.set_margin_top(10);
        row.set_margin_bottom(6);
        row.set_margin_start(12);
        row.set_margin_end(6);
        let badge = gtk::Label::new(Some(&initial(&s.app)));
        badge.add_css_class("app-badge");
        badge.set_valign(gtk::Align::Start);
        badge.set_tooltip_text(Some(s.app.as_str()));
        row.append(&badge);

        let body = vbox(3);
        body.set_hexpand(true);
        let head = hbox(8);
        head.append(&dot(colour));
        let title = line(&s.heading(), &["heading"]);
        title.set_hexpand(true);
        head.append(&title);
        head.append(&line(&format!("{} · {}", s.time(), s.length()), &["dim-label", "caption", "numeric"]));
        body.append(&head);
        let mut parts = vec![];
        if !s.location.is_empty() {
            parts.push(s.location.clone());
        }
        if !s.project.is_empty() {
            parts.push(project_name(projects, &s.project));
        }
        if !parts.is_empty() {
            let d = line(&parts.join(" · "), &["dim-label", "caption"]);
            d.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
            body.append(&d);
        }
        if let Some(peek) = &s.peek {
            let p = label(peek, &["dim-label", "timeline-text"]);
            p.set_lines(2);
            p.set_ellipsize(gtk::pango::EllipsizeMode::End);
            body.append(&p);
        }
        let key = s.key();
        let reads_open = self.reads_open.borrow().contains(&key);
        let reads_line = hbox(8);
        reads_line.append(&line(&s.reads_line(), &["dim-label", "caption"]));
        let toggle = flat_button(if reads_open { tl::HIDE_READS } else { tl::SHOW_READS });
        toggle.add_css_class("caption");
        let (weak, k) = (self.me.borrow().clone(), key.clone());
        toggle.connect_clicked(move |_| {
            let Some(p) = weak.upgrade() else { return };
            let mut open = p.reads_open.borrow_mut();
            if !open.remove(&k) {
                open.insert(k.clone());
            }
            drop(open);
            p.show_list();
        });
        reads_line.append(&toggle);
        body.append(&reads_line);
        row.append(&body);
        row.append(&self.menu(tl::DELETE_STRETCH_MENU, &s.session, Target::Stretch(key.clone())));
        outer.append(&row);

        if reads_open {
            let reads = vbox(0);
            reads.set_margin_start(48);
            reads.set_margin_bottom(8);
            let limit = self.limits.borrow().get(&key).copied().unwrap_or(tl::PAGE);
            let shown = s.shown_reads();
            for (i, m) in shown.iter().take(limit).enumerate() {
                if i > 0 {
                    reads.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
                }
                reads.append(&self.read_row(m));
            }
            if shown.len() > limit {
                let more = flat_button(tl::SHOW_MORE);
                more.add_css_class("caption");
                more.set_halign(gtk::Align::Start);
                let weak = self.me.borrow().clone();
                more.connect_clicked(move |_| {
                    if let Some(p) = weak.upgrade() {
                        p.limits.borrow_mut().insert(key.clone(), limit + tl::PAGE);
                        p.show_list();
                    }
                });
                reads.append(&more);
            }
            outer.append(&reads);
        }
        outer.upcast()
    }

    /// One read: the time, where, "Looked at"; the first lines (Show all); ⋯.
    fn read_row(&self, m: &Moment) -> gtk::Widget {
        let row = hbox(8);
        row.set_margin_top(6);
        row.set_margin_bottom(6);
        let body = vbox(3);
        body.set_hexpand(true);
        let head = hbox(8);
        head.append(&line(&m.time(), &["caption", "numeric"]));
        if !m.location.is_empty() {
            let w = line(&m.location, &["dim-label", "caption"]);
            w.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
            head.append(&w);
        }
        if m.looked_at() {
            let tag = line(tl::LOOKED_AT, &["tag", "caption"]);
            tag.set_tooltip_text(Some(tl::LOOKED_AT_HELP));
            tag.set_valign(gtk::Align::Center);
            head.append(&tag);
        }
        body.append(&head);
        let open = self.expanded.borrow().contains(&m.key());
        let text = m.shown_text(open);
        if text.trim().is_empty() {
            body.append(&line(tl::NOTHING_NEW, &["dim-label", "caption"]));
        } else {
            let t = label(&text, &["timeline-text"]);
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
        row.append(&self.menu(tl::DELETE_READ_MENU, &m.session, Target::Read(m.key())));
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
        row.append(&self.menu(tl::delete_note_menu(m), &m.session, Target::Note(m.key())));
        row.upcast()
    }

    /// ⋯ → one "Delete this …" item.
    fn menu(&self, item: &str, session: &str, target: Target) -> gtk::MenuButton {
        let (weak, session) = (self.me.borrow().clone(), session.to_string());
        more_menu(item, move || {
            if let Some(p) = weak.upgrade() {
                p.confirm_delete(&session, target.clone());
            }
        })
    }

    /// Delete a read, a note, a stretch or a whole recording, after a confirm. Not while it's recording
    /// ("Stop first"). What's deleted is looked up in the whole day (a searched stretch lists only some reads).
    fn confirm_delete(&self, session: &str, target: Target) {
        let status = self.ctx.status.borrow().clone();
        if status.recording && status.session_id.as_deref() == Some(session) {
            let (title, body) = tl::Refused::Recording.message();
            widgets::message(&self.ctx.win, &title, &body);
            return;
        }
        let this = self.ctx.keys().this_computer;
        enum What {
            Read(Moment),
            Stretch(Stretch),
            Recording,
        }
        let (what, heading, detail) = {
            let day = self.day.borrow();
            let Some(day) = day.as_ref() else { return };
            match &target {
                Target::Read(k) | Target::Note(k) => {
                    let Some(m) = day.find(k).cloned() else { return };
                    let heading = if matches!(target, Target::Note(_)) { tl::delete_note_question(&m) } else { tl::DELETE_READ };
                    (What::Read(m), heading, tl::delete_read_detail(this))
                }
                Target::Stretch(k) => {
                    let Some(s) = day.stretch(k).cloned() else { return };
                    let detail = tl::delete_stretch_detail(&s, this);
                    (What::Stretch(s), tl::DELETE_STRETCH, detail)
                }
                Target::Recording(id) => {
                    let Some(b) = day.block(id) else { return };
                    (What::Recording, tl::DELETE_BLOCK, tl::delete_block_detail(b, this))
                }
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
            let done = match &what {
                What::Read(m) => tl::delete_read(&p.ctx.paths, m),
                What::Stretch(s) => tl::delete_stretch(&p.ctx.paths, s),
                What::Recording => tl::delete_session(&p.ctx.paths, &session),
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

/// Open unless the user closed it: the one recording; while searching, those with a match.
fn default_open(b: &Block, searching: bool) -> bool {
    if searching {
        !b.items.is_empty()
    } else {
        b.live
    }
}

/// A small dot in a project's colour (a stretch's, before its name).
fn dot(colour: Colour) -> gtk::DrawingArea {
    let area = gtk::DrawingArea::new();
    area.set_content_width(8);
    area.set_content_height(8);
    area.set_valign(gtk::Align::Center);
    area.set_draw_func(move |_, cr, w, h| {
        let (r, g, b) = rgb(colour);
        cr.set_source_rgb(r, g, b);
        cr.arc(w as f64 / 2.0, h as f64 / 2.0, (w.min(h) as f64) / 2.0, 0.0, 2.0 * std::f64::consts::PI);
        let _ = cr.fill();
    });
    area
}

/// Something is typed in the search box.
fn searching(q: &str) -> bool {
    q.split_whitespace().next().is_some()
}

/// A project's colour: plain system colours (GNOME's palette), "Not sorted" grey. No purple.
fn rgb(c: Colour) -> (f64, f64, f64) {
    let hex: u32 = match c {
        Colour::Blue => 0x3584e4,
        Colour::Teal => 0x2190a4,
        Colour::Orange => 0xff7800,
        Colour::Green => 0x2ec27e,
        Colour::Pink => 0xdd2a77,
        Colour::Brown => 0x986a44,
        Colour::Grey => 0x9a9996,
    };
    let part = |shift: u32| ((hex >> shift) & 0xff) as f64 / 255.0;
    (part(16), part(8), part(0))
}

/// A rounded rectangle's outline.
fn rounded(cr: &cairo::Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    let r = r.min(w / 2.0).min(h / 2.0);
    let pi = std::f64::consts::PI;
    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -pi / 2.0, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0, pi / 2.0);
    cr.arc(x + r, y + h - r, r, pi / 2.0, pi);
    cr.arc(x + r, y + r, r, pi, 3.0 * pi / 2.0);
    cr.close_path();
}

/// The day bar: a grey track, each stretch in its project's colour, the hours under it (those that fit).
fn draw_bar(cr: &cairo::Context, w: f64, bar: &DayBar) {
    let (g, gg, gb) = rgb(Colour::Grey);
    cr.set_source_rgba(g, gg, gb, 0.2);
    rounded(cr, 0.0, 0.0, w, BAR_H, 4.0);
    let _ = cr.fill();
    rounded(cr, 0.0, 0.0, w, BAR_H, 4.0);
    cr.clip();
    for s in &bar.segments {
        let (r, gr, b) = rgb(s.colour);
        cr.set_source_rgb(r, gr, b);
        cr.rectangle(s.from * w, 0.0, ((s.to - s.from) * w).max(MIN_SEGMENT), BAR_H);
        let _ = cr.fill();
    }
    cr.reset_clip();
    cr.set_source_rgb(g, gg, gb);
    cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
    cr.set_font_size(11.0);
    let mut right = f64::MIN;
    for (at, text) in &bar.hours {
        let Ok(ext) = cr.text_extents(text) else { continue };
        let x = (at * w - ext.width() / 2.0).clamp(0.0, (w - ext.width()).max(0.0));
        if x < right + 8.0 {
            continue;
        }
        cr.move_to(x, BAR_H + HOURS_H - 4.0);
        let _ = cr.show_text(text);
        right = x + ext.width();
    }
}

/// The stretch under `x` on a day bar `w` wide.
fn segment_at(bar: &DayBar, x: f64, w: f64) -> Option<StretchKey> {
    bar.segments
        .iter()
        .rev()
        .find(|s| {
            let from = s.from * w;
            let to = from + ((s.to - s.from) * w).max(MIN_SEGMENT);
            x >= from - 1.0 && x <= to + 1.0
        })
        .map(|s| s.key.clone())
}

/// One row's bar in "Where your time went": `share` of the width, in the project's colour.
fn share_bar(share: f64, colour: Colour) -> gtk::DrawingArea {
    let area = gtk::DrawingArea::new();
    area.set_content_height(8);
    area.set_hexpand(true);
    area.set_valign(gtk::Align::Center);
    area.set_draw_func(move |_, cr, w, h| {
        let (w, h) = (w as f64, h as f64);
        let (g, gg, gb) = rgb(Colour::Grey);
        cr.set_source_rgba(g, gg, gb, 0.2);
        rounded(cr, 0.0, 0.0, w, h, h / 2.0);
        let _ = cr.fill();
        let (r, gr, b) = rgb(colour);
        cr.set_source_rgb(r, gr, b);
        rounded(cr, 0.0, 0.0, (share * w).max(h), h, h / 2.0);
        let _ = cr.fill();
    });
    area
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
