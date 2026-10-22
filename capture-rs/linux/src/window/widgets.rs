// context-capsule — linux: small building blocks for the window's pages — labels, cards, lists, keycaps, a
// plain message, the page frame (scrolls, centered, never too wide) and a watcher for the content's width.
// Colors come from the system theme (the accent color, light or dark); the CSS below only sets sizes.

use crate::ui;
use adw::prelude::*;
use adw::subclass::prelude::*;
use context_capsule_core::window::Section;
use gtk::glib;
use std::cell::{Cell, RefCell};

const CSS: &str = "
.greeting { font-size: 26pt; font-weight: 300; }
.stat-value { font-size: 20pt; font-weight: 300; }
.narrow .stat-value { font-size: 15pt; }
.stat + .stat { border-left: 1px solid alpha(currentColor, 0.12); }
.status-pill { padding: 3px 10px; border-radius: 999px; background-color: alpha(currentColor, 0.08); }
.status-pill.recording, .rec-dot { color: @error_color; }
.idle-dot { color: alpha(currentColor, 0.45); }
progressbar.project-bar > trough, progressbar.project-bar > trough > progress { min-height: 8px; border-radius: 4px; }
progressbar.project-bar.muted > trough > progress { background-color: alpha(currentColor, 0.35); }
.tag { font-size: smaller; padding: 1px 6px; border-radius: 4px; background-color: alpha(currentColor, 0.1); }
.cc-sidebar { background-color: alpha(currentColor, 0.03); }
.command { font-family: monospace; padding: 8px 10px; border-radius: 6px; background-color: alpha(currentColor, 0.06); }
.bubble { padding: 8px 12px; border-radius: 12px; background-color: alpha(@accent_bg_color, 0.14); }
.app-badge { min-width: 24px; min-height: 24px; border-radius: 12px; font-size: smaller; font-weight: bold; background-color: alpha(currentColor, 0.1); }
.timeline-text { font-size: smaller; }
";

pub fn load_css() {
    let Some(display) = gtk::gdk::Display::default() else { return };
    let provider = gtk::CssProvider::new();
    provider.load_from_data(CSS);
    gtk::style_context_add_provider_for_display(&display, &provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
}

/// A left-aligned label that wraps, with these style classes.
pub fn label(text: &str, classes: &[&str]) -> gtk::Label {
    let l = gtk::Label::new(Some(text));
    l.set_xalign(0.0);
    l.set_wrap(true);
    l.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    for c in classes {
        l.add_css_class(c);
    }
    l
}

/// A one-line label that shortens with "…".
pub fn line(text: &str, classes: &[&str]) -> gtk::Label {
    let l = label(text, classes);
    l.set_wrap(false);
    l.set_ellipsize(gtk::pango::EllipsizeMode::End);
    l
}

pub fn vbox(spacing: i32) -> gtk::Box {
    gtk::Box::new(gtk::Orientation::Vertical, spacing)
}

pub fn hbox(spacing: i32) -> gtk::Box {
    gtk::Box::new(gtk::Orientation::Horizontal, spacing)
}

/// A rounded panel (Adwaita's "card") holding `child` with padding.
pub fn card(child: &impl IsA<gtk::Widget>, padding: i32) -> gtk::Box {
    let b = vbox(0);
    b.add_css_class("card");
    child.set_margin_top(padding);
    child.set_margin_bottom(padding);
    child.set_margin_start(padding);
    child.set_margin_end(padding);
    b.append(child);
    b
}

/// A section's name above its card or list.
pub fn heading(text: &str) -> gtk::Label {
    let l = label(text, &["heading"]);
    l.set_margin_start(2);
    l
}

/// A list of rows in one rounded box.
pub fn boxed_list() -> gtk::ListBox {
    let l = gtk::ListBox::new();
    l.set_selection_mode(gtk::SelectionMode::None);
    l.add_css_class("boxed-list");
    l
}

/// A row with a title (and maybe a subtitle), shown as typed (no markup).
pub fn row(title: &str, subtitle: &str) -> adw::ActionRow {
    let r = adw::ActionRow::new();
    r.set_use_markup(false);
    r.set_title(title);
    if !subtitle.is_empty() {
        r.set_subtitle(subtitle);
    }
    r
}

/// "Alt+C" drawn as keys.
pub fn keycap(keys: &str) -> gtk::ShortcutLabel {
    let k = gtk::ShortcutLabel::new(&ui::accelerator(keys));
    k.set_valign(gtk::Align::Center);
    k
}

/// A flat text button.
pub fn flat_button(text: &str) -> gtk::Button {
    let b = gtk::Button::with_label(text);
    b.add_css_class("flat");
    b.set_valign(gtk::Align::Center);
    b
}

/// A message with one OK button.
pub fn message(win: &adw::ApplicationWindow, heading: &str, body: &str) {
    let d = adw::MessageDialog::new(Some(win), Some(heading), Some(body));
    d.add_response("ok", "OK");
    d.set_default_response(Some("ok"));
    d.present();
}

/// A sidebar entry: icon and name.
pub fn sidebar_row(s: Section) -> gtk::ListBoxRow {
    let icon = match s {
        Section::Home => "user-home-symbolic",
        Section::Timeline => "document-open-recent-symbolic",
        Section::Capsules => "folder-documents-symbolic",
        Section::Projects => "folder-symbolic",
        Section::Settings => "emblem-system-symbolic",
        Section::Help => "help-about-symbolic",
    };
    let b = hbox(12);
    b.append(&gtk::Image::from_icon_name(icon));
    b.append(&line(s.title(), &[]));
    let r = gtk::ListBoxRow::new();
    r.set_child(Some(&b));
    r
}

/// A sidebar group's name.
pub fn sidebar_heading(text: &str, first: bool) -> gtk::Label {
    let l = line(text, &["caption-heading", "dim-label"]);
    l.set_margin_start(12);
    l.set_margin_top(if first { 6 } else { 18 });
    l.set_margin_bottom(6);
    l
}

/// The frame of a page: scrolls up and down, centered, at most as wide as `fit` says.
pub fn page(content: &impl IsA<gtk::Widget>) -> (gtk::ScrolledWindow, adw::Clamp) {
    let clamp = adw::Clamp::builder().maximum_size(760).tightening_threshold(760).child(content).build();
    clamp.set_margin_start(ui::PAGE_MARGIN as i32);
    clamp.set_margin_end(ui::PAGE_MARGIN as i32);
    clamp.set_margin_top(24);
    clamp.set_margin_bottom(32);
    let scroll = gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).vexpand(true).hexpand(true).child(&clamp).build();
    (scroll, clamp)
}

