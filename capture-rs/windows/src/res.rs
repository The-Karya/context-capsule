// context-capsule — windows: the exe's resources, written by build.rs as a compiled .res the linker takes in
// (no rc.exe or llvm-rc needed): the app icon (icon.ico: Explorer, the Start menu shortcut, Settings → Apps, the
// title bar and taskbar) and the version info (the name Task Manager and Windows show: "Karya Context Capsule").
//! build.rs includes this file with `#[path]` to write the .res; the app uses `ICON_ID` (the window and dialog icons)
//! and runs the tests.

/// Resource types and the ids we use.
const RT_ICON: u16 = 3;
const RT_GROUP_ICON: u16 = 14;
const RT_VERSION: u16 = 16;
/// The icon group's id: what `LoadImageW(instance, 1, IMAGE_ICON, …)` and `"<exe>",0` find.
pub const ICON_ID: u16 = 1;
const LANG_EN_US: u16 = 0x0409;

/// One image of an .ico file: its size (0 = 256), colour depth and bytes (PNG or BMP).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IcoImage {
    pub width: u8,
    pub height: u8,
    pub colors: u8,
    pub planes: u16,
    pub bits: u16,
    pub data: Vec<u8>,
}

/// The images of an .ico file; None when it isn't one.
pub fn parse_ico(ico: &[u8]) -> Option<Vec<IcoImage>> {
    let u16_at = |o: usize| ico.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]]));
    let u32_at = |o: usize| ico.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    if u16_at(0)? != 0 || u16_at(2)? != 1 {
        return None;
    }
    let count = u16_at(4)? as usize;
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let e = 6 + i * 16;
        let (len, off) = (u32_at(e + 8)? as usize, u32_at(e + 12)? as usize);
        out.push(IcoImage {
            width: *ico.get(e)?,
            height: *ico.get(e + 1)?,
            colors: *ico.get(e + 2)?,
            planes: u16_at(e + 4)?,
            bits: u16_at(e + 6)?,
            data: ico.get(off..off.checked_add(len)?)?.to_vec(),
        });
    }
    (!out.is_empty()).then_some(out)
}

/// What the version info says.
pub struct Version<'a> {
    /// "0.2.0"
    pub version: &'a str,
    /// The name people see ("Karya Context Capsule").
    pub name: &'a str,
    pub company: &'a str,
    /// "context-capsule.exe"
    pub file_name: &'a str,
}

/// A compiled .res: the icon images, their group (id `ICON_ID`) and the version info.
pub fn res(images: &[IcoImage], v: &Version) -> Vec<u8> {
    let mut out = Vec::new();
    // A .res starts with an empty entry.
    entry(&mut out, 0, 0, 0, 0, &[]);
    for (i, img) in images.iter().enumerate() {
        entry(&mut out, RT_ICON, i as u16 + 1, 0x1010, LANG_EN_US, &img.data);
    }
    let mut group = Vec::new();
    for v in [0u16, 1, images.len() as u16] {
        group.extend_from_slice(&v.to_le_bytes());
    }
    for (i, img) in images.iter().enumerate() {
        group.extend_from_slice(&[img.width, img.height, img.colors, 0]);
        group.extend_from_slice(&img.planes.to_le_bytes());
        group.extend_from_slice(&img.bits.to_le_bytes());
        group.extend_from_slice(&(img.data.len() as u32).to_le_bytes());
        group.extend_from_slice(&(i as u16 + 1).to_le_bytes());
    }
    entry(&mut out, RT_GROUP_ICON, ICON_ID, 0x1030, LANG_EN_US, &group);
    entry(&mut out, RT_VERSION, 1, 0x0030, LANG_EN_US, &version_info(v));
    out
}

/// One resource: header (type and name as ids), then its data, each padded to 4 bytes.
fn entry(out: &mut Vec<u8>, kind: u16, id: u16, flags: u16, lang: u16, data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&32u32.to_le_bytes());
    for v in [0xFFFF, kind, 0xFFFF, id] {
        out.extend_from_slice(&u16::to_le_bytes(v));
    }
    out.extend_from_slice(&0u32.to_le_bytes()); // data version
    out.extend_from_slice(&flags.to_le_bytes());
    out.extend_from_slice(&lang.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // version
    out.extend_from_slice(&0u32.to_le_bytes()); // characteristics
    out.extend_from_slice(data);
    pad(out);
}

fn pad(b: &mut Vec<u8>) {
    while !b.len().is_multiple_of(4) {
        b.push(0);
    }
}

fn utf16z(s: &str) -> Vec<u8> {
    s.encode_utf16().chain(Some(0)).flat_map(u16::to_le_bytes).collect()
}

/// A version-info block: length, value length, type (1: text), key, value, children; each 4-byte aligned.
fn block(key: &str, value: &[u8], value_len: u16, text: bool, children: &[Vec<u8>]) -> Vec<u8> {
    let mut b = vec![0, 0];
    b.extend_from_slice(&value_len.to_le_bytes());
    b.extend_from_slice(&u16::from(text).to_le_bytes());
    b.extend_from_slice(&utf16z(key));
    pad(&mut b);
    b.extend_from_slice(value);
    for c in children {
        pad(&mut b);
        b.extend_from_slice(c);
    }
    let len = b.len() as u16;
    b[..2].copy_from_slice(&len.to_le_bytes());
    b
}

/// "0.2.0" → (0 << 16 | 2, 0 << 16): the two version words.
fn version_words(v: &str) -> (u32, u32) {
    let n: Vec<u32> = v.split(['.', '-', '+']).map(|p| p.parse().unwrap_or(0)).chain([0, 0, 0, 0]).take(4).collect();
    ((n[0] << 16) | (n[1] & 0xFFFF), (n[2] << 16) | (n[3] & 0xFFFF))
}

