// context-capsule — core: projects — projects.json, finding a window's project from its clues, and a
// window's page address or file path (`where`).
//! A port of the Projects section of `capture/Sources/Core.swift`. Clues are matched against the window
//! title, its `where` and the app name — never screen text (other projects' names show up there).

use crate::json::write_atomically;
use crate::redact::Redactor;
use std::path::Path;
use unicode_normalization::char::is_combining_mark;
use unicode_normalization::UnicodeNormalization;
use unicode_segmentation::UnicodeSegmentation;

/// A project the user works on.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Project {
    pub id: String,
    pub name: String,
    /// `match` in projects.json: clues found in the window title, its `where`, or the app name.
    pub clues: Vec<String>,
}

pub struct Projects;

impl Projects {
    /// "Client Report (Q3)!" → "client-report-q3"
    pub fn slug(name: &str) -> String {
        // Accents folded away (é → e), then lowercase; only ASCII letters and digits are kept.
        let folded: String = name.nfkd().filter(|c| !is_combining_mark(*c)).collect::<String>().to_lowercase();
        let mut out = String::new();
        let mut dash = false;
        for c in folded.chars() {
            if c.is_ascii_alphanumeric() {
                out.push(c);
                dash = false;
            } else if !dash && !out.is_empty() {
                out.push('-');
                dash = true;
            }
        }
        while out.ends_with('-') {
            out.pop();
        }
        let cut: String = out.chars().take(60).collect();
        cut.trim_end_matches('-').to_string() // same as the MCP: never end on a dash
    }

    /// `{"projects":[{"id","name","match":[…]}]}`; a missing or broken file means no projects.
    pub fn load(path: &Path) -> Vec<Project> {
        let Ok(data) = std::fs::read(path) else { return vec![] };
        Self::parse(&data)
    }

    pub fn parse(data: &[u8]) -> Vec<Project> {
        let Ok(serde_json::Value::Object(obj)) = serde_json::from_slice(data) else { return vec![] };
        let Some(list) = obj.get("projects").and_then(|v| v.as_array()) else { return vec![] };
        // Like Swift's `as? [[String: Any]]`: one entry that is not an object spoils the whole list.
        if !list.iter().all(|p| p.is_object()) {
            return vec![];
        }
        list.iter()
            .filter_map(|p| {
                let name = trim_spaces(p.get("name")?.as_str()?);
                if name.is_empty() {
                    return None;
                }
                let id = match p.get("id").and_then(|v| v.as_str()) {
                    Some(id) if !id.is_empty() => id.to_string(),
                    _ => Self::slug(name),
                };
                // Like `as? [String]`: a list with anything but strings counts as no clues.
                let clues = match p.get("match").and_then(|v| v.as_array()) {
                    Some(m) if m.iter().all(|c| c.is_string()) => m
                        .iter()
                        .filter_map(|c| c.as_str())
                        .map(|c| trim_spaces(c).to_string())
                        .filter(|c| !c.is_empty())
                        .collect(),
                    _ => vec![],
                };
                Some(Project { id, name: name.to_string(), clues })
            })
            .collect()
    }

    /// projects.json as the Mac app writes it (Foundation's pretty printing, keys sorted).
    pub fn json(projects: &[Project]) -> String {
        let s = apple_json_string;
        let mut out = String::from("{\n  \"projects\" : [");
        if projects.is_empty() {
            out.push_str("\n\n  ]\n}");
            return out;
        }
        for (i, p) in projects.iter().enumerate() {
            out.push_str(if i == 0 { "\n" } else { ",\n" });
            out.push_str(&format!("    {{\n      \"id\" : {},\n      \"match\" : [", s(&p.id)));
            if p.clues.is_empty() {
                out.push_str("\n\n      ]");
            } else {
                let clues: Vec<String> = p.clues.iter().map(|c| format!("        {}", s(c))).collect();
                out.push_str(&format!("\n{}\n      ]", clues.join(",\n")));
            }
            out.push_str(&format!(",\n      \"name\" : {}\n    }}", s(&p.name)));
        }
        out.push_str("\n  ]\n}");
        out
    }

    pub fn save(projects: &[Project], path: &Path) -> std::io::Result<()> {
        write_atomically(path, &(Self::json(projects) + "\n"))
    }

    /// The project whose clue appears in the title, location or app name (case-insensitive). On a tie
    /// the longest clue wins, then list order. None: no clue found.
    pub fn detect<'a>(projects: &'a [Project], title: &str, location: &str, app: &str) -> Option<&'a Project> {
        let hay = [title.to_lowercase(), location.to_lowercase(), app.to_lowercase()];
        let mut best: Option<(&Project, usize)> = None;
        for p in projects {
            for clue in &p.clues {
                let c = clue.to_lowercase();
                let len = c.graphemes(true).count();
                if len < 2 || !hay.iter().any(|h| h.contains(&c)) {
                    continue;
                }
                if best.is_none_or(|(_, l)| len > l) {
                    best = Some((p, len));
                }
            }
        }
        best.map(|(p, _)| p)
    }

    /// A clue to remember a window by, when the user makes a project from it: the file or folder name
    /// from its location, else the page's host (and the first two path parts), else the most specific
    /// part of the title.
    pub fn clue(title: &str, location: &str) -> Option<String> {
        if let Some((scheme, rest)) = split_scheme(location) {
            if scheme.eq_ignore_ascii_case("file") {
                let name = file_stem(&percent_decode(path_of(rest)));
                if name.graphemes(true).count() >= 3 {
                    return Some(name);
                }
            } else if let Some(host) = host_of(rest).filter(|h| !h.is_empty()) {
                let after = rest.strip_prefix("//").unwrap_or(rest);
                let path = percent_decode(path_of(&after[after.find('/').unwrap_or(after.len())..]));
                let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).take(2).collect();
                return Some(if parts.is_empty() { host } else { format!("{host}/{}", parts.join("/")) });
            }
        }
        // "main.swift — karya-capsule" / "Pricing - Linear": the longest part (the first, on a tie).
        let parts = title
            .split(['—', '–', '|', '·'])
            .map(|p| p.trim())
            .filter(|p| p.graphemes(true).count() >= 3);
        let mut best: Option<(&str, usize)> = None;
        for p in parts {
            let n = p.graphemes(true).count();
            if best.is_none_or(|(_, l)| n > l) {
                best = Some((p, n));
            }
        }
        best.map(|(p, _)| p.to_string())
    }
}

