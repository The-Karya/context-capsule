// context-capsule — windows: drawing the Context Capsule window — Windows 11 colors (light and dark, the system
// accent), Segoe UI Variable (Segoe UI on Windows 10) through GDI with ClearType, and smooth rounded shapes
// through GDI+. Everything is drawn into an off-screen bitmap, then copied in one go (no flicker).

use crate::layout::{Align, Font, Measure, Node, Style};
use std::collections::HashMap;
use windows::core::HSTRING;
use windows::Win32::Foundation::{COLORREF, LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    CreateFontW, CreateSolidBrush, DeleteObject, DrawTextW, EnumFontFamiliesExW, FillRect, GetDC, ReleaseDC, SelectObject, SetBkMode, SetTextColor, DEFAULT_CHARSET,
    DRAW_TEXT_FORMAT, DT_CALCRECT, DT_CENTER, DT_EDITCONTROL, DT_END_ELLIPSIS, DT_LEFT, DT_NOPREFIX, DT_RIGHT, DT_SINGLELINE, DT_VCENTER,
    DT_WORDBREAK, FONT_QUALITY, HDC, HFONT, LOGFONTW, TEXTMETRICW, TRANSPARENT,
};
use windows::Win32::Graphics::GdiPlus::{
    GdipAddPathArc, GdipAddPathRectangle, GdipClosePathFigure, GdipCreateFromHDC, GdipCreatePath, GdipCreatePen1, GdipCreateSolidFill, GdipDeleteBrush,
    GdipDeleteGraphics, GdipDeletePath, GdipDeletePen, GdipDrawPath, GdipFillPath, GdipSetPixelOffsetMode, GdipSetSmoothingMode,
    GdiplusStartup, GdiplusStartupInput, GpBrush, GpGraphics, GpPath, FillModeAlternate, PixelOffsetModeHalf, SmoothingModeAntiAlias,
    UnitPixel,
};

pub use crate::palette::{mix, Argb, Theme};
use crate::palette::Outline;

pub fn colorref(c: Argb) -> COLORREF {
    COLORREF(((c & 0xFF) << 16) | (c & 0xFF00) | ((c >> 16) & 0xFF))
}

/// Fill a rectangle (pixels) with plain GDI: exact edges, no GDI+ involved. Used for the page background.
pub fn fill_rect(dc: HDC, r: RECT, color: Argb) {
    // SAFETY: a brush we make and delete here, on a valid DC.
    unsafe {
        let brush = CreateSolidBrush(colorref(color));
        FillRect(dc, &r, brush);
        let _ = DeleteObject(brush.into());
    }
}

// MARK: GDI+

static GDIPLUS: std::sync::Once = std::sync::Once::new();

/// Start GDI+ once for the process.
pub fn start_gdiplus() {
    GDIPLUS.call_once(|| {
        let input = GdiplusStartupInput { GdiplusVersion: 1, ..Default::default() };
        let mut token = 0usize;
        // SAFETY: plain startup; the token is kept for the life of the process (never shut down).
        unsafe {
            let _ = GdiplusStartup(&mut token, &input, std::ptr::null_mut());
        }
    });
}

/// A GDI+ surface on a DC, for anti-aliased shapes. Dropped (and flushed) before GDI text is drawn over it.
pub struct Shapes(*mut GpGraphics);

impl Shapes {
    pub fn on(dc: HDC) -> Shapes {
        let mut g = std::ptr::null_mut();
        // SAFETY: a graphics object on a valid DC, deleted in Drop.
        unsafe {
            let _ = GdipCreateFromHDC(dc, &mut g);
            if !g.is_null() {
                let _ = GdipSetSmoothingMode(g, SmoothingModeAntiAlias);
                let _ = GdipSetPixelOffsetMode(g, PixelOffsetModeHalf);
            }
        }
        Shapes(g)
    }

