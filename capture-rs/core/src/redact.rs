// context-capsule — core: redaction — keys, tokens and password lines become "[redacted]"; a 100,000-character
// safety cap (only new lines are saved, so this is not a working limit).
//! The patterns are Swift's (NSRegularExpression). Rust's `regex` has no lookbehind or lookahead,
//! so `(?<![a-z])` and `(?<![A-Za-z0-9])…(?![A-Za-z0-9])` are emulated: the preceding character
//! (or the start) is captured and kept, and only the rest is replaced.

use regex::{Captures, Regex};
use std::sync::OnceLock;
use unicode_segmentation::UnicodeSegmentation;

pub struct Redactor;

struct Patterns {
    password_line: Regex,
    tokens: Vec<Regex>,
    hex: Regex,
    base64ish: Regex,
}

/// ICU's line terminators: what `.` never matches and where `^` (multiline) starts a line.
const NL: &str = r"\n\x0B\x0C\r\x{85}\x{2028}\x{2029}";

fn patterns() -> &'static Patterns {
    static P: OnceLock<Patterns> = OnceLock::new();
    P.get_or_init(|| {
        // Patterns are constants; a typo should fail loudly in the tests.
        let re = |p: &str| Regex::new(p).expect("redaction pattern");
        Patterns {
            // Swift: (?im)^.*(?<![a-z])(password|passwd|passphrase|pwd)["']?\s*[:=].*$
            // Group 1 is the line start (start of text or the terminator before it), kept as is.
            password_line: re(&format!(
                r#"(?i)(^|[{NL}])((?:[^{NL}]*[^a-z{NL}])?(?:password|passwd|passphrase|pwd)["']?\s*[:=][^{NL}]*)"#
            )),
            tokens: vec![
                re(r"sk-[A-Za-z0-9_\-]{16,}"),                  // OpenAI / Anthropic style
                re(r"gh[pousr]_[A-Za-z0-9]{20,}"),              // GitHub tokens
                re(r"github_pat_[A-Za-z0-9_]{20,}"),
                re(r"xox[bpoasr]-[A-Za-z0-9\-]{10,}"),          // Slack
                re(r"(?:AKIA|ASIA)[0-9A-Z]{16}"),               // AWS access key id
                re(r"(?i)bearer\s+[A-Za-z0-9._\-+/=]{16,}"),    // Authorization headers
            ],
            // Long hex; only when no letter or digit touches it (checked by hand, see `redact`).
            hex: re(r"[0-9a-fA-F]{33,}"),
            // Long base64-ish runs; only redacted when they look random (see `looks_random`).
            base64ish: re(r"[A-Za-z0-9+/_\-]{33,}={0,2}"),
        }
    })
}

impl Redactor {
    pub const MARK: &'static str = "[redacted]";
    /// A safety net only (a whole book read through accessibility), not a working limit: only new lines are saved.
    pub const TEXT_CAP: usize = 100_000;

    /// Mixed case + digits and a long unbroken alphanumeric run: a key, not a path or a word.
    pub fn looks_random(s: &str) -> bool {
        let has_digit = s.chars().any(|c| c.is_ascii_digit());
        let has_upper = s.chars().any(|c| c.is_ascii_uppercase());
        let has_lower = s.chars().any(|c| c.is_ascii_lowercase());
        let longest_run = s.split(|c: char| !c.is_ascii_alphanumeric()).map(|r| r.len()).max().unwrap_or(0);
        has_digit && has_upper && has_lower && longest_run >= 16
    }

    pub fn redact(text: &str) -> String {
        let p = patterns();
        let mut s = p
            .password_line
            .replace_all(text, |c: &Captures| format!("{}{}", &c[1], Self::MARK))
            .into_owned();
        for r in &p.tokens {
            s = r.replace_all(&s, Self::MARK).into_owned();
        }
        s = replace_hex(&p.hex, &s);
        p.base64ish
            .replace_all(&s, |c: &Captures| {
                if Self::looks_random(&c[0]) { Self::MARK.to_string() } else { c[0].to_string() }
            })
            .into_owned()
    }

    /// Redact, then keep the first `TEXT_CAP` characters (grapheme clusters, like Swift's `count`).
    pub fn clean(text: &str) -> String {
        Self::clean_cap(text, Self::TEXT_CAP)
    }

    pub fn clean_cap(text: &str, cap: usize) -> String {
        prefix_chars(&Self::redact(text), cap)
    }
}

/// `(?<![A-Za-z0-9])[0-9a-fA-F]{33,}(?![A-Za-z0-9])`: each match of the plain pattern is a whole
/// hex run, so it is a hit exactly when the characters around it are not ASCII letters or digits.
fn replace_hex(re: &Regex, s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last = 0;
    for m in re.find_iter(s) {
        let before = s[..m.start()].chars().next_back();
        let after = s[m.end()..].chars().next();
        let touches = |c: Option<char>| c.is_some_and(|c| c.is_ascii_alphanumeric());
        if touches(before) || touches(after) {
            continue;
        }
        out.push_str(&s[last..m.start()]);
        out.push_str(Redactor::MARK);
        last = m.end();
    }
    out.push_str(&s[last..]);
    out
}

/// Swift `String.count` / `prefix(n)` work on grapheme clusters, not code points.
pub fn char_count(s: &str) -> usize {
    s.graphemes(true).count()
}

pub fn prefix_chars(s: &str, n: usize) -> String {
    match s.grapheme_indices(true).nth(n) {
        Some((i, _)) => s[..i].to_string(),
        None => s.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookbehind_emulation() {
        // The letter before "password" blocks it; the start of a line or a non-letter does not.
        assert_eq!(Redactor::redact("mypassword: x"), "mypassword: x");
        assert_eq!(Redactor::redact("a\nMY_PASSWORD=1\nb"), "a\n[redacted]\nb");
        assert_eq!(Redactor::redact("password: a\npassword: b"), "[redacted]\n[redacted]");
        assert_eq!(Redactor::redact("x\r\npwd = 1\r\ny"), "x\r\n[redacted]\r\ny");
        // Hex touching a letter or digit on either side stays.
        let hex = "9f86d081884c7d659a2feaa0c55ad015a3";
        assert_eq!(Redactor::redact(&format!("({hex})")), "([redacted])");
        assert_eq!(Redactor::redact(&format!("z{hex}")), format!("z{hex}"));
        assert_eq!(Redactor::redact(&format!("{hex}g")), format!("{hex}g"));
        assert!(Redactor::looks_random("dGhpcyBpcyBhIHNlY3JldCBrZXkgZm9yIHRlc3Rpbmc0MjQy"));
        assert!(!Redactor::looks_random("internationalization-and-localization"));
    }

    #[test]
    fn grapheme_cap() {
        let s = "e\u{301}".repeat(5); // 5 characters, 10 code points
        assert_eq!(char_count(&s), 5);
        assert_eq!(prefix_chars(&s, 2), "e\u{301}e\u{301}");
        assert_eq!(Redactor::clean_cap("✓✓✓", 2), "✓✓");
    }
}
