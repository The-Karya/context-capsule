// context-capsule — windows: the Context Capsule window's colors — the Mac app's light and dark palettes and
// macOS's blue accent — and the outline of each filled shape. Plain values, so they build and test on any OS; `paint`
// draws with them.

use crate::layout::Ink;
use context_capsule_core::settings::Appearance;
use context_capsule_core::timeline::Colour;

/// ARGB.
pub type Argb = u32;

/// `a` over `b`, `t` of the way (0 = b, 1 = a).
pub fn mix(a: Argb, b: Argb, t: f64) -> Argb {
    let ch = |c: Argb, s: u32| ((c >> s) & 0xFF) as f64;
    let m = |s| (ch(a, s) * t + ch(b, s) * (1.0 - t)).round() as u32;
    0xFF00_0000 | (m(16) << 16) | (m(8) << 8) | m(0)
}

/// The Mac app's colors for one mode (macOS light and dark: white or dark grey window, a light panel for the
/// sidebar, grey rounded cards), and the accent: macOS's default blue. Every surface of the window is painted
/// from these: the page (`bg`), the sidebar, cards, controls and text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Theme {
    pub dark: bool,
    /// The whole window behind everything else.
    pub bg: Argb,
    /// The sidebar's rounded panel, and its edge.
    pub sidebar: Argb,
    pub sidebar_border: Argb,
    pub card: Argb,
    pub card_border: Argb,
    pub divider: Argb,
    pub text: Argb,
    pub text2: Argb,
    pub text3: Argb,
    pub accent: Argb,
    /// What the Mac paints for the accent's fills: prominent buttons and switches, and the sidebar's selection.
    pub accent_fill: Argb,
    pub selection: Argb,
    pub on_accent: Argb,
    /// Links ("Show what was read", "Change").
    pub link: Argb,
    pub subtle_hover: Argb,
    pub subtle_selected: Argb,
    /// A bordered button (grey, like the Mac's).
    pub control: Argb,
    pub control_hover: Argb,
    pub control_pressed: Argb,
    /// The edge of text boxes.
    pub control_border: Argb,
    pub field: Argb,
    /// Search boxes (on the page).
    pub search: Argb,
    /// The empty part of a bar.
    pub track: Argb,
    pub red: Argb,
    pub key: Argb,
}

/// macOS's default accent (blue), light and dark.
pub const MAC_BLUE: Argb = 0xFF00_7AFF;
pub const MAC_BLUE_DARK: Argb = 0xFF0A_84FF;

impl Theme {
    /// `accent`: another accent for this mode; None: macOS's blue, as the Mac app.
    pub fn new(dark: bool, accent: Option<Argb>) -> Theme {
        if dark {
            Theme {
                dark,
                bg: 0xFF28_2828,
                sidebar: 0xFF27_2727,
                sidebar_border: 0xFF3E_3E3E,
                card: 0xFF36_3636,
                card_border: 0xFF44_4444,
                divider: 0xFF49_4949,
                text: 0xFFDF_DFDF,
                text2: 0xFFA5_A5A5,
                text3: 0xFF71_7173,
                accent: accent.unwrap_or(MAC_BLUE_DARK),
                accent_fill: accent.unwrap_or(0xFF41_8FF8),
                selection: accent.unwrap_or(0xFF2E_70D4),
                on_accent: 0xFFFF_FFFF,
                link: accent.unwrap_or(0xFF41_9CFF),
                subtle_hover: 0xFF32_3232,
                subtle_selected: 0xFF3A_3A3A,
                control: 0xFF47_4747,
                control_hover: 0xFF4E_4E4E,
                control_pressed: 0xFF55_5555,
                control_border: 0xFF55_5555,
                field: 0xFF42_4242,
                search: 0xFF28_2828,
                track: 0xFF46_4646,
                red: 0xFFFF_4245,
                key: 0xFF3F_3F3F,
            }
        } else {
            Theme {
                dark,
                bg: 0xFFFF_FFFF,
                sidebar: 0xFFFA_FAFA,
                sidebar_border: 0xFFE8_E8E8,
                card: 0xFFF6_F6F6,
                card_border: 0xFFEA_EAEA,
                divider: 0xFFE6_E6E6,
                text: 0xFF26_2626,
                text2: 0xFF6B_6B6B,
                text3: 0xFFC9_C9CB,
                accent: accent.unwrap_or(MAC_BLUE),
                accent_fill: accent.unwrap_or(0xFF3B_86F1),
                selection: accent.unwrap_or(0xFF34_7AE1),
                on_accent: 0xFFFF_FFFF,
                link: accent.unwrap_or(0xFF00_68DA),
                subtle_hover: 0xFFF0_F0F0,
                subtle_selected: 0xFFE6_E6E6,
                control: 0xFFE9_E9E9,
                control_hover: 0xFFE2_E2E2,
                control_pressed: 0xFFDA_DADA,
                control_border: 0xFFE0_E0E0,
                field: 0xFFE7_E7E7,
                search: 0xFFFF_FFFF,
                track: 0xFFE9_E9E9,
                red: 0xFFFF_383C,
                key: 0xFFED_EDED,
            }
        }
    }