    fn path(x: f32, y: f32, w: f32, h: f32, r: f32) -> *mut GpPath {
        let mut p = std::ptr::null_mut();
        // SAFETY: a path we own; deleted by the caller.
        unsafe {
            let _ = GdipCreatePath(FillModeAlternate, &mut p);
            if p.is_null() {
                return p;
            }
            match Outline::of(w, h, r) {
                // Square corners: a rectangle (GDI+ refuses zero-size arcs, which left the path empty).
                Outline::Rect => {
                    let _ = GdipAddPathRectangle(p, x, y, w, h);
                }
                Outline::Rounded(d) => {
                    let _ = GdipAddPathArc(p, x, y, d, d, 180.0, 90.0);
                    let _ = GdipAddPathArc(p, x + w - d, y, d, d, 270.0, 90.0);
                    let _ = GdipAddPathArc(p, x + w - d, y + h - d, d, d, 0.0, 90.0);
                    let _ = GdipAddPathArc(p, x, y + h - d, d, d, 90.0, 90.0);
                    let _ = GdipClosePathFigure(p);
                }
            }
        }
        p
    }

    pub fn fill(&self, r: [f32; 4], radius: f32, color: Argb) {
        if self.0.is_null() || r[2] <= 0.0 || r[3] <= 0.0 {
            return;
        }
        let p = Self::path(r[0], r[1], r[2], r[3], radius);
        let mut brush = std::ptr::null_mut();
        // SAFETY: our own graphics, path and brush, all deleted here.
        unsafe {
            let _ = GdipCreateSolidFill(color, &mut brush);
            if !brush.is_null() && !p.is_null() {
                let _ = GdipFillPath(self.0, brush as *mut GpBrush, p);
            }
            if !brush.is_null() {
                let _ = GdipDeleteBrush(brush as *mut GpBrush);
            }
            if !p.is_null() {
                let _ = GdipDeletePath(p);
            }
        }
    }

    /// An outline `width` thick, inside `r`.
    pub fn stroke(&self, r: [f32; 4], radius: f32, color: Argb, width: f32) {
        if self.0.is_null() || r[2] <= width || r[3] <= width {
            return;
        }
        let half = width / 2.0;
        let p = Self::path(r[0] + half, r[1] + half, r[2] - width, r[3] - width, (radius - half).max(0.0));
        let mut pen = std::ptr::null_mut();
        // SAFETY: as in `fill`.
        unsafe {
            let _ = GdipCreatePen1(color, width, UnitPixel, &mut pen);
            if !pen.is_null() && !p.is_null() {
                let _ = GdipDrawPath(self.0, pen, p);
            }
            if !pen.is_null() {
                let _ = GdipDeletePen(pen);
            }
            if !p.is_null() {
                let _ = GdipDeletePath(p);
            }
        }
    }
}

impl Drop for Shapes {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: created in `on`.
            unsafe {
                let _ = GdipDeleteGraphics(self.0);
            }
        }
    }
}

// MARK: fonts

/// Is this font family installed?
pub fn has_font(face: &str) -> bool {
    unsafe extern "system" fn found(_: *const LOGFONTW, _: *const TEXTMETRICW, _: u32, lp: LPARAM) -> i32 {
        // SAFETY: `lp` is the `&mut bool` passed below.
        unsafe { *(lp.0 as *mut bool) = true };
        0
    }
    let mut lf = LOGFONTW { lfCharSet: DEFAULT_CHARSET, ..Default::default() };
    for (dst, src) in lf.lfFaceName.iter_mut().zip(face.encode_utf16().take(31)) {
        *dst = src;
    }
    let mut yes = false;
    // SAFETY: the screen DC, released below; the callback writes only `yes`.
    unsafe {
        let dc = GetDC(None);
        EnumFontFamiliesExW(dc, &lf, Some(found), LPARAM(&mut yes as *mut bool as isize), 0);
        ReleaseDC(None, dc);
    }
    yes
}

