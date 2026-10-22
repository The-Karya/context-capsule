// context-capsule — windows: the Context Capsule window's colors — Windows 11's light and dark palettes and the
// user's accent — and the outline of each filled shape. Plain values, so they build and test on any OS; `paint`
// draws with them.

use crate::layout::Ink;
use context_capsule_core::settings::Appearance;

/// ARGB.
pub type Argb = u32;

/// `a` over `b`, `t` of the way (0 = b, 1 = a).
pub fn mix(a: Argb, b: Argb, t: f64) -> Argb {
    let ch = |c: Argb, s: u32| ((c >> s) & 0xFF) as f64;
    let m = |s| (ch(a, s) * t + ch(b, s) * (1.0 - t)).round() as u32;
    0xFF00_0000 | (m(16) << 16) | (m(8) << 8) | m(0)
}

/// Windows 11's colors for one mode, and the user's accent. Every surface of the window is painted from these:
/// the page (behind the sidebar, the header and the content: `bg`), cards, controls and text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Theme {
    pub dark: bool,
    /// The whole window behind everything else (Windows 11 Settings' page color).
    pub bg: Argb,
    pub card: Argb,
    pub card_border: Argb,
    pub divider: Argb,
    pub text: Argb,
    pub text2: Argb,
    pub text3: Argb,
    pub accent: Argb,
    pub on_accent: Argb,
    pub subtle_hover: Argb,
    pub subtle_selected: Argb,
    pub control: Argb,
    pub control_hover: Argb,
    pub control_pressed: Argb,
    pub control_border: Argb,
    pub control_bottom: Argb,
    pub field: Argb,
    pub track: Argb,
    pub red: Argb,
    pub key: Argb,
}

impl Theme {
    /// `accent`: the system accent for this mode (dark text on light accents in dark mode, like Windows).
    pub fn new(dark: bool, accent: Option<Argb>) -> Theme {
        if dark {
            Theme {
                dark,
                bg: 0xFF20_2020,
                card: 0xFF2B_2B2B,
                card_border: 0xFF1C_1C1C,
                divider: 0xFF3A_3A3A,
                text: 0xFFFF_FFFF,
                text2: 0xFFC8_C8C8,
                text3: 0xFF9A_9A9A,
                accent: accent.unwrap_or(0xFF60_CDFF),
                on_accent: 0xFF00_0000,
                subtle_hover: 0xFF2D_2D2D,
                subtle_selected: 0xFF33_3333,
                control: 0xFF2D_2D2D,
                control_hover: 0xFF32_3232,
                control_pressed: 0xFF27_2727,
                control_border: 0xFF3A_3A3A,
                control_bottom: 0xFF30_3030,
                field: 0xFF2D_2D2D,
                track: 0xFF3D_3D3D,
                red: 0xFFFF_99A4,
                key: 0xFF32_3232,
            }
        } else {
            Theme {
                dark,
                bg: 0xFFF3_F3F3,
                card: 0xFFFB_FBFB,
                card_border: 0xFFE5_E5E5,
                divider: 0xFFEA_EAEA,
                text: 0xFF1B_1B1B,
                text2: 0xFF5D_5D5D,
                text3: 0xFF8A_8A8A,
                accent: accent.unwrap_or(0xFF00_5FB8),
                on_accent: 0xFFFF_FFFF,
                subtle_hover: 0xFFEA_EAEA,
                subtle_selected: 0xFFE6_E6E6,
                control: 0xFFFB_FBFB,
                control_hover: 0xFFF6_F6F6,
                control_pressed: 0xFFF2_F2F2,
                control_border: 0xFFE5_E5E5,
                control_bottom: 0xFFCC_CCCC,
                field: 0xFFFF_FFFF,
                track: 0xFFE6_E6E6,
                red: 0xFFC4_2B1C,
                key: 0xFFF3_F3F3,
            }
        }
    }

    /// The palette for the Appearance setting, given whether Windows is dark (apps' setting) and its accent.
    pub fn for_appearance(appearance: Appearance, system_dark: bool, accent: impl Fn(bool) -> Option<Argb>) -> Theme {
        let dark = appearance.is_dark(system_dark);
        Theme::new(dark, accent(dark))
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
        assert!(lum(light.bg) > 0.85, "light page: light grey, not black");
        assert!(lum(light.card) >= lum(light.bg), "white cards on a light grey page");
        assert!(lum(light.text) < 0.05, "dark text");
        let dark = Theme::new(true, None);
        assert!(lum(dark.bg) < 0.05 && lum(dark.card) > lum(dark.bg), "dark page, slightly lighter cards");
        assert!(lum(dark.text) > 0.9);
        for t in [light, dark] {
            for (what, ink) in [("text", t.text), ("secondary text", t.text2)] {
                for (on, surface) in [("page", t.bg), ("card", t.card), ("selected row", t.subtle_selected), ("hovered row", t.subtle_hover)] {
                    let c = contrast(ink, surface);
                    assert!(c >= 4.5, "dark={} {what} on the {on}: contrast {c:.1}", t.dark);
                }
            }
            assert!(contrast(t.on_accent, t.accent) >= 4.5, "dark={} button text on the accent", t.dark);
            assert_ne!(t.subtle_selected, t.bg, "the selected sidebar item stands out");
        }
    }

    #[test]
    fn the_setting_or_windows_decides() {
        let accent = |dark: bool| Some(if dark { 0xFF11_1111 } else { 0xFF22_2222 });
        assert!(!Theme::for_appearance(Appearance::System, false, accent).dark);
        assert!(Theme::for_appearance(Appearance::System, true, accent).dark);
        let light = Theme::for_appearance(Appearance::Light, true, accent);
        assert_eq!((light.dark, light.bg, light.accent), (false, 0xFFF3_F3F3, 0xFF22_2222), "Light wins over a dark Windows");
        let dark = Theme::for_appearance(Appearance::Dark, false, accent);
        assert_eq!((dark.dark, dark.bg, dark.accent), (true, 0xFF20_2020, 0xFF11_1111));
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