/// What the recorder does about projects: detect automatically, or stick to the one the user picked.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProjectChoice {
    /// None: auto-detect.
    pub pinned: Option<String>,
}

impl ProjectChoice {
    /// `project_by` for a project matched by a clue, and for one the user picked.
    pub const AUTO: &'static str = "auto";
    pub const YOU: &'static str = "you";

    /// The project id and how it was chosen, for one sample.
    pub fn resolve(&self, projects: &[Project], title: &str, location: &str, app: &str) -> Option<(String, String)> {
        if let Some(p) = &self.pinned {
            return Some((p.clone(), Self::YOU.into()));
        }
        Projects::detect(projects, title, location, app).map(|p| (p.id.clone(), Self::AUTO.into()))
    }
}

/// A window's page address or file path, safe to store: query and fragment dropped (they often carry
/// tokens or private ids), then redacted like any text.
pub struct Location;

impl Location {
    pub const CAP: usize = 500;

    pub fn clean(raw: Option<&str>) -> String {
        let Some(raw) = raw.map(str::trim).filter(|r| !r.is_empty()) else { return String::new() };
        let cut = raw.find(['?', '#']).unwrap_or(raw.len());
        Redactor::clean_cap(&raw[..cut], Self::CAP)
    }
}

/// Swift's `.whitespaces`: spaces and tabs, not line breaks.
pub(crate) fn trim_spaces(s: &str) -> &str {
    s.trim_matches(|c: char| c == '\t' || (c.is_whitespace() && !matches!(c, '\n' | '\r' | '\u{0B}' | '\u{0C}' | '\u{85}' | '\u{2028}' | '\u{2029}')))
}

/// A JSON string as Foundation writes it: `/` escaped, \b and \f short, other controls as \u00xx.
fn apple_json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '/' => out.push_str("\\/"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0C}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// "https://x.com/a" → ("https", "//x.com/a"). A scheme is a letter, then letters, digits, + - or .
fn split_scheme(s: &str) -> Option<(&str, &str)> {
    let colon = s.find(':')?;
    let scheme = &s[..colon];
    let mut chars = scheme.chars();
    let ok = chars.next()?.is_ascii_alphabetic() && chars.all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c));
    ok.then(|| (scheme, &s[colon + 1..]))
}

/// The host of "//user@host:port/path" (like Foundation's URL.host); None without "//".
fn host_of(rest: &str) -> Option<String> {
    let after = rest.strip_prefix("//")?;
    let authority = &after[..after.find(['/', '?', '#']).unwrap_or(after.len())];
    let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let host = if host.starts_with('[') {
        host.split_once(']').map_or(host, |(h, _)| h).trim_start_matches('[')
    } else {
        host.split_once(':').map_or(host, |(h, _)| h)
    };
    Some(host.to_string())
}

/// The path part of a URL remainder: query and fragment cut off (the authority, if any, removed).
fn path_of(rest: &str) -> &str {
    let rest = &rest[..rest.find(['?', '#']).unwrap_or(rest.len())];
    match rest.strip_prefix("//") {
        Some(after) => &after[after.find('/').unwrap_or(after.len())..],
        None => rest,
    }
}

/// The last path component without its extension ("/a/SPEC.md" → "SPEC", "/a/dir/" → "dir").
fn file_stem(path: &str) -> String {
    let name = path.trim_end_matches('/').rsplit('/').next().unwrap_or("");
    match name.rfind('.') {
        Some(i) if i > 0 => name[..i].to_string(),
        _ => name.to_string(),
    }
}

/// %XX escapes decoded (invalid UTF-8 kept as it was).
fn percent_decode(s: &str) -> String {
    let hex = |c: u8| (c as char).to_digit(16).map(|d| d as u8);
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2])) {
                out.push(h * 16 + l);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_parts() {
        assert_eq!(host_of("//u:p@Linear.app:443/x"), Some("Linear.app".into()));
        assert_eq!(host_of("/no/host"), None);
        assert_eq!(path_of("//h/a/b?q#f"), "/a/b");
        assert_eq!(file_stem("/a/dir/"), "dir");
        assert_eq!(file_stem("/a/.bashrc"), ".bashrc");
        assert_eq!(file_stem("/a/x.tar.gz"), "x.tar");
        assert_eq!(percent_decode("my%20file%2"), "my file%2");
        assert_eq!(split_scheme("C:\\Users\\x"), Some(("C", "\\Users\\x")));
        assert_eq!(split_scheme("/plain/path"), None);
    }
}