/// Which faces this PC has (checked once).
#[derive(Clone, Debug)]
pub struct Faces {
    /// Windows 11's Segoe UI Variable.
    variable: bool,
    pub icons: &'static str,
}

impl Faces {
    pub fn detect() -> Faces {
        Faces {
            variable: has_font("Segoe UI Variable Text"),
            icons: if has_font("Segoe Fluent Icons") { "Segoe Fluent Icons" } else { "Segoe MDL2 Assets" },
        }
    }

    fn face(&self, font: Font) -> &'static str {
        let (_, weight, display) = font.spec();
        if font.is_icon() {
            return self.icons;
        }
        match (self.variable, display, weight) {
            (true, true, _) => "Segoe UI Variable Display",
            (true, false, _) => "Segoe UI Variable Text",
            (false, _, w) if w <= 300 => "Segoe UI Light",
            (false, _, w) if w >= 600 => "Segoe UI Semibold",
            _ => "Segoe UI",
        }
    }
}

/// Fonts at one DPI scale.
pub struct Fonts {
    pub scale: f64,
    faces: Faces,
    made: HashMap<Font, HFONT>,
}

impl Fonts {
    pub fn new(scale: f64, faces: Faces) -> Fonts {
        Fonts { scale, faces, made: HashMap::new() }
    }

    pub fn get(&mut self, font: Font) -> HFONT {
        if let Some(f) = self.made.get(&font) {
            return *f;
        }
        let (size, weight, _) = font.spec();
        let face = self.faces.face(font);
        // SAFETY: a font we keep and delete in Drop.
        let f = unsafe {
            CreateFontW(
                -((size * self.scale).round() as i32),
                0,
                0,
                0,
                weight,
                0,
                0,
                0,
                DEFAULT_CHARSET,
                Default::default(),
                Default::default(),
                FONT_QUALITY(5), // ClearType
                0,
                &HSTRING::from(face),
            )
        };
        self.made.insert(font, f);
        f
    }
}

impl Drop for Fonts {
    fn drop(&mut self) {
        for (_, f) in self.made.drain() {
            // SAFETY: fonts we made; nothing selects them any more.
            unsafe {
                let _ = DeleteObject(f.into());
            }
        }
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

fn flags(lines: u8, align: Align) -> DRAW_TEXT_FORMAT {
    let a = match align {
        Align::Left => DT_LEFT,
        Align::Center => DT_CENTER,
        Align::Right => DT_RIGHT,
    };
    let base = DT_NOPREFIX | a;
    match lines {
        1 => base | DT_SINGLELINE | DT_END_ELLIPSIS | DT_VCENTER,
        0 => base | DT_WORDBREAK | DT_EDITCONTROL,
        _ => base | DT_WORDBREAK | DT_EDITCONTROL | DT_END_ELLIPSIS,
    }
}

/// Measures with GDI on a DC (in DIPs).
pub struct GdiMeasure<'a> {
    pub dc: HDC,
    pub fonts: std::cell::RefCell<&'a mut Fonts>,
}

impl Measure for GdiMeasure<'_> {
    fn text(&self, text: &str, font: Font, width: f64, lines: u8) -> (f64, f64) {
        let mut fonts = self.fonts.borrow_mut();
        let scale = fonts.scale;
        let f = fonts.get(font);
        let mut w = wide(if text.is_empty() { " " } else { text });
        let line_h;
        // SAFETY: measuring on our DC with our font.
        unsafe {
            let old = SelectObject(self.dc, f.into());
            let mut one = RECT { left: 0, top: 0, right: 100_000, bottom: 0 };
            DrawTextW(self.dc, &mut wide("Ag"), &mut one, DT_CALCRECT | DT_SINGLELINE | DT_NOPREFIX);
            line_h = one.bottom as f64;
            let mut r = RECT { left: 0, top: 0, right: if lines == 1 { 100_000 } else { (width * scale).max(1.0) as i32 }, bottom: 0 };
            let how = if lines == 1 { DT_CALCRECT | DT_SINGLELINE | DT_NOPREFIX } else { DT_CALCRECT | DT_WORDBREAK | DT_EDITCONTROL | DT_NOPREFIX };
            DrawTextW(self.dc, &mut w, &mut r, how);
            SelectObject(self.dc, old);
            let mut h = r.bottom as f64;
            if lines > 1 {
                h = h.min(line_h * lines as f64);
            }
            if lines == 1 {
                h = line_h;
            }
            (((r.right as f64) / scale).ceil().min(width), (h / scale).ceil())
        }
    }
}

