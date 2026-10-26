// context-capsule — linux: the little GTK the shared window still shows itself — its dialogs' colours (the Mac's
// blue and backgrounds, light or dark as Adwaita shows now), a label, a column and a plain message.

use adw::prelude::*;
use std::cell::RefCell;

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

thread_local! {
    static PROVIDER: RefCell<Option<gtk::CssProvider>> = const { RefCell::new(None) };
}

/// The colours, for light or dark (as Adwaita shows now); loaded again whenever that changes.
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
    PROVIDER.with(|p| {
        if let Some(p) = p.borrow().as_ref() {
            p.load_from_data(&css);
        }
    });
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

pub fn vbox(spacing: i32) -> gtk::Box {
    gtk::Box::new(gtk::Orientation::Vertical, spacing)
}

/// A message with one OK button.
pub fn message(win: &adw::ApplicationWindow, heading: &str, body: &str) {
    let d = adw::MessageDialog::new(Some(win), Some(heading), Some(body));
    d.add_response("ok", "OK");
    d.set_default_response(Some("ok"));
    d.present();
}
