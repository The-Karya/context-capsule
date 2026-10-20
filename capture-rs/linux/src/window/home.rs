// context-capsule — linux: Home — hello and the date, today / this week / streak, the status card (start, stop,
// speak a note, add a screen), today's time per project, the newest capsules; on the side (below, when narrow)
// the shortcuts, what you're working on, and "Make today's capsules".

use super::widgets::{self, boxed_list, card, flat_button, hbox, heading, keycap, label, line, row, vbox};
use super::Ctx;
use crate::ui;
use adw::prelude::*;
use context_capsule_core::capsules::today;
use context_capsule_core::stats::WorkStats;
use context_capsule_core::window::{self as words, Chip, Section};
use context_capsule_core::{Msg, ShareScreens};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::SystemTime;

pub struct Home {
    ctx: Rc<Ctx>,
    pub root: gtk::Box,
    greeting: gtk::Label,
    date: gtk::Label,
    stats: Vec<(gtk::Label, gtk::Label)>,
    strip: gtk::Grid,
    columns: gtk::Box,
    side: gtk::Box,
    dot: gtk::Label,
    status_title: gtk::Label,
    status_detail: gtk::Label,
    now_line: gtk::Label,
    toggle: gtk::Button,
    voice: gtk::Button,
    add_screen: gtk::Button,
    bars: gtk::Box,
    recent: gtk::Box,
    see_all: gtk::Button,
    chips: gtk::ListBox,
    chips_shown: RefCell<Option<Vec<Chip>>>,
}

impl Home {
    pub fn new(ctx: &Rc<Ctx>) -> Rc<Home> {
        let root = vbox(24);

        // Hello, and the date.
        let greeting = label("", &["greeting"]);
        let date = label("", &["dim-label"]);
        let hello = vbox(2);
        hello.append(&greeting);
        hello.append(&date);
        root.append(&hello);

        // Today, this week, the streak: three big numbers in one panel.
        let strip = gtk::Grid::new();
        strip.set_column_homogeneous(true);
        strip.add_css_class("card");
        let mut stats = vec![];
        for (i, s) in words::stats(&Default::default()).iter().enumerate() {
            let col = vbox(2);
            col.add_css_class("stat");
            col.set_margin_top(0);
            let name = label(s.label, &["caption-heading", "dim-label"]);
            let value = line("", &["stat-value", "numeric"]);
            let detail = line("", &["caption", "dim-label"]);
            for w in [&name, &value, &detail] {
                w.set_margin_start(18);
                w.set_margin_end(12);
            }
            name.set_margin_top(14);
            detail.set_margin_bottom(14);
            col.append(&name);
            col.append(&value);
            col.append(&detail);
            strip.attach(&col, i as i32, 0, 1, 1);
            stats.push((value, detail));
        }
        root.append(&strip);

        // The main column.
        let main = vbox(24);
        main.set_hexpand(true);

        let dot = label("●", &["idle-dot"]);
        let status_title = line("", &["title-3", "numeric"]);
        let top = hbox(10);
        top.append(&dot);
        top.append(&status_title);
        let status_detail = label("", &["dim-label"]);
        let now_line = line("", &["caption", "dim-label"]);
        let toggle = gtk::Button::with_label("");
        toggle.add_css_class("suggested-action");
        let voice = gtk::Button::with_label("");
        let add_screen = gtk::Button::with_label(words::ADD_SCREEN);
        let buttons = hbox(8);
        buttons.set_margin_top(4);
        buttons.append(&toggle);
        buttons.append(&voice);
        buttons.append(&add_screen);
        let status_box = vbox(6);
        status_box.append(&top);
        status_box.append(&status_detail);
        status_box.append(&now_line);
        status_box.append(&buttons);
        main.append(&card(&status_box, 18));

        let today_box = vbox(8);
        today_box.append(&heading(words::TODAY));
        let bars = vbox(10);
        today_box.append(&card(&bars, 16));
        main.append(&today_box);

        let recent_box = vbox(8);
        let recent_head = hbox(8);
        let recent_title = heading(words::RECENT_CAPSULES);
        recent_title.set_hexpand(true);
        let see_all = flat_button(words::SEE_ALL);
        see_all.add_css_class("caption");
        recent_head.append(&recent_title);
        recent_head.append(&see_all);
        recent_box.append(&recent_head);
        let recent = vbox(0);
        recent_box.append(&recent);
        main.append(&recent_box);

        // The side column.
        let side = vbox(24);
        let keys_box = vbox(8);
        keys_box.append(&heading(words::SHORTCUTS));
        let keys_list = boxed_list();
        for (keys, what) in words::home_shortcuts(&ctx.keys) {
            let r = row(what, "");
            r.add_suffix(&keycap(keys));
            keys_list.append(&r);
        }
        keys_box.append(&keys_list);
        side.append(&keys_box);

        let working = vbox(8);
        working.append(&heading(words::WORKING_ON));
        let chips = boxed_list();
        working.append(&chips);
        side.append(&working);

        let make_box = vbox(8);
        make_box.append(&heading(words::MAKE_TITLE));
        let make_inner = hbox(12);
        let make_hint = label(&words::make_hint(&ctx.keys), &["dim-label", "caption"]);
        make_hint.set_hexpand(true);
        let make = gtk::Button::with_label(words::MAKE_BUTTON);
        make.add_css_class("suggested-action");
        make.set_valign(gtk::Align::Center);
        make_inner.append(&make_hint);
        make_inner.append(&make);
        make_box.append(&card(&make_inner, 14));
        side.append(&make_box);

        let columns = vbox(24);
        columns.append(&main);
        columns.append(&side);
        root.append(&columns);

        let home = Rc::new(Home {
            ctx: ctx.clone(),
            root,
            greeting,
            date,
            stats,
            strip,
            columns,
            side,
            dot,
            status_title,
            status_detail,
            now_line,
            toggle,
            voice,
            add_screen,
            bars,
            recent,
            see_all,
            chips,
            chips_shown: RefCell::new(None),
        });

        let c = ctx.clone();
        home.toggle.connect_clicked(move |_| c.send(Msg::ToggleRecord));
        let c = ctx.clone();
        home.voice.connect_clicked(move |_| c.send(Msg::ToggleVoice));
        let c = ctx.clone();
        home.add_screen.connect_clicked(move |_| c.send(Msg::AddScreen));
        let c = ctx.clone();
        home.see_all.connect_clicked(move |_| c.go(Section::Capsules));
        let c = ctx.clone();
        make.connect_clicked(move |_| c.make_capsules());
        home.set_numbers();
        home.set_status();
        home
    }

