// context-capsule — linux: on-device OCR with the Tesseract CLI. The image is encoded as PNG in memory
// and piped to `tesseract stdin stdout` — never a temp file. No Tesseract: "" and a one-time hint.

use context_capsule_core::Image;
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Longest one OCR may take before we give up on it.
const TIMEOUT: Duration = Duration::from_secs(20);

pub struct Ocr {
    /// The `-l` argument, e.g. "deu+eng"; None until the first call (then None = no tesseract).
    langs: Option<Option<String>>,
    locale: String,
}

pub enum OcrResult {
    Text(String),
    /// Tesseract is not installed (or cannot run).
    Missing,
}

impl Ocr {
    pub fn new(locale: &str) -> Ocr {
        Ocr { langs: None, locale: locale.to_string() }
    }

    /// Lines top to bottom, joined by "\n".
    pub fn read(&mut self, image: &Image) -> OcrResult {
        let locale = self.locale.clone();
        let langs = self.langs.get_or_insert_with(|| installed_langs().map(|have| languages(&locale, &have))).clone();
        let Some(langs) = langs else { return OcrResult::Missing };
        let Some(png) = encode_png(image) else { return OcrResult::Text(String::new()) };
        match run(&png, &langs) {
            Ok(text) => OcrResult::Text(clean(&text)),
            Err(e) => {
                eprintln!("Context Capsule: ocr: {e}");
                OcrResult::Text(String::new())
            }
        }
    }
}

/// `tesseract --list-langs`, or None when tesseract is missing.
fn installed_langs() -> Option<Vec<String>> {
    let out = Command::new("tesseract").arg("--list-langs").stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
    if !out.status.success() {
        return None;
    }
    // The first line is "List of available languages in "/usr/share/…" (N):".
    let text = String::from_utf8_lossy(&out.stdout);
    Some(text.lines().skip(1).map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect())
}

fn run(png: &[u8], langs: &str) -> Result<String, String> {
    let mut child = Command::new("tesseract")
        .args(["stdin", "stdout", "-l", langs, "--psm", "3"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("cannot start tesseract: {e}"))?;
    // Feed stdin from a thread so a full stdout pipe can never deadlock us.
    let mut stdin = child.stdin.take().ok_or("no stdin")?;
    let data = png.to_vec();
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(&data);
    });
    let mut stdout = child.stdout.take().ok_or("no stdout")?;
    let reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stdout.read_to_string(&mut s);
        s
    });
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            break status;
        }
        if start.elapsed() > TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            return Err("tesseract took too long".into());
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let _ = writer.join();
    let text = reader.join().unwrap_or_default();
    if !status.success() {
        return Err(format!("tesseract failed ({status})"));
    }
    Ok(text)
}

/// Non-empty lines, trimmed, top to bottom.
pub fn clean(text: &str) -> String {
    text.lines().map(str::trim).filter(|l| !l.is_empty() && *l != "\u{c}").collect::<Vec<_>>().join("\n")
}

/// The image as an RGB PNG, in memory. Fast compression: it is only piped to tesseract.
pub fn encode_png(image: &Image) -> Option<Vec<u8>> {
    if image.width == 0 || image.height == 0 || image.stride < image.width * 4 {
        return None;
    }
    let mut rgb = Vec::with_capacity(image.width * image.height * 3);
    for y in 0..image.height {
        let row = image.bgra.get(y * image.stride..y * image.stride + image.width * 4)?;
        for px in row.as_chunks::<4>().0 {
            rgb.extend_from_slice(&[px[2], px[1], px[0]]);
        }
    }
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, image.width as u32, image.height as u32);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_compression(png::Compression::Fast);
        let mut w = enc.write_header().ok()?;
        w.write_image_data(&rgb).ok()?;
    }
    Some(out)
}

