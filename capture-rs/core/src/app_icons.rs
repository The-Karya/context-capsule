// context-capsule — core: app icons for the shared window (bridge `apps.icons`): which apps the page asks for, a
// cache per app for the program's life (a day with many stretches costs one lookup per app), the answer as data
// URLs, and a small PNG writer for hosts that only have pixels (Windows).

use base64::Engine as _;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// Icons are drawn at 32 × 32 points, so the host renders this many pixels (@2×).
pub const SIZE: u32 = 64;
/// At most this many apps in one call.
pub const MAX_APPS: usize = 200;

/// One app the page asks about: its name and id as events store them (`app`, `bundle`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppRef {
    pub app: String,
    pub bundle: String,
}

impl AppRef {
    /// The answer's key: the bundle id (Mac) / exe (Windows) / app id (Linux), else the app's name.
    pub fn key(&self) -> &str {
        if self.bundle.is_empty() {
            &self.app
        } else {
            &self.bundle
        }
    }
}

/// `{apps:[{app,bundle}]}` → the apps, without empty or repeated keys, at most [`MAX_APPS`].
pub fn parse(args: &Value) -> Vec<AppRef> {
    let s = |o: &Value, k: &str| o.get(k).and_then(Value::as_str).unwrap_or("").trim().to_string();
    let mut out: Vec<AppRef> = Vec::new();
    for o in args.get("apps").and_then(Value::as_array).into_iter().flatten() {
        let r = AppRef { app: s(o, "app"), bundle: s(o, "bundle") };
        if r.key().is_empty() || r.key().len() > 512 || out.iter().any(|x| x.key() == r.key()) {
            continue;
        }
        out.push(r);
        if out.len() == MAX_APPS {
            break;
        }
    }
    out
}

/// PNG bytes → `data:image/png;base64,…`.
pub fn data_url(png: &[u8]) -> String {
    format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png))
}

/// Answers per key; each key looked up once (found or not) and kept.
#[derive(Default)]
pub struct Cache {
    known: Mutex<HashMap<String, Option<String>>>,
}

impl Cache {
    /// `{ key: data URL | null }` for every app; `lookup(app, bundle)` gives PNG bytes, only for keys not seen yet.
    pub fn answer(&self, apps: &[AppRef], lookup: &dyn Fn(&str, &str) -> Option<Vec<u8>>) -> Map<String, Value> {
        let mut out = Map::new();
        for a in apps {
            let key = a.key().to_string();
            let hit = self.known.lock().ok().and_then(|k| k.get(&key).cloned());
            let url = match hit {
                Some(u) => u,
                None => {
                    let u = lookup(&a.app, &a.bundle).filter(|png| !png.is_empty()).map(|png| data_url(&png));
                    if let Ok(mut k) = self.known.lock() {
                        k.insert(key.clone(), u.clone());
                    }
                    u
                }
            };
            out.insert(key, url.map_or(Value::Null, Value::String));
        }
        out
    }
}

/// The program's cache (the bridge uses this one).
pub fn shared() -> &'static Cache {
    static CACHE: OnceLock<Cache> = OnceLock::new();
    CACHE.get_or_init(Cache::default)
}

// MARK: a PNG from pixels

/// An RGBA image (`width * height * 4` bytes, straight alpha) as a PNG (stored, not compressed: an icon is ~16 KB).
/// None if the size doesn't match.
pub fn png_rgba(width: u32, height: u32, rgba: &[u8]) -> Option<Vec<u8>> {
    let row = width as usize * 4;
    if width == 0 || height == 0 || rgba.len() != row * height as usize {
        return None;
    }
    // Each row: filter 0, then its pixels.
    let mut raw = Vec::with_capacity((row + 1) * height as usize);
    for r in rgba.chunks(row) {
        raw.push(0);
        raw.extend_from_slice(r);
    }
    // zlib: header, stored blocks of at most 65535 bytes, Adler-32.
    let mut z = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = raw.chunks(65535).collect();
    for (i, b) in blocks.iter().enumerate() {
        z.push(u8::from(i + 1 == blocks.len()));
        let len = b.len() as u16;
        z.extend_from_slice(&len.to_le_bytes());
        z.extend_from_slice(&(!len).to_le_bytes());
        z.extend_from_slice(b);
    }
    z.extend_from_slice(&adler32(&raw).to_be_bytes());

    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // 8 bits, RGBA, deflate, no filter set, no interlace
    chunk(&mut png, b"IHDR", &ihdr);
    chunk(&mut png, b"IDAT", &z);
    chunk(&mut png, b"IEND", &[]);
    Some(png)
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let crc = crc32(&out[start..]);
    out.extend_from_slice(&crc.to_be_bytes());
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut c = 0xffff_ffffu32;
    for &b in bytes {
        c ^= b as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
        }
    }
    !c
}

fn adler32(bytes: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &x in bytes {
        a = (a + x as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::cell::RefCell;

    #[test]
    fn keys_and_parsing() {
        let apps = parse(&json!({ "apps": [
            { "app": "Safari", "bundle": "com.apple.Safari" },
            { "app": "Safari again", "bundle": "com.apple.Safari" },
            { "app": "Notes", "bundle": "" },
            { "app": "", "bundle": "" },
            { "app": 4 },
            "nonsense",
        ]}));
        assert_eq!(apps.iter().map(AppRef::key).collect::<Vec<_>>(), ["com.apple.Safari", "Notes"]);
        assert!(parse(&json!({})).is_empty());
        let many: Vec<Value> = (0..500).map(|i| json!({ "app": format!("a{i}") })).collect();
        assert_eq!(parse(&json!({ "apps": many })).len(), MAX_APPS);
    }

    #[test]
    fn looks_up_each_app_once() {
        let cache = Cache::default();
        let asked = RefCell::new(Vec::new());
        let lookup = |app: &str, bundle: &str| {
            asked.borrow_mut().push(format!("{app}|{bundle}"));
            (bundle == "chrome.exe").then(|| vec![1, 2, 3])
        };
        let apps = parse(&json!({ "apps": [{ "app": "Chrome", "bundle": "chrome.exe" }, { "app": "Gone" }] }));
        let first = cache.answer(&apps, &lookup);
        assert_eq!(first["chrome.exe"], json!("data:image/png;base64,AQID"));
        assert_eq!(first["Gone"], Value::Null);
        let again = cache.answer(&apps, &lookup);
        assert_eq!(again, first);
        assert_eq!(*asked.borrow(), ["Chrome|chrome.exe", "Gone|"], "one lookup per app, misses kept too");
    }

    #[test]
    fn png_is_well_formed() {
        assert_eq!(png_rgba(2, 2, &[0; 15]), None);
        assert_eq!(png_rgba(0, 0, &[]), None);
        let px: Vec<u8> = (0..64 * 64).flat_map(|i| [i as u8, 0, 255, 128]).collect();
        let png = png_rgba(64, 64, &px).unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(&png[12..16], b"IHDR");
        assert_eq!(u32::from_be_bytes(png[16..20].try_into().unwrap()), 64);
        assert_eq!(&png[png.len() - 8..png.len() - 4], b"IEND");
        assert_eq!(crc32(b"IEND"), 0xae42_6082, "the IEND chunk's well-known CRC");
        assert_eq!(adler32(b"Wikipedia"), 0x11e6_0398);
        // A row bigger than one stored block still fits.
        assert!(png_rgba(20000, 1, &vec![7; 80000]).is_some());
    }
}
