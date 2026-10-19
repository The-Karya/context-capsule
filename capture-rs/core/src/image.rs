// context-capsule — core: crop a BGRA frame to the OCR region and paint private windows black.
//! The copy lives only as long as the OCR call; frames are never written to disk.

use crate::geometry::Rect;

/// A tightly packed BGRA image (`stride` = `width * 4`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    pub width: usize,
    pub height: usize,
    pub stride: usize,
    pub bgra: Vec<u8>,
}

impl Image {
    pub fn pixel(&self, x: usize, y: usize) -> [u8; 4] {
        let i = y * self.stride + x * 4;
        [self.bgra[i], self.bgra[i + 1], self.bgra[i + 2], self.bgra[i + 3]]
    }
}

/// Pixels a mask grows on each side, for rounding and window shadows.
pub const MASK_GROW: f64 = 2.0;

/// Copy `region` (normalized, top-left origin) out of a BGRA frame, with every mask (normalized to
/// the whole frame) painted opaque black. None if the frame is malformed or the region has no pixels.
pub fn crop_masked(bgra: &[u8], width: usize, height: usize, stride: usize, region: Rect, masks: &[Rect]) -> Option<Image> {
    if width == 0 || height == 0 || stride < width * 4 || bgra.len() < stride * (height - 1) + width * 4 {
        return None;
    }
    let (w, h) = (width as f64, height as f64);
    let span = |a: f64, b: f64, max: usize| -> (usize, usize) {
        let lo = a.floor().clamp(0.0, max as f64) as usize;
        let hi = b.ceil().clamp(0.0, max as f64) as usize;
        (lo, hi.max(lo))
    };
    let (x0, x1) = span(region.min_x() * w, region.max_x() * w, width);
    let (y0, y1) = span(region.min_y() * h, region.max_y() * h, height);
    if x1 == x0 || y1 == y0 {
        return None;
    }
    let (cw, ch) = (x1 - x0, y1 - y0);
    let mut out = Vec::with_capacity(cw * ch * 4);
    for y in y0..y1 {
        let row = y * stride + x0 * 4;
        out.extend_from_slice(&bgra[row..row + cw * 4]);
    }
    let mut image = Image { width: cw, height: ch, stride: cw * 4, bgra: out };
    for m in masks {
        let (mx0, mx1) = span(m.min_x() * w - MASK_GROW, m.max_x() * w + MASK_GROW, width);
        let (my0, my1) = span(m.min_y() * h - MASK_GROW, m.max_y() * h + MASK_GROW, height);
        let (mx0, mx1) = (mx0.max(x0), mx1.min(x1));
        let (my0, my1) = (my0.max(y0), my1.min(y1));
        for y in my0..my1.max(my0) {
            for x in mx0..mx1.max(mx0) {
                let i = (y - y0) * image.stride + (x - x0) * 4;
                image.bgra[i..i + 4].copy_from_slice(&[0, 0, 0, 255]);
            }
        }
    }
    Some(image)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A w×h frame where pixel (x, y) is [x, y, 7, 255], with `pad` spare bytes per row.
    fn frame(w: usize, h: usize, pad: usize) -> (Vec<u8>, usize) {
        let stride = w * 4 + pad;
        let mut b = vec![0xEE; stride * h];
        for y in 0..h {
            for x in 0..w {
                b[y * stride + x * 4..y * stride + x * 4 + 4].copy_from_slice(&[x as u8, y as u8, 7, 255]);
            }
        }
        (b, stride)
    }

    #[test]
    fn crops_top_left_region() {
        let (b, stride) = frame(10, 8, 8);
        let img = crop_masked(&b, 10, 8, stride, Rect::new(0.2, 0.25, 0.5, 0.5), &[]).unwrap();
        assert_eq!((img.width, img.height, img.stride), (5, 4, 20));
        assert_eq!(img.pixel(0, 0), [2, 2, 7, 255], "starts at x 2, y 2 (top-left origin)");
        assert_eq!(img.pixel(4, 3), [6, 5, 7, 255]);
        assert_eq!(img.bgra.len(), 5 * 4 * 4, "padding dropped");
    }

    #[test]
    fn whole_frame_and_partial_pixels() {
        let (b, stride) = frame(4, 4, 0);
        let all = crop_masked(&b, 4, 4, stride, Rect::UNIT, &[]).unwrap();
        assert_eq!(all.bgra, b);
        let part = crop_masked(&b, 4, 4, stride, Rect::new(0.3, 0.3, 0.3, 0.3), &[]).unwrap();
        assert_eq!((part.width, part.height), (2, 2), "partly covered pixels are kept");
    }

    #[test]
    fn masks_grow_and_clip() {
        let (b, stride) = frame(20, 20, 0);
        // Mask the 2×2 px at (10, 10) of the whole frame; grown by 2 px → 8..14 on both axes.
        let mask = Rect::new(0.5, 0.5, 0.1, 0.1);
        let img = crop_masked(&b, 20, 20, stride, Rect::new(0.25, 0.25, 0.5, 0.5), &[mask]).unwrap();
        assert_eq!((img.width, img.height), (10, 10)); // frame 5..15
        for y in 0..10 {
            for x in 0..10 {
                let (fx, fy) = (x + 5, y + 5);
                let inside = (8..14).contains(&fx) && (8..14).contains(&fy);
                let p = img.pixel(x, y);
                if inside {
                    assert_eq!(p, [0, 0, 0, 255], "masked at {fx},{fy}");
                } else {
                    assert_eq!(p, [fx as u8, fy as u8, 7, 255], "kept at {fx},{fy}");
                }
            }
        }
        // A mask outside the crop changes nothing; one over everything blacks it all out.
        let off = crop_masked(&b, 20, 20, stride, Rect::new(0.0, 0.0, 0.25, 0.25), &[Rect::new(0.8, 0.8, 0.1, 0.1)]).unwrap();
        assert!(off.bgra.chunks(4).all(|p| p != [0, 0, 0, 255]));
        let black = crop_masked(&b, 20, 20, stride, Rect::UNIT, &[Rect::UNIT]).unwrap();
        assert!(black.bgra.chunks(4).all(|p| p == [0, 0, 0, 255]));
    }

    #[test]
    fn rejects_bad_input() {
        let (b, stride) = frame(4, 4, 0);
        assert!(crop_masked(&b, 4, 4, stride, Rect::ZERO, &[]).is_none(), "empty region");
        assert!(crop_masked(&b, 4, 4, stride, Rect::new(2.0, 2.0, 1.0, 1.0), &[]).is_none(), "region off the frame");
        assert!(crop_masked(&b[..10], 4, 4, stride, Rect::UNIT, &[]).is_none(), "short buffer");
        assert!(crop_masked(&b, 4, 4, 8, Rect::UNIT, &[]).is_none(), "stride too small");
        assert!(crop_masked(&[], 0, 0, 0, Rect::UNIT, &[]).is_none(), "no pixels");
    }
}
