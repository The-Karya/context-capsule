// context-capsule — linux: small building blocks for the window's pages — labels, cards, lists, keycaps, a
// plain message, the page frame (scrolls, centered, never too wide) and a watcher for the content's width.
// The look is the Mac app's (macOS system colours, its sizes and its rounded panels), set by an app-level style
// sheet so the desktop's theme doesn't change it: light or dark follows Settings → Appearance (or the computer).

use crate::ui;
use adw::prelude::*;
use adw::subclass::prelude::*;
use context_capsule_core::window::Section;
use gtk::glib;
use std::cell::{Cell, RefCell};

/// light and dark: the Mac's system colours (labels, panels, separators, controls, the default blue).
fn palette(dark: bool) -> String {
    // The Mac's colours as they look on screen (DESIGN.md §5: Liquid Glass painted as flat fills).
    let c: [(&str, &str); 27] = if dark {
        [
            ("cc_bg", "#282828"),
            ("cc_fg", "rgba(255,255,255,0.85)"),
            ("cc_fg2", "rgba(255,255,255,0.55)"),
            ("cc_fg3", "#8c8c8c"),
            ("cc_side", "#272727"),
            ("cc_side_border", "#3e3e3e"),
            ("cc_panel", "#363636"),
            ("cc_panel_border", "rgba(255,255,255,0.08)"),
            ("cc_panel_rim", "rgba(255,255,255,0.14)"),
            ("cc_panel_shadow", "rgba(0,0,0,0.3)"),
            ("cc_flat_panel", "#343434"),
            ("cc_sep", "rgba(255,255,255,0.1)"),
            ("cc_control", "rgba(255,255,255,0.09)"),
            ("cc_control_hover", "rgba(255,255,255,0.13)"),
            ("cc_control_active", "rgba(255,255,255,0.18)"),
            ("cc_field", "#282828"),
            ("cc_field_border", "#3a3a3a"),
            ("cc_text_field", "#424242"),
            ("cc_accent", "#418ff8"),
            ("cc_accent_hover", "#5a9ef9"),
            ("cc_link", "#419cff"),
            ("cc_red", "#ff4245"),
            ("cc_popover", "#323232"),
            ("cc_pill", "#393939"),
            ("cc_side_selected", "#2e70d4"),
            ("cc_bubble", "#364254"),
            ("cc_switch_off", "#4a4a4a"),
        ]
    } else {
        [
            ("cc_bg", "#ffffff"),
            ("cc_fg", "rgba(0,0,0,0.85)"),
            ("cc_fg2", "rgba(0,0,0,0.5)"),
            ("cc_fg3", "#9a9a9a"),
            ("cc_side", "#fafafa"),
            ("cc_side_border", "#ededed"),
            ("cc_panel", "#f6f6f6"),
            ("cc_panel_border", "rgba(0,0,0,0.05)"),
            ("cc_panel_rim", "rgba(255,255,255,0.7)"),
            ("cc_panel_shadow", "rgba(0,0,0,0.06)"),
            ("cc_flat_panel", "#f7f7f7"),
            ("cc_sep", "rgba(0,0,0,0.1)"),
            ("cc_control", "rgba(0,0,0,0.06)"),
            ("cc_control_hover", "rgba(0,0,0,0.09)"),
            ("cc_control_active", "rgba(0,0,0,0.13)"),
            ("cc_field", "#ffffff"),
            ("cc_field_border", "#e5e5e5"),
            ("cc_text_field", "#e7e7e7"),
            ("cc_accent", "#3b86f1"),
            ("cc_accent_hover", "#2f79e6"),
            ("cc_link", "#0068da"),
            ("cc_red", "#ff383c"),
            ("cc_popover", "#fafafa"),
            ("cc_pill", "#ffffff"),
            ("cc_side_selected", "#347ae1"),
            ("cc_bubble", "#e3eaf7"),
            ("cc_switch_off", "#e3e3e3"),
        ]
    };
    let mut out: String = c.iter().map(|(k, v)| format!("@define-color {k} {v};\n")).collect();
    // libadwaita's own colours (switches, checks, dialogs, popovers, focus rings): the same blue and backgrounds.
    let named = [
        ("accent_color", "cc_accent"),
        ("accent_bg_color", "cc_accent"),
        ("window_bg_color", "cc_bg"),
        ("view_bg_color", "cc_bg"),
        ("headerbar_bg_color", "cc_bg"),
        ("popover_bg_color", "cc_popover"),
        ("dialog_bg_color", "cc_popover"),
    ];
    for (k, v) in named {
        out.push_str(&format!("@define-color {k} @{v};\n"));
    }
    out.push_str("@define-color accent_fg_color #ffffff;\n");
    out.push_str(&format!("@define-color window_fg_color {};\n@define-color view_fg_color {};\n", c[1].1, c[1].1));
    out
}