/// The tessdata name for a locale ("de-DE" → "deu", "zh-TW" → "chi_tra"); None if we know none.
pub fn tessdata_name(locale: &str) -> Option<&'static str> {
    let lower = locale.to_lowercase().replace('_', "-");
    let mut parts = lower.split('-');
    let lang = parts.next().unwrap_or("");
    let rest: Vec<&str> = parts.collect();
    if lang == "zh" {
        let traditional = rest.iter().any(|p| matches!(*p, "tw" | "hk" | "mo" | "hant"));
        return Some(if traditional { "chi_tra" } else { "chi_sim" });
    }
    Some(match lang {
        "en" => "eng",
        "de" => "deu",
        "fr" => "fra",
        "es" => "spa",
        "it" => "ita",
        "pt" => "por",
        "nl" => "nld",
        "sv" => "swe",
        "da" => "dan",
        "nb" | "nn" | "no" => "nor",
        "fi" => "fin",
        "pl" => "pol",
        "cs" => "ces",
        "sk" => "slk",
        "sl" => "slv",
        "hr" => "hrv",
        "hu" => "hun",
        "ro" => "ron",
        "bg" => "bul",
        "ru" => "rus",
        "uk" => "ukr",
        "el" => "ell",
        "tr" => "tur",
        "et" => "est",
        "lv" => "lav",
        "lt" => "lit",
        "ca" => "cat",
        "ja" => "jpn",
        "ko" => "kor",
        "ar" => "ara",
        "he" | "iw" => "heb",
        "fa" => "fas",
        "hi" => "hin",
        "bn" => "ben",
        "ta" => "tam",
        "th" => "tha",
        "vi" => "vie",
        "id" => "ind",
        "ms" => "msa",
        _ => return None,
    })
}

/// The `-l` value: the locale's language plus English when both are installed, else what is.
pub fn languages(locale: &str, installed: &[String]) -> String {
    let has = |l: &str| installed.iter().any(|i| i == l);
    match tessdata_name(locale) {
        Some(l) if l != "eng" && has(l) && has("eng") => format!("{l}+eng"),
        Some(l) if has(l) => l.to_string(),
        _ if has("eng") || installed.is_empty() => "eng".to_string(),
        _ => installed.iter().find(|l| *l != "osd").cloned().unwrap_or_else(|| "eng".into()),
    }
}

/// What to tell the user once when OCR is needed but Tesseract is missing.
pub const INSTALL_HINT: &str = "Context Capsule reads text with Tesseract OCR when an app shows too little text \
    to accessibility. Install it: sudo apt install tesseract-ocr (Debian/Ubuntu) or sudo dnf install tesseract (Fedora).";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locale_to_tessdata() {
        assert_eq!(tessdata_name("en-US"), Some("eng"));
        assert_eq!(tessdata_name("de_DE"), Some("deu"));
        assert_eq!(tessdata_name("pt-BR"), Some("por"));
        assert_eq!(tessdata_name("zh-CN"), Some("chi_sim"));
        assert_eq!(tessdata_name("zh-Hant-TW"), Some("chi_tra"));
        assert_eq!(tessdata_name("zh_HK"), Some("chi_tra"));
        assert_eq!(tessdata_name("nb-NO"), Some("nor"));
        assert_eq!(tessdata_name("xx"), None);
    }

    #[test]
    fn language_choice() {
        let langs = |l: &[&str]| l.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(languages("de-DE", &langs(&["eng", "deu", "osd"])), "deu+eng");
        assert_eq!(languages("de-DE", &langs(&["eng", "osd"])), "eng", "German not installed");
        assert_eq!(languages("en-GB", &langs(&["eng"])), "eng");
        assert_eq!(languages("fr-FR", &langs(&["fra"])), "fra");
        assert_eq!(languages("xx", &langs(&["osd", "spa"])), "spa");
        assert_eq!(languages("en-US", &[]), "eng");
    }

    #[test]
    fn png_in_memory() {
        let image = Image { width: 2, height: 1, stride: 12, bgra: vec![255, 0, 0, 255, 0, 0, 255, 255, 9, 9, 9, 9] };
        let png = encode_png(&image).unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        let decoder = png::Decoder::new(std::io::Cursor::new(png));
        let mut reader = decoder.read_info().unwrap();
        let mut buf = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut buf).unwrap();
        assert_eq!((info.width, info.height), (2, 1));
        assert_eq!(&buf[..6], &[0, 0, 255, 255, 0, 0], "BGRA → RGB, row padding dropped");
        assert!(encode_png(&Image { width: 0, height: 0, stride: 0, bgra: vec![] }).is_none());
    }

    #[test]
    fn cleans_lines() {
        assert_eq!(clean("  Hello \n\n world\n\u{c}\n"), "Hello\nworld");
    }
}
