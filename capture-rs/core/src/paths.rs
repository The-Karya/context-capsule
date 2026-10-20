// context-capsule — core: where the data lives. Same rule as the MCP server (mcp/src/paths.ts).
//! `CONTEXT_CAPSULE_HOME` if set; else `$XDG_DATA_HOME/context-capsule` on Linux when that is an
//! absolute path; else `<home>/.context-capsule` (home = `HOME`, else `USERPROFILE` on Windows).

use std::path::{Path, PathBuf};

/// Which rules apply. Passed in so the rule is testable on any machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Os {
    MacOs,
    Windows,
    Linux,
    Other,
}

impl Os {
    pub fn current() -> Os {
        if cfg!(target_os = "macos") {
            Os::MacOs
        } else if cfg!(windows) {
            Os::Windows
        } else if cfg!(target_os = "linux") {
            Os::Linux
        } else {
            Os::Other
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Paths {
    pub root: PathBuf,
}

impl Paths {
    pub fn new(root: impl Into<PathBuf>) -> Paths {
        Paths { root: root.into() }
    }

    /// The root for this machine, from the real environment (made absolute).
    pub fn standard() -> Paths {
        let root = root_from_env(Os::current(), &|k| std::env::var(k).ok());
        Paths { root: std::path::absolute(&root).unwrap_or(root) }
    }

    pub fn sessions(&self) -> PathBuf {
        self.root.join("sessions")
    }
    pub fn session_dir(&self, id: &str) -> PathBuf {
        self.sessions().join(id)
    }
    pub fn capsules(&self) -> PathBuf {
        self.root.join("capsules")
    }
    pub fn config(&self) -> PathBuf {
        self.root.join("config.json")
    }
    pub fn control(&self) -> PathBuf {
        self.root.join("control")
    }
    pub fn state(&self) -> PathBuf {
        self.root.join("state.json")
    }
    /// Diagnostics: counts, durations, reasons and error codes only (see `AppLog`).
    pub fn log(&self) -> PathBuf {
        self.root.join("app.log")
    }
    /// The user's projects (written by the app and the MCP server, editable by hand).
    pub fn projects(&self) -> PathBuf {
        self.root.join("projects.json")
    }
}

/// The data root as a pure function of an environment lookup. Empty values count as unset.
/// A relative `CONTEXT_CAPSULE_HOME` is returned as is (`Paths::standard` makes it absolute).
pub fn root_from_env(os: Os, env: &dyn Fn(&str) -> Option<String>) -> PathBuf {
    let get = |k: &str| env(k).map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
    let home = || -> PathBuf {
        let h = get("HOME").or_else(|| if os == Os::Windows { get("USERPROFILE") } else { None });
        PathBuf::from(h.unwrap_or_default())
    };
    if let Some(dir) = get("CONTEXT_CAPSULE_HOME") {
        return expand_home(&dir, &home);
    }
    if os == Os::Linux {
        if let Some(xdg) = get("XDG_DATA_HOME") {
            if xdg.starts_with('/') {
                return Path::new(&xdg).join("context-capsule");
            }
        }
    }
    home().join(".context-capsule")
}

/// A leading `~` means the home dir.
fn expand_home(p: &str, home: &dyn Fn() -> PathBuf) -> PathBuf {
    if p == "~" {
        return home();
    }
    if let Some(rest) = p.strip_prefix("~/").or_else(|| p.strip_prefix("~\\")) {
        return home().join(rest);
    }
    PathBuf::from(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |k| pairs.iter().find(|(n, _)| *n == k).map(|(_, v)| v.to_string())
    }

    #[test]
    fn root_rules() {
        let home = Path::new("/home/s");
        assert_eq!(root_from_env(Os::Linux, &env(&[("HOME", "/home/s")])), home.join(".context-capsule"));
        assert_eq!(
            root_from_env(Os::Linux, &env(&[("HOME", "/home/s"), ("XDG_DATA_HOME", "/home/s/.local/share")])),
            Path::new("/home/s/.local/share").join("context-capsule")
        );
        assert_eq!(
            root_from_env(Os::Linux, &env(&[("HOME", "/home/s"), ("XDG_DATA_HOME", "rel")])),
            home.join(".context-capsule"),
            "relative XDG_DATA_HOME is ignored"
        );
        assert_eq!(
            root_from_env(Os::MacOs, &env(&[("HOME", "/Users/s"), ("XDG_DATA_HOME", "/x")])),
            Path::new("/Users/s").join(".context-capsule"),
            "XDG only on Linux"
        );
        assert_eq!(
            root_from_env(Os::Linux, &env(&[("HOME", "/home/s"), ("XDG_DATA_HOME", "/x"), ("CONTEXT_CAPSULE_HOME", " /data/cc ")])),
            Path::new("/data/cc"),
            "CONTEXT_CAPSULE_HOME wins, trimmed"
        );
        assert_eq!(
            root_from_env(Os::Linux, &env(&[("HOME", "/home/s"), ("CONTEXT_CAPSULE_HOME", "~/cc")])),
            home.join("cc")
        );
        assert_eq!(
            root_from_env(Os::Windows, &env(&[("USERPROFILE", r"C:\Users\s"), ("HOME", "  ")])),
            Path::new(r"C:\Users\s").join(".context-capsule"),
            "USERPROFILE when HOME is unset or blank"
        );
        let p = Paths::new("/r");
        assert_eq!(p.session_dir("20261019-100103"), Path::new("/r/sessions/20261019-100103"));
        assert_eq!(p.state(), Path::new("/r/state.json"));
        assert_eq!(p.control(), Path::new("/r/control"));
        assert_eq!(p.config(), Path::new("/r/config.json"));
        assert_eq!(p.capsules(), Path::new("/r/capsules"));
        assert_eq!(p.projects(), Path::new("/r/projects.json"));
        assert_eq!(p.log(), Path::new("/r/app.log"));
    }
}
