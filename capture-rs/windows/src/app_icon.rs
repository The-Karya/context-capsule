// context-capsule — windows: an app's icon for the shared window (bridge `apps.icons`): find the exe (a running
// process, else App Paths in the registry, else the Windows folder), ask the shell for its icon at 64 px, and
// hand back a PNG. The core caches answers, so each app is looked up once.
//! Pure pieces (which exe names are allowed, App Paths values, turning the shell's pixels into a 64 × 64 straight
//! RGBA image) build and test on any OS; the Windows calls are behind `cfg(windows)`.

use context_capsule_core::app_icons::SIZE;

/// The exe to look for: `bundle` when it is a plain exe file name ("chrome.exe"); with no bundle, the app's name
/// plus ".exe" (only tried in App Paths). The bool says whether it came from `bundle`.
pub fn exe_for(app: &str, bundle: &str) -> Option<(String, bool)> {
    let bundle = bundle.trim();
    if !bundle.is_empty() {
        return is_exe_name(bundle).then(|| (bundle.to_string(), true));
    }
    let guess = format!("{}.exe", app.trim());
    (!app.trim().is_empty() && is_exe_name(&guess)).then_some((guess, false))
}

/// A plain file name ending in ".exe": no folders, drive letters or odd characters.
pub fn is_exe_name(name: &str) -> bool {
    let n = name.len();
    n > 4
        && n <= 260
        && name[n - 4..].eq_ignore_ascii_case(".exe")
        && !name.starts_with('.')
        && !name.contains("..")
        && !name.chars().any(|c| c.is_control() || matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
}

/// Whether a full path's file name is `exe` (any case).
pub fn same_exe(path: &str, exe: &str) -> bool {
    crate::windows_list::exe_file_name(path).eq_ignore_ascii_case(exe)
}

/// An App Paths default value → a path: quotes and spaces around it dropped. None if it isn't a full path to `exe`.
pub fn app_path_value(raw: &str, exe: &str) -> Option<String> {
    let p = raw.trim().trim_matches('"').trim();
    let full = p.len() > 3 && (p.as_bytes()[1] == b':' || p.starts_with(r"\\"));
    (full && same_exe(p, exe)).then(|| p.to_string())
}

/// The shell's pixels (`w * h` BGRA, top-down) → a [`SIZE`] × [`SIZE`] straight RGBA image, scaled to fit and
/// centred. `premultiplied`: the colours are already multiplied by alpha (shell images); else straight (icons).
/// `mask`: per pixel, true = transparent (an icon's AND mask), used when the pixels carry no alpha at all.
pub fn icon_rgba(bgra: &[u8], w: u32, h: u32, premultiplied: bool, mask: Option<&[bool]>) -> Option<Vec<u8>> {
    let count = w as usize * h as usize;
    if w == 0 || h == 0 || bgra.len() != count * 4 || mask.is_some_and(|m| m.len() != count) {
        return None;
    }
    let no_alpha = bgra.as_chunks::<4>().0.iter().all(|p| p[3] == 0);
    // Premultiplied RGBA, so scaling doesn't bleed the colour of transparent pixels.
    let mut px = Vec::with_capacity(bgra.len());
    for (i, p) in bgra.as_chunks::<4>().0.iter().enumerate() {
        let a = if no_alpha {
            if mask.is_some_and(|m| m[i]) {
                0
            } else {
                255
            }
        } else {
            p[3]
        };
        let (r, g, b) = if premultiplied && !no_alpha {
            (p[2].min(a), p[1].min(a), p[0].min(a))
        } else {
            (mul(p[2], a), mul(p[1], a), mul(p[0], a))
        };
        px.extend_from_slice(&[r, g, b, a]);
    }
    let mut out = fit(&px, w as usize, h as usize, SIZE as usize);
    unpremultiply(&mut out);
    Some(out)
}

fn mul(c: u8, a: u8) -> u8 {
    ((c as u32 * a as u32 + 127) / 255) as u8
}

/// Premultiplied RGBA → straight RGBA, in place.
pub fn unpremultiply(rgba: &mut [u8]) {
    for p in rgba.as_chunks_mut::<4>().0.iter_mut() {
        let a = p[3] as u32;
        if a == 0 {
            p[..3].fill(0);
        } else if a < 255 {
            for c in &mut p[..3] {
                *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
}

/// A premultiplied RGBA image scaled (area average) to fit `size` × `size`, keeping its shape, centred on clear.
fn fit(px: &[u8], w: usize, h: usize, size: usize) -> Vec<u8> {
    let scale = (size as f64 / w as f64).min(size as f64 / h as f64);
    let (dw, dh) = (((w as f64 * scale).round() as usize).clamp(1, size), ((h as f64 * scale).round() as usize).clamp(1, size));
    let (ox, oy) = ((size - dw) / 2, (size - dh) / 2);
    let (sx, sy) = (w as f64 / dw as f64, h as f64 / dh as f64);
    let mut out = vec![0u8; size * size * 4];
    for y in 0..dh {
        let (y0, y1) = (y as f64 * sy, (y + 1) as f64 * sy);
        for x in 0..dw {
            let (x0, x1) = (x as f64 * sx, (x + 1) as f64 * sx);
            let mut sum = [0f64; 4];
            let mut area = 0f64;
            for yy in y0.floor() as usize..(y1.ceil() as usize).min(h) {
                let wy = (y1.min(yy as f64 + 1.0) - y0.max(yy as f64)).max(0.0);
                for xx in x0.floor() as usize..(x1.ceil() as usize).min(w) {
                    let wx = (x1.min(xx as f64 + 1.0) - x0.max(xx as f64)).max(0.0);
                    let k = wx * wy;
                    let i = (yy * w + xx) * 4;
                    for c in 0..4 {
                        sum[c] += px[i + c] as f64 * k;
                    }
                    area += k;
                }
            }
            let o = ((oy + y) * size + ox + x) * 4;
            for c in 0..4 {
                out[o + c] = if area > 0.0 { (sum[c] / area).round().clamp(0.0, 255.0) as u8 } else { 0 };
            }
        }
    }
    out
}

#[cfg(windows)]
pub use win::icon;

#[cfg(windows)]
mod win {
    use super::*;
    use context_capsule_core::app_icons::png_rgba;
    use std::collections::HashSet;
    use windows::core::{BOOL, HSTRING};
    use windows::Win32::Foundation::{HWND, LPARAM, SIZE as WinSize};
    use windows::Win32::Graphics::Gdi::{
        DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
        DIB_RGB_COLORS, HBITMAP,
    };
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
    use windows::Win32::UI::Shell::{
        IShellItemImageFactory, SHCreateItemFromParsingName, SHDefExtractIconW, SIIGBF_ICONONLY,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        DestroyIcon, EnumWindows, GetIconInfo, GetWindowThreadProcessId, HICON, ICONINFO,
    };

    /// The app's icon as a 64 × 64 PNG, or None. Runs on a bridge worker thread.
    pub fn icon(app: &str, bundle: &str) -> Option<Vec<u8>> {
        let (exe, from_bundle) = exe_for(app, bundle)?;
        let path = if from_bundle {
            running(&exe).or_else(|| app_paths(&exe)).or_else(|| windows_folder(&exe))
        } else {
            app_paths(&exe)
        }?;
        // COM for this thread; dropped last, after every shell object below.
        let _com = Com::init();
        let rgba = shell_image(&path).or_else(|| extracted(&path))?;
        png_rgba(SIZE, SIZE, &rgba)
    }

    /// COM on this thread (single-threaded, as the shell likes); undone on drop if we did it.
    struct Com(bool);

    impl Com {
        fn init() -> Com {
            // SAFETY: plain per-thread init; RPC_E_CHANGED_MODE (COM already set up another way) is fine to use.
            Com(unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok())
        }
    }

    impl Drop for Com {
        fn drop(&mut self) {
            if self.0 {
                // SAFETY: pairs with the successful CoInitializeEx above, on the same thread.
                unsafe { CoUninitialize() };
            }
        }
    }

    /// The path of a running process (one with a top-level window, shown or not) whose exe is `exe`.
    fn running(exe: &str) -> Option<String> {
        let mut handles: Vec<HWND> = Vec::new();
        // SAFETY: the callback only pushes into `handles`, which outlives the call.
        unsafe {
            let _ = EnumWindows(Some(collect), LPARAM(&mut handles as *mut Vec<HWND> as isize));
        }
        let mut seen = HashSet::new();
        for h in handles {
            let mut pid = 0u32;
            // SAFETY: a read-only query; a stale handle just gives 0.
            unsafe { GetWindowThreadProcessId(h, Some(&mut pid)) };
            if pid == 0 || !seen.insert(pid) {
                continue;
            }
            if let Some(path) = crate::windows_list::exe_path(pid).filter(|p| same_exe(p, exe)) {
                return Some(path);
            }
        }
        None
    }

    unsafe extern "system" fn collect(h: HWND, lparam: LPARAM) -> BOOL {
        // SAFETY: `lparam` is the `&mut Vec<HWND>` passed by `running`.
        let list = unsafe { &mut *(lparam.0 as *mut Vec<HWND>) };
        list.push(h);
        BOOL(1)
    }

    /// Where App Paths says the exe is (this user's first, then the machine's), if the file is there.
    fn app_paths(exe: &str) -> Option<String> {
        let key = format!(r"SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\{exe}");
        [false, true].into_iter().find_map(|machine| {
            let raw = crate::win::reg_string_at(machine, &key, "")?;
            app_path_value(&raw, exe).filter(|p| std::path::Path::new(p).is_file())
        })
    }

    /// Windows' own programs (explorer.exe, notepad.exe, cmd.exe).
    fn windows_folder(exe: &str) -> Option<String> {
        let root = std::path::PathBuf::from(std::env::var_os("SystemRoot")?);
        [root.join(exe), root.join("System32").join(exe)]
            .into_iter()
            .find(|p| p.is_file())
            .map(|p| p.to_string_lossy().into_owned())
    }

    /// The shell's icon for the file at 64 px (what Explorer shows), premultiplied.
    fn shell_image(path: &str) -> Option<Vec<u8>> {
        // SAFETY: COM is set up on this thread; the bitmap we get is ours and deleted below.
        unsafe {
            let item: IShellItemImageFactory = SHCreateItemFromParsingName(&HSTRING::from(path), None).ok()?;
            let bmp = item.GetImage(WinSize { cx: SIZE as i32, cy: SIZE as i32 }, SIIGBF_ICONONLY).ok()?;
            let px = bitmap_bgra(bmp);
            let _ = DeleteObject(bmp.into());
            let (bgra, w, h) = px?;
            icon_rgba(&bgra, w, h, true, None)
        }
    }

    /// The exe's first icon resource at 64 px, straight alpha (or its mask when it has none).
    fn extracted(path: &str) -> Option<Vec<u8>> {
        let mut icon = HICON::default();
        // SAFETY: one icon handle out, destroyed below; the ICONINFO bitmaps are ours and deleted below.
        unsafe {
            if SHDefExtractIconW(&HSTRING::from(path), 0, 0, Some(&mut icon), None, SIZE).is_err() || icon.is_invalid() {
                return None;
            }
            let mut info = ICONINFO::default();
            let ok = GetIconInfo(icon, &mut info).is_ok();
            let _ = DestroyIcon(icon);
            if !ok {
                return None;
            }
            let color = (!info.hbmColor.is_invalid()).then(|| bitmap_bgra(info.hbmColor)).flatten();
            let mask = (!info.hbmMask.is_invalid()).then(|| bitmap_bgra(info.hbmMask)).flatten();
            let _ = DeleteObject(info.hbmColor.into());
            let _ = DeleteObject(info.hbmMask.into());
            let (bgra, w, h) = color?;
            // The mask as 32 bpp: white = transparent. Only used when it is the colour bitmap's size.
            let mask: Option<Vec<bool>> =
                mask.filter(|(_, mw, mh)| (*mw, *mh) == (w, h)).map(|(m, _, _)| m.as_chunks::<4>().0.iter().map(|p| p[0] != 0).collect());
            icon_rgba(&bgra, w, h, false, mask.as_deref())
        }
    }

    /// A bitmap's pixels as 32 bpp BGRA, top-down, with its size. At most 1024 px a side.
    fn bitmap_bgra(bmp: HBITMAP) -> Option<(Vec<u8>, u32, u32)> {
        let mut info = BITMAP::default();
        // SAFETY: `info` is one BITMAP and its size is passed.
        let got = unsafe { GetObjectW(bmp.into(), std::mem::size_of::<BITMAP>() as i32, Some(&mut info as *mut BITMAP as *mut _)) };
        let (w, h) = (info.bmWidth, info.bmHeight.abs());
        if got == 0 || !(1..=1024).contains(&w) || !(1..=1024).contains(&h) {
            return None;
        }
        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h, // top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut buf = vec![0u8; w as usize * h as usize * 4];
        // SAFETY: `buf` holds `h` rows of `w` 32-bit pixels, as `bmi` asks for; the screen DC is released.
        let lines = unsafe {
            let dc = GetDC(None);
            let n = GetDIBits(dc, bmp, 0, h as u32, Some(buf.as_mut_ptr().cast()), &mut bmi, DIB_RGB_COLORS);
            ReleaseDC(None, dc);
            n
        };
        (lines == h).then_some((buf, w as u32, h as u32))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exe_names() {
        for ok in ["chrome.exe", "Code.EXE", "WINWORD.exe", "my app.exe"] {
            assert!(is_exe_name(ok), "{ok}");
        }
        for bad in ["", ".exe", "chrome", "chrome.exe.lnk", r"C:\x\chrome.exe", "a/b.exe", "c:chrome.exe", "..\\x.exe", "a..exe", "a\0.exe"] {
            assert!(!is_exe_name(bad), "{bad:?}");
        }
        assert_eq!(exe_for("Google Chrome", "chrome.exe"), Some(("chrome.exe".into(), true)));
        assert_eq!(exe_for("Chrome", r"C:\evil\chrome.exe"), None, "a bad bundle isn't replaced by the name");
        assert_eq!(exe_for("Notepad", ""), Some(("Notepad.exe".into(), false)));
        assert_eq!(exe_for("", ""), None);
        assert_eq!(exe_for("a/b", ""), None);
    }

    #[test]
    fn app_path_values() {
        let p = r"C:\Program Files\Google\Chrome\Application\chrome.exe";
        assert_eq!(app_path_value(&format!("\"{p}\""), "CHROME.EXE").as_deref(), Some(p));
        assert_eq!(app_path_value(&format!("  {p} "), "chrome.exe").as_deref(), Some(p));
        assert_eq!(app_path_value(p, "firefox.exe"), None, "a different exe");
        assert_eq!(app_path_value("chrome.exe", "chrome.exe"), None, "not a full path");
        assert!(app_path_value(r"\\server\share\chrome.exe", "chrome.exe").is_some());
        assert!(same_exe(r"C:\Windows\explorer.exe", "Explorer.EXE"));
    }

    fn one(bgra: [u8; 4], premultiplied: bool) -> [u8; 4] {
        let px: Vec<u8> = bgra.iter().copied().cycle().take(64 * 64 * 4).collect();
        let out = icon_rgba(&px, 64, 64, premultiplied, None).unwrap();
        assert_eq!(out.len(), 64 * 64 * 4);
        out[..4].try_into().unwrap()
    }

    #[test]
    fn premultiplied_to_straight() {
        assert_eq!(one([0, 0, 128, 128], true), [255, 0, 0, 128], "half-clear red");
        assert_eq!(one([10, 20, 30, 255], true), [30, 20, 10, 255], "opaque: only BGR → RGB");
        assert_eq!(one([0, 0, 255, 255], false), [255, 0, 0, 255]);
        assert_eq!(one([0, 0, 255, 128], false), [255, 0, 0, 128], "straight stays straight");
        assert_eq!(one([9, 9, 9, 0], false), [9, 9, 9, 255], "no alpha anywhere: opaque");
    }

    #[test]
    fn mask_when_no_alpha() {
        let px = [1u8, 2, 3, 0, 4, 5, 6, 0, 7, 8, 9, 0, 10, 11, 12, 0];
        let out = icon_rgba(&px, 2, 2, false, Some(&[true, false, false, true])).unwrap();
        // 2 × 2 → 64 × 64: each source pixel covers a 32 × 32 quarter.
        assert_eq!(&out[..4], &[0, 0, 0, 0], "masked: clear");
        assert_eq!(&out[32 * 4..32 * 4 + 4], &[6, 5, 4, 255]);
        assert!(icon_rgba(&px, 2, 2, false, Some(&[true])).is_none(), "mask of the wrong size");
        assert!(icon_rgba(&px, 3, 2, false, None).is_none(), "pixels of the wrong size");
    }

    #[test]
    fn fits_and_centres() {
        // 128 × 64 opaque white → 64 × 32, centred: rows 16..48 white, the rest clear.
        let px = vec![255u8; 128 * 64 * 4];
        let out = icon_rgba(&px, 128, 64, true, None).unwrap();
        let at = |x: usize, y: usize| &out[(y * 64 + x) * 4..(y * 64 + x) * 4 + 4];
        assert_eq!(at(0, 0), [0, 0, 0, 0]);
        assert_eq!(at(0, 15), [0, 0, 0, 0]);
        assert_eq!(at(0, 16), [255, 255, 255, 255]);
        assert_eq!(at(63, 47), [255, 255, 255, 255]);
        assert_eq!(at(63, 48), [0, 0, 0, 0]);
        // 256 × 256: half clear, half opaque red, averaged down without darkening the red.
        let mut big = vec![0u8; 256 * 256 * 4];
        for (i, p) in big.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            if i % 2 == 0 {
                p.copy_from_slice(&[0, 0, 255, 255]);
            }
        }
        let out = icon_rgba(&big, 256, 256, true, None).unwrap();
        assert_eq!(&out[..4], &[255, 0, 0, 128]);
        assert!(context_capsule_core::app_icons::png_rgba(SIZE, SIZE, &out).is_some());
    }
}