/// Draw text in a rect (pixels).
#[allow(clippy::too_many_arguments)]
pub fn text(dc: HDC, fonts: &mut Fonts, r: [f32; 4], s: &str, font: Font, color: Argb, align: Align, lines: u8) {
    let mut rect = RECT { left: r[0].round() as i32, top: r[1].round() as i32, right: (r[0] + r[2]).round() as i32, bottom: (r[1] + r[3]).round() as i32 };
    let mut w = wide(s);
    // SAFETY: drawing on our DC with our font.
    unsafe {
        let old = SelectObject(dc, fonts.get(font).into());
        SetBkMode(dc, TRANSPARENT);
        SetTextColor(dc, colorref(color));
        DrawTextW(dc, &mut w, &mut rect, flags(lines, align));
        SelectObject(dc, old);
    }
}

/// What the pointer and keyboard are doing to one control.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Feel {
    pub hover: bool,
    pub pressed: bool,
    pub focused: bool,
}

/// Draw one node at `dy` pixels down, at `scale`.
pub fn node(dc: HDC, fonts: &mut Fonts, t: &Theme, n: &Node, dy: f64, feel: Feel) {
    let s = fonts.scale;
    let px = |r: crate::layout::Rect| -> [f32; 4] {
        [(r.x * s) as f32, ((r.y + dy) * s) as f32, (r.w * s) as f32, (r.h * s) as f32]
    };
    let k = s as f32;
    match n {
        Node::Card(r) => {
            let g = Shapes::on(dc);
            g.fill(px(*r), 8.0 * k, t.card_border);
            let p = px(*r);
            g.fill([p[0] + k, p[1] + k, p[2] - 2.0 * k, p[3] - 2.0 * k], 7.0 * k, t.card);
        }
        Node::Divider(r) => {
            let p = px(*r);
            let g = Shapes::on(dc);
            g.fill([p[0], p[1], p[2].max(1.0), p[3].max(1.0)], 0.0, t.divider);
        }
        Node::Text { r, text: s2, font, ink, align, lines } => {
            text(dc, fonts, px(*r), s2, *font, t.ink(*ink), *align, *lines);
        }
        Node::Dot { r, on } => {
            let g = Shapes::on(dc);
            let p = px(*r);
            if *on {
                g.fill(p, p[2] / 2.0, t.red);
            } else {
                g.stroke(p, p[2] / 2.0, t.text2, 1.5 * k);
            }
        }
        Node::Bar { r, fraction, muted } => {
            let g = Shapes::on(dc);
            let p = px(*r);
            g.fill(p, p[3] / 2.0, t.track);
            let w = (p[2] * *fraction as f32).max(6.0 * k).min(p[2]);
            g.fill([p[0], p[1], w, p[3]], p[3] / 2.0, if *muted { t.text3 } else { t.accent });
        }
        Node::Key { r, text: s2 } => {
            let p = px(*r);
            {
                let g = Shapes::on(dc);
                g.fill(p, 4.0 * k, t.control_bottom);
                g.fill([p[0] + k, p[1] + k, p[2] - 2.0 * k, p[3] - 3.0 * k], 3.0 * k, t.key);
            }
            text(dc, fonts, [p[0], p[1], p[2], p[3] - k], s2, Font::SmallStrong, t.text, Align::Center, 1);
        }
        Node::Pill { r, text: s2, live } => {
            let p = px(*r);
            {
                let g = Shapes::on(dc);
                g.fill(p, p[3] / 2.0, t.card_border);
                g.fill([p[0] + k, p[1] + k, p[2] - 2.0 * k, p[3] - 2.0 * k], p[3] / 2.0 - k, t.card);
            }
            text(dc, fonts, p, s2, Font::Small, if *live { t.red } else { t.text2 }, Align::Center, 1);
        }
        Node::Toggle { r, on, .. } => {
            let p = px(*r);
            {
                let g = Shapes::on(dc);
                let knob = if feel.hover || feel.pressed { 14.0 } else { 12.0 } * k;
                let cy = p[1] + p[3] / 2.0 - knob / 2.0;
                if *on {
                    g.fill(p, p[3] / 2.0, if feel.hover { mix(t.accent, t.bg, 0.9) } else { t.accent });
                    g.fill([p[0] + p[2] - p[3] / 2.0 - knob / 2.0, cy, knob, knob], knob / 2.0, t.on_accent);
                } else {
                    if feel.hover {
                        g.fill(p, p[3] / 2.0, t.subtle_hover);
                    }
                    g.stroke(p, p[3] / 2.0, t.text2, k);
                    g.fill([p[0] + p[3] / 2.0 - knob / 2.0, cy, knob, knob], knob / 2.0, t.text2);
                }
            }
            focus_ring(dc, t, p, p[3] / 2.0, feel, k);
        }
        Node::Field { r, .. } => {
            let p = px(*r);
            let g = Shapes::on(dc);
            g.fill(p, 4.0 * k, t.control_border);
            g.fill([p[0] + k, p[1] + k, p[2] - 2.0 * k, p[3] - 2.0 * k], 3.0 * k, t.field);
            let line = if feel.focused { 2.0 } else { 1.0 } * k;
            let color = if feel.focused { t.accent } else { t.text3 };
            g.fill([p[0] + 2.0 * k, p[1] + p[3] - line, p[2] - 4.0 * k, line], 0.0, color);
        }
        Node::Button { r, label, icon, style, enabled, check, .. } => button(dc, fonts, t, px(*r), label, *icon, *style, *enabled, *check, feel, k),
    }
}