/// Remove every child of a box.
/// Empty a list (GTK 4.8 has no `remove_all`).
pub fn clear_list(l: &gtk::ListBox) {
    while let Some(c) = l.first_child() {
        l.remove(&c);
    }
}

pub fn clear(b: &gtk::Box) {
    while let Some(c) = b.first_child() {
        b.remove(&c);
    }
}

type OnWidth = Box<dyn Fn(i32)>;

mod imp {
    use super::*;

    /// Holds one child; tells when its width changes (GTK 4.8 has no breakpoints).
    #[derive(Default)]
    pub struct WidthWatch {
        pub last: Cell<i32>,
        pub on_width: RefCell<Option<OnWidth>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for WidthWatch {
        const NAME: &'static str = "CcWidthWatch";
        type Type = super::WidthWatch;
        type ParentType = adw::Bin;
    }

    impl ObjectImpl for WidthWatch {
        fn constructed(&self) {
            self.parent_constructed();
            // With a layout manager GTK never calls `size_allocate` below; it measures and places the child itself.
            self.obj().set_layout_manager(None::<gtk::LayoutManager>);
        }
    }

    impl WidgetImpl for WidthWatch {
        fn measure(&self, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            match self.obj().child() {
                Some(child) => child.measure(orientation, for_size),
                None => (0, 0, -1, -1),
            }
        }

        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            if let Some(child) = self.obj().child() {
                child.allocate(width, height, baseline, None);
            }
            if self.last.replace(width) != width {
                // Not while GTK lays out: changing sizes now would be ignored.
                let obj = self.obj().downgrade();
                glib::idle_add_local_once(move || {
                    if let Some(o) = obj.upgrade() {
                        let imp = o.imp();
                        if let Some(f) = imp.on_width.borrow().as_ref() {
                            f(imp.last.get());
                        }
                    }
                });
            }
        }
    }

    impl BinImpl for WidthWatch {}
}

glib::wrapper! {
    pub struct WidthWatch(ObjectSubclass<imp::WidthWatch>)
        @extends adw::Bin, gtk::Widget,
        @implements gtk::Buildable, gtk::ConstraintTarget;
}

impl WidthWatch {
    pub fn new(child: &impl IsA<gtk::Widget>) -> WidthWatch {
        let w: WidthWatch = glib::Object::new();
        adw::prelude::BinExt::set_child(&w, Some(child));
        w
    }

    pub fn on_width(&self, f: impl Fn(i32) + 'static) {
        *self.imp().on_width.borrow_mut() = Some(Box::new(f));
    }
}