/// libadwaita 1.6+ (GTK 4.16+) reads its colours from CSS variables: the same blue and backgrounds there.
const VARIABLES: &str = "
:root {
  --accent-color: @cc_accent; --accent-bg-color: @cc_accent; --accent-fg-color: #ffffff;
  --window-bg-color: @cc_bg; --view-bg-color: @cc_bg; --headerbar-bg-color: @cc_bg;
  --popover-bg-color: @cc_popover; --dialog-bg-color: @cc_popover;
  --window-fg-color: @cc_fg; --view-fg-color: @cc_fg;
}
";

const CSS: &str = r#"
/* The window: the Mac's font sizes (13 px body), its colours. */
window.cc-window { background-color: @cc_bg; color: @cc_fg; font-family: "Inter", "SF Pro Text", "Cantarell", sans-serif; font-size: 13px; }
window.cc-window headerbar { background: none; box-shadow: none; border: none; min-height: 52px; }
window.cc-window headerbar .title { font-size: 13px; font-weight: 600; }
window.cc-window .dim-label { opacity: 1; color: @cc_fg2; }
window.cc-window .caption { font-size: 12px; }
window.cc-window .caption-heading { font-size: 12px; font-weight: 500; }
window.cc-window .heading { font-size: 13px; font-weight: 600; }
window.cc-window .numeric { font-feature-settings: "tnum"; }
window.cc-window .title-1 { font-size: 22px; font-weight: 600; }
window.cc-window .title-2 { font-size: 22px; font-weight: 600; color: @cc_fg2; }
window.cc-window .title-3 { font-size: 17px; font-weight: 600; }
window.cc-window .greeting { font-family: "New York", "Source Serif 4", "Source Serif Pro", "Noto Serif", "DejaVu Serif", serif; font-size: 32px; font-weight: 400; }
window.cc-window .cc-caption { font-size: 12px; font-weight: 500; color: @cc_fg2; margin-left: 4px; }
window.cc-window .cc-card-title { font-size: 14px; font-weight: 600; }
window.cc-window .cc-stretch-title { font-size: 13px; font-weight: 500; }