    /// The palette for the Appearance setting, given whether Windows is dark (apps' setting) and an accent.
    pub fn for_appearance(appearance: Appearance, system_dark: bool, accent: impl Fn(bool) -> Option<Argb>) -> Theme {
        let dark = appearance.is_dark(system_dark);
        Theme::new(dark, accent(dark))
    }

    /// A project's colour on the Timeline and Home: macOS 26's system colours (blue, teal, orange, green, pink and
    /// brown, their dark-mode versions in dark); "Not sorted" grey. No purple.
    pub fn colour(&self, c: Colour) -> Argb {
        match (c, self.dark) {
            (Colour::Blue, false) => 0xFF00_88FF,
            (Colour::Blue, true) => 0xFF00_91FF,
            (Colour::Teal, false) => 0xFF00_C3D0,
            (Colour::Teal, true) => 0xFF00_D2E0,
            (Colour::Orange, false) => 0xFFFF_8D28,
            (Colour::Orange, true) => 0xFFFF_9230,
            (Colour::Green, false) => 0xFF34_C759,
            (Colour::Green, true) => 0xFF30_D158,
            (Colour::Pink, false) => 0xFFFF_2D55,
            (Colour::Pink, true) => 0xFFFF_375F,
            (Colour::Brown, false) => 0xFFAC_7F5E,
            (Colour::Brown, true) => 0xFFB7_8A66,
            (Colour::Grey, _) => self.text3,
        }
    }

    pub fn ink(&self, ink: Ink) -> Argb {
        match ink {
            Ink::Primary => self.text,
            Ink::Secondary => self.text2,
        }
    }
}

/// Windows is dark for apps when "AppsUseLightTheme" is 0 (missing: light, as on Windows 10 before dark mode).
pub fn system_dark_from(apps_use_light_theme: Option<u32>) -> bool {
    apps_use_light_theme == Some(0)
}

/// How a filled shape is outlined. Square corners must be a plain rectangle: GDI+ refuses arcs of size 0, so a
/// "rounded" path with radius 0 is empty and fills nothing (that left the page unpainted, black, in light mode).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Outline {
    Rect,
    /// Corner arcs of this diameter.
    Rounded(f32),
}