fn focus_ring(dc: HDC, t: &Theme, p: [f32; 4], radius: f32, feel: Feel, k: f32) {
    if feel.focused {
        let g = Shapes::on(dc);
        g.stroke([p[0] - 3.0 * k, p[1] - 3.0 * k, p[2] + 6.0 * k, p[3] + 6.0 * k], radius + 3.0 * k, t.text, 2.0 * k);
    }
}

#[allow(clippy::too_many_arguments)]
fn button(dc: HDC, fonts: &mut Fonts, t: &Theme, p: [f32; 4], label: &str, icon: Option<&'static str>, style: Style, enabled: bool, check: bool, feel: Feel, k: f32) {
    let mut ink = if enabled { t.text } else { t.text3 };
    {
        let g = Shapes::on(dc);
        match style {
            Style::Accent => {
                let fill = if !enabled {
                    t.track
                } else if feel.pressed {
                    mix(t.accent, t.bg, 0.8)
                } else if feel.hover {
                    mix(t.accent, t.bg, 0.9)
                } else {
                    t.accent
                };
                g.fill(p, 4.0 * k, fill);
                ink = if enabled { t.on_accent } else { t.text3 };
            }
            Style::Standard | Style::Dropdown => {
                let fill = if feel.pressed { t.control_pressed } else if feel.hover && enabled { t.control_hover } else { t.control };
                g.fill(p, 4.0 * k, t.control_bottom);
                g.fill([p[0], p[1], p[2], p[3] - k], 4.0 * k, t.control_border);
                g.fill([p[0] + k, p[1] + k, p[2] - 2.0 * k, p[3] - 2.0 * k], 3.0 * k, fill);
                if feel.pressed {
                    ink = t.text2;
                }
            }
            Style::Subtle => {
                if enabled && (feel.hover || feel.pressed) {
                    g.fill(p, 4.0 * k, if feel.pressed { t.subtle_selected } else { t.subtle_hover });
                }
                if icon.is_none() {
                    ink = t.accent; // a link, like "See all"
                }
            }
            Style::Check { on } => {
                if feel.hover || feel.pressed {
                    g.fill(p, 4.0 * k, if feel.pressed { t.subtle_selected } else { t.subtle_hover });
                }
                let b = [p[0] + 10.0 * k, p[1] + (p[3] - 18.0 * k) / 2.0, 18.0 * k, 18.0 * k];
                if on {
                    g.fill(b, 4.0 * k, t.accent);
                } else {
                    g.fill(b, 4.0 * k, t.field);
                    g.stroke(b, 4.0 * k, t.text2, k);
                }
            }
            Style::Nav { selected } | Style::Row { selected } => {
                if selected {
                    g.fill(p, 4.0 * k, t.subtle_selected);
                } else if feel.hover || feel.pressed {
                    g.fill(p, 4.0 * k, t.subtle_hover);
                }
                if selected && matches!(style, Style::Nav { .. }) {
                    let h = 16.0 * k;
                    g.fill([p[0], p[1] + (p[3] - h) / 2.0, 3.0 * k, h], 1.5 * k, t.accent);
                }
            }
        }
    }
    let radius = 4.0 * k;
    match style {
        Style::Nav { .. } => {
            if let Some(i) = icon {
                text(dc, fonts, [p[0] + 12.0 * k, p[1], 20.0 * k, p[3]], i, Font::Icon, ink, Align::Center, 1);
            }
            text(dc, fonts, [p[0] + 44.0 * k, p[1], p[2] - 52.0 * k, p[3]], label, Font::Body, ink, Align::Left, 1);
        }
        Style::Row { .. } => {
            let mut x = p[0] + 12.0 * k;
            if let Some(i) = icon {
                text(dc, fonts, [x, p[1], 16.0 * k, p[3]], i, Font::IconSmall, ink, Align::Center, 1);
                x += 24.0 * k;
            }
            text(dc, fonts, [x, p[1], p[0] + p[2] - x - 36.0 * k, p[3]], label, Font::Body, ink, Align::Left, 1);
            if check {
                text(dc, fonts, [p[0] + p[2] - 32.0 * k, p[1], 20.0 * k, p[3]], crate::layout::glyph::CHECK, Font::IconSmall, t.accent, Align::Center, 1);
            }
        }
        Style::Check { on } => {
            if on {
                text(dc, fonts, [p[0] + 10.0 * k, p[1], 18.0 * k, p[3]], crate::layout::glyph::CHECK, Font::IconSmall, t.on_accent, Align::Center, 1);
            }
            text(dc, fonts, [p[0] + 40.0 * k, p[1], p[2] - 48.0 * k, p[3]], label, Font::Body, ink, Align::Left, 1);
        }
        Style::Dropdown => {
            text(dc, fonts, [p[0] + 12.0 * k, p[1], p[2] - 44.0 * k, p[3]], label, Font::Body, ink, Align::Left, 1);
            text(dc, fonts, [p[0] + p[2] - 30.0 * k, p[1], 16.0 * k, p[3]], crate::layout::glyph::CHEVRON, Font::IconSmall, t.text2, Align::Center, 1);
        }
        _ => {
            if label.is_empty() {
                if let Some(i) = icon {
                    text(dc, fonts, p, i, Font::Icon, ink, Align::Center, 1);
                }
            } else {
                let font = if style == Style::Subtle { Font::Small } else { Font::Body };
                text(dc, fonts, [p[0] + 4.0 * k, p[1], p[2] - 8.0 * k, p[3]], label, font, ink, Align::Center, 1);
            }
        }
    }
    focus_ring(dc, t, p, radius, feel, k);
}