fn version_info(v: &Version) -> Vec<u8> {
    let (ms, ls) = version_words(v.version);
    let mut fixed = Vec::with_capacity(52);
    for d in [0xFEEF_04BDu32, 0x0001_0000, ms, ls, ms, ls, 0x3F, 0, 0x0004_0004, 1, 0, 0, 0] {
        fixed.extend_from_slice(&d.to_le_bytes());
    }
    let s = |k: &str, val: &str| block(k, &utf16z(val), val.encode_utf16().count() as u16 + 1, true, &[]);
    let strings = [
        s("CompanyName", v.company),
        s("FileDescription", v.name),
        s("FileVersion", v.version),
        s("InternalName", v.file_name.trim_end_matches(".exe")),
        s("OriginalFilename", v.file_name),
        s("ProductName", v.name),
        s("ProductVersion", v.version),
    ];
    let table = block("040904B0", &[], 0, true, &strings);
    let string_info = block("StringFileInfo", &[], 0, true, &[table]);
    let translation = block("Translation", &0x04B0_0409u32.to_le_bytes(), 4, false, &[]);
    let var_info = block("VarFileInfo", &[], 0, true, &[translation]);
    block("VS_VERSION_INFO", &fixed, 52, false, &[string_info, var_info])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u16_at(b: &[u8], o: usize) -> u16 {
        u16::from_le_bytes([b[o], b[o + 1]])
    }
    fn u32_at(b: &[u8], o: usize) -> u32 {
        u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
    }

    /// The entries of a .res: (type, id, data).
    fn entries(res: &[u8]) -> Vec<(u16, u16, Vec<u8>)> {
        let mut out = vec![];
        let mut o = 0;
        while o < res.len() {
            let (size, header) = (u32_at(res, o) as usize, u32_at(res, o + 4) as usize);
            assert_eq!(header, 32);
            assert_eq!((u16_at(res, o + 8), u16_at(res, o + 12)), (0xFFFF, 0xFFFF), "ids, not names");
            out.push((u16_at(res, o + 10), u16_at(res, o + 14), res[o + header..o + header + size].to_vec()));
            o += header + size.div_ceil(4) * 4;
        }
        assert_eq!(o, res.len());
        out
    }

    fn has_text(b: &[u8], s: &str) -> bool {
        let w: Vec<u8> = s.encode_utf16().flat_map(u16::to_le_bytes).collect();
        b.windows(w.len()).any(|x| x == w.as_slice())
    }

    #[test]
    fn the_panther_icon() {
        let images = parse_ico(include_bytes!("../icon.ico")).expect("icon.ico");
        let sizes: Vec<u32> = images.iter().map(|i| if i.width == 0 { 256 } else { i.width as u32 }).collect();
        assert_eq!(sizes, [16, 20, 24, 32, 40, 48, 64, 256]);
        assert!(images.iter().all(|i| i.width == i.height && i.bits == 32 && i.data.starts_with(b"\x89PNG")));
        assert!(parse_ico(b"not an icon").is_none());
    }

    #[test]
    fn compiled_resources() {
        let images = parse_ico(include_bytes!("../icon.ico")).unwrap();
        let v = Version { version: "0.2.0", name: "Karya Context Capsule", company: "Karya", file_name: "context-capsule.exe" };
        let res = res(&images, &v);
        let e = entries(&res);
        assert_eq!((e[0].0, e[0].1, e[0].2.len()), (0, 0, 0), "starts with the empty entry");
        let icons: Vec<_> = e.iter().filter(|x| x.0 == RT_ICON).collect();
        assert_eq!(icons.len(), images.len());
        assert_eq!(icons[7].2, images[7].data);
        let group = &e.iter().find(|x| x.0 == RT_GROUP_ICON).unwrap();
        assert_eq!(group.1, ICON_ID);
        assert_eq!((u16_at(&group.2, 2), u16_at(&group.2, 4)), (1, 8));
        assert_eq!(group.2.len(), 6 + 14 * 8);
        // The 256 px entry: width 0, its size and id 8.
        let last = 6 + 14 * 7;
        assert_eq!(group.2[last], 0);
        assert_eq!(u32_at(&group.2, last + 8) as usize, images[7].data.len());
        assert_eq!(u16_at(&group.2, last + 12), 8);
        let version = &e.iter().find(|x| x.0 == RT_VERSION).unwrap().2;
        assert_eq!(u16_at(version, 0) as usize, version.len(), "the outer block's length");
        assert_eq!(u16_at(version, 2), 52);
        assert!(has_text(version, "VS_VERSION_INFO") && has_text(version, "FileDescription") && has_text(version, "Karya Context Capsule"));
        assert!(has_text(version, "context-capsule.exe") && has_text(version, "040904B0") && has_text(version, "Translation"));
        let sig = version.windows(4).position(|w| w == 0xFEEF_04BDu32.to_le_bytes()).unwrap();
        assert_eq!(sig % 4, 0, "the fixed info is aligned");
        assert_eq!(u32_at(version, sig + 8), 2, "file version 0.2 (major << 16 | minor)");
    }

    #[test]
    fn version_numbers() {
        assert_eq!(version_words("0.2.0"), (2, 0));
        assert_eq!(version_words("1.12.3"), ((1 << 16) | 12, 3 << 16));
        assert_eq!(version_words("2.0.0-beta.1"), (2 << 16, 0));
    }
}
