// context-capsule — core: opening Claude on a new chat with a request typed in — "continue from this capsule"
// (a capsule someone sent: Open With, or "Open a capsule…"). Also which file a launch asks to open (`--open <path>`,
// or a .md path as the only argument).

use crate::reminder::front_matter;
use std::path::{Path, PathBuf};

/// A new chat in the Claude app, with `prompt` typed in (not sent).
pub fn new_chat_url(prompt: &str) -> String {
    format!("claude://claude.ai/new?q={}", encode_query(prompt))
}

/// Percent-encoding for a URL query value: letters, digits and `-._~` stay, every other byte (UTF-8) is `%XX`.
pub fn encode_query(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// What Claude is asked when a received capsule is opened.
pub fn continue_prompt(path: &Path) -> String {
    format!("Use Context Capsule to continue from the capsule at {}.", path.display())
}

/// Does this look like a capsule: front matter with `capsule:` and `title:`?
pub fn is_capsule(text: &str) -> bool {
    let front = front_matter(text);
    let has = |k: &str| front.iter().any(|(key, value)| key == k && !value.is_empty());
    has("capsule") && has("title")
}

/// The file a launch asks to open: `--open <path>` (or `--open=<path>`), else a lone argument ending in `.md`.
/// Made absolute against `cwd`.
pub fn open_arg(args: &[String], cwd: &Path) -> Option<PathBuf> {
    let mut found: Option<String> = None;
    let mut it = args.iter().skip(1);
    while let Some(a) = it.next() {
        if a == "--open" {
            found = it.next().cloned();
            break;
        }
        if let Some(p) = a.strip_prefix("--open=") {
            found = Some(p.to_string());
            break;
        }
        if !a.starts_with('-') && a.to_lowercase().ends_with(".md") {
            found = Some(a.clone());
        }
    }
    let raw = found.filter(|p| !p.trim().is_empty())?;
    let p = PathBuf::from(raw.trim());
    Some(if p.is_absolute() { p } else { cwd.join(p) })
}

/// Opening a file that isn't a capsule.
pub const NOT_A_CAPSULE: &str = "Not a capsule";
pub const NOT_A_CAPSULE_BODY: &str = "This file isn't a Context Capsule, so there's nothing to continue from.";
pub const CANT_READ: &str = "Couldn't open that file.";

/// What opening `path` should do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Opened {
    /// A capsule: open Claude with this request.
    Capsule { prompt: String },
    NotACapsule,
    Unreadable,
}

/// Read `path` and decide.
pub fn open_file(path: &Path) -> Opened {
    match std::fs::read(path) {
        Ok(bytes) if is_capsule(&String::from_utf8_lossy(&bytes).replace("\r\n", "\n")) => Opened::Capsule { prompt: continue_prompt(path) },
        Ok(_) => Opened::NotACapsule,
        Err(_) => Opened::Unreadable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reminder::{CLAUDE_NEW_CHAT, DAY_PROMPT};

    #[test]
    fn urls() {
        assert_eq!(new_chat_url(DAY_PROMPT), CLAUDE_NEW_CHAT, "the same encoding as the evening reminder's link");
        assert_eq!(encode_query("a b&c=d/é?"), "a%20b%26c%3Dd%2F%C3%A9%3F");
        assert_eq!(
            new_chat_url(&continue_prompt(Path::new(r"C:\Users\ana\Pricing.capsule.md"))),
            "claude://claude.ai/new?q=Use%20Context%20Capsule%20to%20continue%20from%20the%20capsule%20at%20C%3A%5CUsers%5Cana%5CPricing.capsule.md."
        );
    }

    #[test]
    fn capsules_and_not() {
        assert!(is_capsule("---\ncapsule: 1\ntitle: Pricing\n---\n## In short\n"));
        assert!(!is_capsule("---\ntitle: Notes\n---\n"), "no capsule:");
        assert!(!is_capsule("---\ncapsule: 1\n---\n"), "no title:");
        assert!(!is_capsule("# Just Markdown\n"));
        let dir = std::env::temp_dir().join(format!("cc-open-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (cap, other) = (dir.join("a.capsule.md"), dir.join("b.md"));
        std::fs::write(&cap, "---\r\ncapsule: 1\r\ntitle: A\r\n---\r\n").unwrap();
        std::fs::write(&other, "hello").unwrap();
        assert_eq!(open_file(&cap), Opened::Capsule { prompt: continue_prompt(&cap) });
        assert_eq!(open_file(&other), Opened::NotACapsule);
        assert_eq!(open_file(&dir.join("missing.md")), Opened::Unreadable);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn launch_arguments() {
        let a = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let cwd = &std::env::temp_dir();
        let full = cwd.join("x").join("a.capsule.md");
        assert_eq!(open_arg(&a(&["cc", "--open", &full.to_string_lossy()]), cwd), Some(full.clone()));
        assert_eq!(open_arg(&a(&["cc", "--open=b.md"]), cwd), Some(cwd.join("b.md")));
        assert_eq!(open_arg(&a(&["cc", "Notes.MD"]), cwd), Some(cwd.join("Notes.MD")));
        assert_eq!(open_arg(&a(&["cc", "--login"]), cwd), None);
        assert_eq!(open_arg(&a(&["cc", "--updated", "42"]), cwd), None);
        assert_eq!(open_arg(&a(&["cc", "--open"]), cwd), None);
    }
}
