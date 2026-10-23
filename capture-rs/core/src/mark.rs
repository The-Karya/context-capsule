// context-capsule — core: the Karya panther as the idle tray icon, the same on Windows and Linux. Ships at the
// sizes the tray uses at 100–300 % (`SIZES`, made by capture-rs/icons/make-icons.mjs: hand-drawn pixels at
// 16, 20 and 24 px, the brand's bold mark above that), so it is never scaled: a tray size in between gets the
// next smaller mark, centred, crisp.

#[path = "mark_data.rs"]
mod data;

/// The sizes the mark is drawn at.
pub const SIZES: [usize; 6] = [16, 20, 24, 32, 40, 48];

/// Which drawn size goes in a `icon`×`icon` tray icon: the largest that fits (16 when the icon is smaller).
pub fn size_for(icon: usize) -> usize {
    SIZES.iter().copied().filter(|&s| s <= icon).max().unwrap_or(SIZES[0])
}

/// The mark as coverage 0…255 of an `icon`×`icon` image, top row first: the drawn size from `size_for`,
/// centred (an odd spare pixel goes right and below). Colour it for the taskbar.
pub fn panther(icon: usize) -> Vec<u8> {
    let s = size_for(icon);
    let src = data::MARKS.iter().find(|(n, _)| *n == s).map(|(_, m)| *m).unwrap_or(&[]);
    let mut out = vec![0u8; icon * icon];
    if icon < s {
        return out;
    }
    let off = (icon - s) / 2;
    for y in 0..s {
        for x in 0..s {
            out[(y + off) * icon + x + off] = src.get(y * s + x).copied().unwrap_or(0);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_size_is_there() {
        assert_eq!(data::MARKS.map(|(s, _)| s), SIZES);
        for (s, m) in data::MARKS {
            assert_eq!(m.len(), s * s, "{s} px");
            let on = m.iter().filter(|&&a| a >= 128).count();
            assert!(on > s * s / 4 && on < s * s * 3 / 4, "{s} px: a silhouette, not empty or a block ({on} on)");
        }
    }

    #[test]
    fn small_sizes_are_pure_pixels() {
        for (s, m) in data::MARKS.iter().filter(|(s, _)| *s <= 24) {
            assert!(m.iter().all(|&a| a == 0 || a == 255), "{s} px: on or off only");
        }
    }

    /// The drawing fills its box: top to bottom, centred left to right (a pixel of slack at most).
    #[test]
    fn centred_and_filling() {
        for (s, m) in data::MARKS {
            let on = |x: usize, y: usize| m[y * s + x] > 0;
            let cols: Vec<usize> = (0..s).filter(|&x| (0..s).any(|y| on(x, y))).collect();
            let rows: Vec<usize> = (0..s).filter(|&y| (0..s).any(|x| on(x, y))).collect();
            let (l, r) = (cols[0], s - 1 - cols[cols.len() - 1]);
            let (t, b) = (rows[0], s - 1 - rows[rows.len() - 1]);
            assert!(l.abs_diff(r) <= 1 && t.abs_diff(b) <= 1, "{s} px: margins {l} {r} {t} {b}");
            assert!(rows.len() >= s - s / 16 - 1, "{s} px: {} rows tall", rows.len());
        }
    }

    #[test]
    fn picks_the_size_that_fits() {
        assert_eq!(size_for(16), 16);
        assert_eq!(size_for(20), 20);
        assert_eq!(size_for(24), 24);
        assert_eq!(size_for(28), 24, "175 %: the 24 px mark, not a scaled one");
        assert_eq!(size_for(36), 32);
        assert_eq!(size_for(48), 48);
        assert_eq!(size_for(64), 48);
        assert_eq!(size_for(12), 16);
    }

    #[test]
    fn padded_when_between_sizes() {
        let at24 = panther(24);
        let at28 = panther(28);
        assert_eq!(at28.len(), 28 * 28);
        for y in 0..24 {
            for x in 0..24 {
                assert_eq!(at28[(y + 2) * 28 + x + 2], at24[y * 24 + x]);
            }
        }
        assert!(at28[..28 * 2].iter().all(|&a| a == 0), "top margin clear");
        assert!(panther(12).iter().all(|&a| a == 0), "too small: nothing rather than a crop");
    }
}