    /// Two columns (wide), or the side column under the main one.
    pub fn set_wide(&self, wide: bool) {
        self.columns.set_orientation(if wide { gtk::Orientation::Horizontal } else { gtk::Orientation::Vertical });
        self.side.set_size_request(if wide { words::SIDE_WIDTH as i32 } else { -1 }, -1);
        if wide {
            self.strip.remove_css_class("narrow");
        } else {
            self.strip.add_css_class("narrow");
        }
    }

    pub fn refresh(&self) {
        self.set_numbers();
        self.set_status();
    }

    /// Hello, the three numbers, today's bars and the newest capsules.
    pub fn set_numbers(&self) {
        let n = self.ctx.numbers.borrow();
        let now = SystemTime::now();
        self.greeting.set_text(&words::greeting(WorkStats::hour(now), &n.first_name));
        self.date.set_text(&ui::date_line(today()));
        for ((value, detail), s) in self.stats.iter().zip(words::stats(&n)) {
            value.set_text(&s.value);
            detail.set_text(&s.detail);
        }

        widgets::clear(&self.bars);
        let bars = words::bars(&n.today, &self.ctx.status.borrow().projects);
        if bars.is_empty() {
            self.bars.append(&label(words::TODAY_EMPTY, &["dim-label"]));
        }
        let grid = gtk::Grid::new();
        grid.set_column_spacing(12);
        grid.set_row_spacing(10);
        for (i, b) in bars.iter().enumerate() {
            let classes: &[&str] = if b.muted { &["dim-label"] } else { &[] };
            let name = line(&b.name, classes);
            name.set_width_chars(12);
            name.set_max_width_chars(22);
            let bar = gtk::ProgressBar::new();
            bar.add_css_class("project-bar");
            if b.muted {
                bar.add_css_class("muted");
            }
            bar.set_fraction(b.fraction.max(0.02));
            bar.set_hexpand(true);
            bar.set_valign(gtk::Align::Center);
            let time = line(&b.time, &["dim-label", "numeric", "caption"]);
            time.set_xalign(1.0);
            time.set_width_chars(8);
            grid.attach(&name, 0, i as i32, 1, 1);
            grid.attach(&bar, 1, i as i32, 1, 1);
            grid.attach(&time, 2, i as i32, 1, 1);
        }
        if !bars.is_empty() {
            self.bars.append(&grid);
        }

        widgets::clear(&self.recent);
        self.see_all.set_visible(!n.recent.is_empty());
        if n.recent.is_empty() {
            self.recent.append(&card(&label(words::NONE_YET, &["dim-label"]), 16));
            return;
        }
        let list = boxed_list();
        let day = today();
        let projects = self.ctx.status.borrow().projects.clone();
        for c in &n.recent {
            let (title, when) = ui::recent_capsule(c, &projects, day);
            let r = row(&title, &c.in_short);
            r.set_subtitle_lines(1);
            if !when.is_empty() {
                r.add_suffix(&line(&when, &["dim-label", "caption"]));
            }
            if c.draft {
                r.add_suffix(&super::capsules::draft_tag());
            }
            r.add_suffix(&gtk::Image::from_icon_name("go-next-symbolic"));
            r.set_activatable(true);
            let (ctx, path) = (self.ctx.clone(), c.path.clone());
            r.connect_activated(move |_| super::capsules::open_file(&ctx, &path));
            list.append(&r);
        }
        self.recent.append(&list);
    }