/* The sidebar: a light rounded panel inset from the window's edge, the page's blue on the chosen row. */
window.cc-window .cc-sidebar { background-color: @cc_side; border: 1px solid @cc_side_border; border-radius: 16px; margin: 8px 0 8px 8px; box-shadow: 0 1px 4px @cc_panel_shadow; }
window.cc-window .cc-sidebar headerbar { background-color: transparent; border-radius: 16px 16px 0 0; }
window.cc-window .cc-sidebar scrolledwindow, window.cc-window .cc-sidebar list { background: transparent; }
window.cc-window .cc-sidebar list.navigation-sidebar { padding: 0 10px 10px 10px; }
window.cc-window .cc-sidebar list.navigation-sidebar > row { min-height: 32px; padding: 0 10px; margin: 0; border-radius: 8px; color: @cc_fg; font-size: 13px; }
window.cc-window .cc-sidebar list.navigation-sidebar > row:hover { background-color: @cc_control; }
window.cc-window .cc-sidebar list.navigation-sidebar > row:selected { background-color: @cc_side_selected; color: #ffffff; }
window.cc-window .cc-sidebar list.navigation-sidebar > row:selected image { color: #ffffff; }
window.cc-window .cc-sidebar list.navigation-sidebar > row image { color: @cc_fg; }
window.cc-window .cc-side-heading { font-size: 11px; font-weight: 600; color: @cc_fg3; }
window.cc-window list.boxed-list row.expander list.nested, window.cc-window list.boxed-list row.expander list.nested > row { background: transparent; }
window.cc-window list.boxed-list row.expander list.nested > row { min-height: 32px; }
window.cc-window .title-1, window.cc-window .title-2 { letter-spacing: -0.2px; }
window.cc-window .stat-value { letter-spacing: -0.4px; }
window.cc-window .cc-sidebar headerbar .cc-side-title { font-size: 13px; font-weight: 600; color: @cc_fg2; }

/* Panels: rounded 14 px, a faint fill, a soft shadow (Home, Capsules, Projects, Settings, Help). */
window.cc-window .card, window.cc-window list.boxed-list {
  background-color: @cc_panel; color: @cc_fg; border-radius: 14px; border: 1px solid @cc_panel_border;
  box-shadow: inset 0 1px @cc_panel_rim, 0 1px 6px @cc_panel_shadow;
}
window.cc-window list.boxed-list > row { background-color: transparent; border-bottom: none; min-height: 44px; }
window.cc-window list.boxed-list > row:not(:last-child) {
  background-image: linear-gradient(to right, transparent 14px, @cc_sep 14px);
  background-size: 100% 1px; background-position: left bottom; background-repeat: no-repeat;
}
window.cc-window list.boxed-list > row.activatable:hover { background-color: alpha(@cc_fg, 0.03); }
window.cc-window list.boxed-list > row:first-child { border-radius: 14px 14px 0 0; }
window.cc-window list.boxed-list > row:last-child { border-radius: 0 0 14px 14px; }
window.cc-window list.boxed-list > row:only-child { border-radius: 14px; }
window.cc-window row .title { font-size: 14px; }
window.cc-window row.cc-strong .title { font-weight: 500; }
window.cc-window row .subtitle { font-size: 12px; color: @cc_fg2; opacity: 1; }
window.cc-window preferencesgroup > box > box .heading,
window.cc-window preferencesgroup > box > box > box > .title { font-size: 12px; font-weight: 500; color: @cc_fg2; margin-left: 4px; }
window.cc-window preferencesgroup > box > box .description,
window.cc-window preferencesgroup > box > box .dim-label { font-size: 12px; color: @cc_fg2; margin-left: 4px; }
window.cc-window .stat + .stat { border-left: 1px solid @cc_sep; }
window.cc-window .stat { margin: 14px 0; }
window.cc-window .stat-value { font-size: 28px; font-weight: 600; font-feature-settings: "tnum"; }
window.cc-window .narrow .stat-value { font-size: 22px; }
window.cc-window .stat .caption-heading { color: @cc_fg2; }

/* Inside a panel, a plain list (Home's Shortcuts and Working on). */
window.cc-window list.cc-plain { background: transparent; }
window.cc-window list.cc-plain > row { background: transparent; min-height: 30px; padding: 0; border: none; }
window.cc-window list.cc-plain > row.activatable:hover { background-color: alpha(@cc_fg, 0.04); border-radius: 6px; }
window.cc-window list.cc-plain > row .title { font-size: 13px; }

/* The Timeline's recordings: one flat panel each (no shadow), the stretches under a line. */
window.cc-window .cc-block { background-color: @cc_flat_panel; border-radius: 14px; padding: 6px 0; }
window.cc-window .cc-block list.boxed-list {
  background-color: transparent; border: none; box-shadow: none; border-radius: 0;
  background-image: linear-gradient(to right, transparent 46px, @cc_sep 46px); background-size: 100% 1px; background-position: left top; background-repeat: no-repeat;
}
window.cc-window .cc-block list.boxed-list > row:not(:last-child) { background-image: linear-gradient(to right, transparent 46px, @cc_sep 46px); }
window.cc-window .cc-block button.flat.cc-fold { color: @cc_fg; }
window.cc-window .cc-block list.boxed-list > row:hover { background-color: transparent; }
window.cc-window .cc-went { background-color: @cc_flat_panel; border-radius: 14px; padding: 14px; }
window.cc-window .cc-block button.flat.cc-fold:hover { background: transparent; }

/* Buttons: grey rounded push buttons; the one main action blue and round. */
window.cc-window .cc-page button:not(.flat):not(.circular):not(.image-button) { background-color: @cc_control; background-image: none; color: @cc_fg; border: none; box-shadow: none; border-radius: 999px; min-height: 24px; padding: 2px 12px; font-size: 13px; font-weight: 400; }
window.cc-window .cc-page button:not(.flat):hover { background-color: @cc_control_hover; }
window.cc-window .cc-page button:not(.flat):active, window.cc-window .cc-page button:checked { background-color: @cc_control_active; }
window.cc-window button:disabled { opacity: 0.45; }
window.cc-window .cc-page button.suggested-action:not(.flat):not(.circular):not(.image-button) { background-color: @cc_accent; color: #ffffff; border-radius: 999px; min-height: 28px; padding: 2px 16px; font-weight: 500; }
window.cc-window .cc-page button.suggested-action:not(.flat):not(.circular):not(.image-button):hover { background-color: @cc_accent_hover; }
window.cc-window .cc-page button.cc-large:not(.flat):not(.circular):not(.image-button) { min-height: 28px; border-radius: 999px; }
window.cc-window .cc-page button.flat { color: @cc_fg2; border-radius: 6px; min-height: 22px; padding: 2px 6px; box-shadow: none; font-weight: 400; font-size: 12px; }
window.cc-window .cc-page button.flat:hover { background-color: @cc_control; color: @cc_fg; }
window.cc-window .cc-page button.flat.cc-link { color: @cc_link; padding: 0 2px; }
window.cc-window .cc-page button.flat.cc-link:hover { background: transparent; text-decoration: underline; }
window.cc-window .cc-page menubutton > button.flat, window.cc-window .cc-page button.image-button { color: @cc_fg2; border-radius: 999px; min-width: 26px; min-height: 26px; padding: 0; background-color: transparent; }
window.cc-window .cc-page menubutton > button.flat:hover, window.cc-window .cc-page button.image-button:hover { background-color: @cc_control; }
window.cc-window headerbar button { color: @cc_fg2; }
window.cc-window .cc-page dropdown > button, window.cc-window .cc-page combobox button { background-color: @cc_control; border-radius: 7px; min-height: 24px; }
window.cc-window switch { background-color: @cc_switch_off; border: none; border-radius: 999px; padding: 0; min-height: 24px; }
window.cc-window switch:checked { background-color: @cc_accent; }
window.cc-window switch > slider { background-color: #ffffff; border: none; border-radius: 999px; min-width: 24px; min-height: 20px; margin: 2px; box-shadow: 0 1px 2px rgba(0,0,0,0.25); }
window.cc-window switch image { color: transparent; }
window.cc-window .cc-page button.cc-small:not(.flat):not(.circular):not(.image-button) { min-height: 20px; padding: 0 10px; font-size: 12px; }
window.cc-window .cc-page dropdown.cc-popup > button { background-color: @cc_control; border-radius: 7px; min-height: 24px; padding: 0 8px 0 12px; font-size: 13px; }
window.cc-window .cc-page dropdown.cc-popup > button:hover { background-color: @cc_control_hover; }
window.cc-window .cc-page dropdown.cc-popup arrow { color: @cc_fg2; -gtk-icon-size: 12px; }
window.cc-window .cc-segmented { background-color: @cc_control; border-radius: 999px; padding: 0; }
window.cc-window .cc-page .cc-segmented > button:not(.flat):not(.circular):not(.image-button) { background-color: transparent; border-radius: 999px; min-height: 24px; padding: 0 16px; }
window.cc-window .cc-page .cc-segmented > button:checked:not(.flat):not(.circular):not(.image-button) { background-color: @cc_accent; color: #ffffff; }
window.cc-window .cc-page button.flat.cc-disclosure { color: @cc_fg2; font-size: 13px; padding: 2px 0; min-height: 20px; }
window.cc-window .cc-page button.flat.cc-disclosure:hover { background: transparent; color: @cc_fg; }
window.cc-window .cc-page button.cc-keys:not(.flat):not(.circular):not(.image-button) { background-color: transparent; padding: 0 2px; }
window.cc-window .cc-page button.cc-keys:not(.flat):hover { background-color: @cc_control; }
window.cc-window list.boxed-list > row.cc-sub:hover { background-color: transparent; }
window.cc-window image.success { color: @cc_fg2; }

/* Text fields: the search fields round with a hairline; others filled. */
window.cc-window entry { background-color: @cc_text_field; border: none; border-radius: 7px; min-height: 24px; box-shadow: none; color: @cc_fg; }
window.cc-window entry.search { background-color: @cc_field; border: 1px solid @cc_field_border; border-radius: 999px; min-height: 26px; padding: 0 10px; }
window.cc-window entry.search > image { color: @cc_fg2; }
window.cc-window entry:focus-within { outline: 3px solid alpha(@cc_accent, 0.45); outline-offset: 0; }
window.cc-window entry placeholder, window.cc-window entry > text > placeholder { color: @cc_fg3; }
window.cc-window row.entry { min-height: 44px; }

/* The progress bars and small things. */
window.cc-window .status-pill { min-height: 26px; margin-right: 4px; padding: 4px 12px; border-radius: 999px; background-color: @cc_pill; border: 1px solid @cc_side_border; box-shadow: 0 1px 6px @cc_panel_shadow; font-size: 12px; font-weight: 500; }
window.cc-window .status-pill label { font-size: 12px; font-weight: 500; }
window.cc-window .status-pill image { color: @cc_fg2; }
window.cc-window .status-pill.recording label { color: @cc_red; }
window.cc-window .rec-dot { color: @cc_red; }
window.cc-window .idle-dot { color: @cc_fg2; }
window.cc-window .tag { font-size: 11px; padding: 1px 6px; border-radius: 4px; background-color: @cc_control; }
window.cc-window .command { font-family: monospace; padding: 8px 10px; border-radius: 6px; background-color: @cc_control; }
window.cc-window .bubble { padding: 8px 12px; border-radius: 12px; background-color: @cc_bubble; font-size: 13px; }
window.cc-window .app-badge { min-width: 22px; min-height: 22px; border-radius: 6px; font-size: 11px; font-weight: 600; background-color: @cc_control; }
window.cc-window .timeline-text { font-size: 12px; }
window.cc-window shortcut > .keycap, window.cc-window shortcuts .keycap, window.cc-window .keycap { font-size: 13px; font-weight: 600; padding: 2px 8px; border-radius: 6px; background-color: @cc_control; border: 1px solid @cc_side_border; box-shadow: none; color: @cc_fg; min-width: 14px; }
window.cc-window separator { background-color: @cc_sep; }
window.cc-window scrolledwindow, window.cc-window viewport { background: transparent; }
window.cc-window toast { border-radius: 999px; }
window.cc-window popover > contents { border-radius: 12px; }
window.cc-window calendar { border: none; background: transparent; }
window.cc-window calendar > grid > label.day-number:selected { background-color: @cc_accent; color: #ffffff; border-radius: 999px; }
"#;

thread_local! {
    static PROVIDER: RefCell<Option<gtk::CssProvider>> = const { RefCell::new(None) };
}

/// The look, for light or dark (as Adwaita shows now); loaded again whenever that changes.
pub fn load_css() {
    let Some(display) = gtk::gdk::Display::default() else { return };
    let provider = gtk::CssProvider::new();
    gtk::style_context_add_provider_for_display(&display, &provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
    PROVIDER.with(|p| *p.borrow_mut() = Some(provider));
    let style = adw::StyleManager::default();
    reload_css(style.is_dark());
    style.connect_dark_notify(|s| reload_css(s.is_dark()));
}

fn reload_css(dark: bool) {
    let mut css = palette(dark);
    if gtk::minor_version() >= 16 {
        css.push_str(VARIABLES);
    }
    css.push_str(CSS);
    PROVIDER.with(|p| {
        if let Some(p) = p.borrow().as_ref() {
            p.load_from_data(&css);
        }
    });
    // The bars are drawn by hand in light or dark colours: draw them again.
    for w in gtk::Window::list_toplevels() {
        redraw_drawings(&w);
    }
}

fn redraw_drawings(w: &gtk::Widget) {
    if w.is::<gtk::DrawingArea>() {
        w.queue_draw();
    }
    let mut child = w.first_child();
    while let Some(c) = child {
        redraw_drawings(&c);
        child = c.next_sibling();
    }
}

/// Dark now (the Timeline's colours follow it).
pub fn is_dark() -> bool {
    adw::StyleManager::default().is_dark()
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
    label(text, &["cc-caption"])
}

/// A blue text button, like a link ("Show what was read").
pub fn link_button(text: &str) -> gtk::Button {
    let b = flat_button(text);
    b.add_css_class("cc-link");
    b
}

/// A panel with a bold title inside it, then `child` (Home's Shortcuts, Working on, Make today's capsules).
pub fn titled_card(title: &str, child: &impl IsA<gtk::Widget>) -> gtk::Box {
    let inner = vbox(10);
    inner.append(&label(title, &["cc-card-title"]));
    inner.append(child);
    card(&inner, 16)
}

/// Rows with no box of their own (inside a panel).
pub fn plain_list() -> gtk::ListBox {
    let l = gtk::ListBox::new();
    l.set_selection_mode(gtk::SelectionMode::None);
    l.add_css_class("cc-plain");
    l
}

/// A settings group as on the Mac: its name, then its rows in one rounded box (a note can go under it).
pub fn group(title: &str) -> (gtk::Box, gtk::ListBox) {
    let outer = vbox(8);
    outer.append(&heading(title));
    let list = boxed_list();
    outer.append(&list);
    (outer, list)
}

/// Small grey text under a group.
pub fn note(text: &str) -> gtk::Label {
    let l = label(text, &["dim-label", "caption"]);
    l.set_margin_start(4);
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
        Section::Home => "go-home-symbolic",
        Section::Timeline => "view-list-symbolic",
        Section::Capsules => "text-x-generic-symbolic",
        Section::Projects => "folder-symbolic",
        Section::Settings => "emblem-system-symbolic",
        Section::Help => "help-about-symbolic",
    };
    let b = hbox(10);
    b.append(&gtk::Image::from_icon_name(icon));
    b.append(&line(s.title(), &[]));
    let r = gtk::ListBoxRow::new();
    r.set_child(Some(&b));
    r
}

/// A sidebar group's name.
pub fn sidebar_heading(text: &str, first: bool) -> gtk::Label {
    let l = line(text, &["cc-side-heading"]);
    l.set_margin_start(10);
    l.set_margin_top(if first { 4 } else { 14 });
    l.set_margin_bottom(4);
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