impl Outline {
    pub fn of(w: f32, h: f32, radius: f32) -> Outline {
        let d = radius.min(w / 2.0).min(h / 2.0).max(0.0) * 2.0;
        if d < 0.5 {
            Outline::Rect
        } else {
            Outline::Rounded(d)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// WCAG relative luminance.
    fn lum(c: Argb) -> f64 {
        let f = |s: u32| {
            let v = ((c >> s) & 0xFF) as f64 / 255.0;
            if v <= 0.03928 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * f(16) + 0.7152 * f(8) + 0.0722 * f(0)
    }

    fn contrast(a: Argb, b: Argb) -> f64 {
        let (x, y) = (lum(a), lum(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    #[test]
    fn light_is_light_and_dark_is_dark() {
        let light = Theme::new(false, None);
        assert!(lum(light.bg) > 0.85, "light page: white, not black");
        assert!(lum(light.card) < lum(light.bg) && lum(light.card) > 0.85, "light grey cards on a white page");
        assert!(lum(light.text) < 0.05, "dark text");
        let dark = Theme::new(true, None);
        assert!(lum(dark.bg) < 0.05 && lum(dark.card) > lum(dark.bg), "dark page, slightly lighter cards");
        assert!(lum(dark.text) > 0.7, "light text (the Mac's label colour: white at 85 %)");
        for t in [light, dark] {
            for (what, ink) in [("text", t.text), ("secondary text", t.text2)] {
                for (on, surface) in [("page", t.bg), ("card", t.card), ("sidebar", t.sidebar), ("hovered row", t.subtle_hover)] {
                    let c = contrast(ink, surface);
                    assert!(c >= 4.5, "dark={} {what} on the {on}: contrast {c:.1}", t.dark);
                }
            }
            // macOS's own blue with white labels, as on the Mac (WCAG 3:1, as for large text and controls).
            for fill in [t.accent_fill, t.selection] {
                assert!(contrast(t.on_accent, fill) >= 3.0, "dark={} white labels on the accent", t.dark);
            }
            assert_ne!(t.selection, t.sidebar, "the selected sidebar item stands out");
        }
    }

    #[test]
    fn timeline_colours_show_and_differ() {
        for t in [Theme::new(false, None), Theme::new(true, None)] {
            let all: Vec<Argb> = Colour::ORDER.iter().map(|c| t.colour(*c)).collect();
            for (i, c) in all.iter().enumerate() {
                assert!(!all[..i].contains(c), "dark={}: each project its own colour", t.dark);
                // macOS 26's own colours (its teal and "Not sorted" grey are light, as on the Mac).
                assert!(contrast(*c, t.card) >= 1.4, "dark={}: {c:08x} shows on a card", t.dark);
                let (r, g, b) = ((c >> 16) & 0xFF, (c >> 8) & 0xFF, c & 0xFF);
                assert!(!(b > g + 40 && r > g + 40 && b > r), "dark={}: no purple {c:08x}", t.dark);
            }
            assert_eq!(t.colour(Colour::Grey), t.text3);
        }
    }

    #[test]
    fn the_setting_or_windows_decides() {
        let accent = |dark: bool| Some(if dark { 0xFF11_1111 } else { 0xFF22_2222 });
        assert!(!Theme::for_appearance(Appearance::System, false, accent).dark);
        assert!(Theme::for_appearance(Appearance::System, true, accent).dark);
        let light = Theme::for_appearance(Appearance::Light, true, accent);
        assert_eq!((light.dark, light.bg, light.accent), (false, 0xFFFF_FFFF, 0xFF22_2222), "Light wins over a dark Windows");
        let dark = Theme::for_appearance(Appearance::Dark, false, accent);
        assert_eq!((dark.dark, dark.bg, dark.accent), (true, 0xFF28_2828, 0xFF11_1111));
        assert_eq!((Theme::new(false, None).accent, Theme::new(true, None).accent), (MAC_BLUE, MAC_BLUE_DARK), "macOS's blue by default");
        assert!(!system_dark_from(Some(1)) && system_dark_from(Some(0)) && !system_dark_from(None));
    }

    #[test]
    fn square_shapes_are_rectangles() {
        assert_eq!(Outline::of(1200.0, 800.0, 0.0), Outline::Rect, "the page background");
        assert_eq!(Outline::of(500.0, 1.0, 0.0), Outline::Rect, "a divider");
        assert_eq!(Outline::of(300.0, 100.0, 8.0), Outline::Rounded(16.0));
        assert_eq!(Outline::of(10.0, 4.0, 8.0), Outline::Rounded(4.0), "never rounder than the shape");
        assert_eq!(mix(0xFFFF_FFFF, 0xFF00_0000, 0.5), 0xFF80_8080);
    }
}