    /// The status card and "Working on", from the latest status.
    pub fn set_status(&self) {
        let s = self.ctx.status.borrow().clone();
        self.tick();
        self.dot.set_text(if s.recording { "●" } else { "○" });
        if s.recording {
            self.dot.remove_css_class("idle-dot");
            self.dot.add_css_class("rec-dot");
        } else {
            self.dot.remove_css_class("rec-dot");
            self.dot.add_css_class("idle-dot");
        }
        let share = if s.recording { ShareScreens::Pick } else { ShareScreens::load(&self.ctx.paths.config()) };
        self.status_detail.set_text(&words::status_detail(&s, &self.ctx.keys, share));
        let now = words::now_line(&s);
        self.now_line.set_visible(now.is_some());
        self.now_line.set_text(now.as_deref().unwrap_or(""));
        self.toggle.set_label(words::toggle_button(&s));
        self.voice.set_label(words::voice_button(&s));
        self.voice.set_visible(s.recording);
        self.add_screen.set_visible(s.recording);
        self.add_screen.set_sensitive(s.can_add_screen());
        self.add_screen.set_label(&s.add_screen_label());

        let chips = words::chips(&s);
        if self.chips_shown.borrow().as_ref() == Some(&chips) {
            return;
        }
        while let Some(c) = self.chips.first_child() {
            self.chips.remove(&c);
        }
        for chip in &chips {
            let r = row(&chip.label, "");
            let check = gtk::Image::from_icon_name("object-select-symbolic");
            check.set_visible(chip.selected);
            check.add_css_class("accent");
            r.add_suffix(&check);
            r.set_activatable(true);
            let (ctx, id) = (self.ctx.clone(), chip.id.clone());
            r.connect_activated(move |_| ctx.send(Msg::PickProject(id.clone())));
            self.chips.append(&r);
        }
        let new = row(words::NEW_PROJECT, "");
        new.add_prefix(&gtk::Image::from_icon_name("list-add-symbolic"));
        new.set_activatable(true);
        let ctx = self.ctx.clone();
        new.connect_activated(move |_| ask_new_project(&ctx));
        self.chips.append(&new);
        *self.chips_shown.borrow_mut() = Some(chips);
    }

    /// The running time.
    pub fn tick(&self) {
        self.status_title.set_text(&words::status_title(&self.ctx.status.borrow(), SystemTime::now()));
    }
}

/// "New project": a name (prefilled with the window's clue), then the engine makes it.
fn ask_new_project(ctx: &Rc<Ctx>) {
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
