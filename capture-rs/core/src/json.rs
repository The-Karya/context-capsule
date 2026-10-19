// context-capsule — core: hand-written JSON, so key order matches SPEC.md and bytes match the Swift recorder.

use std::io::Write;
use std::path::Path;

pub enum JsonValue {
    Str(String),
    Int(i64),
    Bool(bool),
    Null,
}

impl JsonValue {
    pub fn opt_str(s: Option<impl Into<String>>) -> JsonValue {
        s.map(|s| JsonValue::Str(s.into())).unwrap_or(JsonValue::Null)
    }
}

/// A JSON string literal with Swift's escaping: \" \\ \n \r \t, U+2028/2029, other controls as \u00XX.
pub fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// `{"k":v,...}` in the given order, no spaces.
pub fn json_object(pairs: &[(&str, JsonValue)]) -> String {
    let body: Vec<String> = pairs
        .iter()
        .map(|(k, v)| {
            let v = match v {
                JsonValue::Str(s) => json_string(s),
                JsonValue::Int(i) => i.to_string(),
                JsonValue::Bool(b) => b.to_string(),
                JsonValue::Null => "null".to_string(),
            };
            json_string(k) + ":" + &v
        })
        .collect();
    format!("{{{}}}", body.join(","))
}

/// Write a whole file at once (temp file + rename), so readers never see half of it.
pub fn write_atomically(path: &Path, text: &str) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir)?;
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    let tmp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(text.as_bytes())?;
        f.flush()?;
    }
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}
