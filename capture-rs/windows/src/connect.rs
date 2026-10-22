// context-capsule — windows: finding the Claude app and "Connect Claude" — opens the Context Capsule extension
// (context-capsule.mcpb, shipped next to the exe) with the Claude app, which asks to install it, and tells whether
// it's connected. Claude may be a classic install, a packaged (MSIX / Store) app, or only reachable by its claude:
// link or app alias: every signal is checked and written to app.log.
//! Connected = the extension is in Claude's "Claude Extensions" folder, or claude_desktop_config.json
//! has a "context-capsule" server. Claude Code doesn't count here: this is about the Claude app.

use std::path::{Path, PathBuf};

/// The extension file, next to context-capsule.exe (built by mcp/extension/build.sh).
pub const EXTENSION_FILE: &str = "context-capsule.mcpb";
pub const SERVER_NAME: &str = "context-capsule";
pub const DOWNLOAD_URL: &str = "https://claude.ai/download";
/// Opens the Claude app (a new chat).
pub const CLAUDE_LINK: &str = "claude://claude.ai/new";

/// When Claude can't open the extension itself (Claude Desktop's menu path, from its extension docs).
pub const MANUAL_STEPS: &str = "In Claude: Settings → Extensions → Advanced settings → Install Extension…, then pick \
                                context-capsule.mcpb (shown in the folder that just opened).";
pub const MANUAL_TITLE: &str = "Install the extension in Claude";
pub const IS_IT_INSTALLED: &str = "Is the Claude app installed?";
pub const IS_IT_INSTALLED_BODY: &str = "Context Capsule works with the Claude desktop app, but couldn't find it on this PC.\n\n\
                                        Yes: show me how to connect it.\nNo: open the download page.";

pub fn extension_next_to(exe: &Path) -> PathBuf {
    exe.parent().unwrap_or(Path::new(".")).join(EXTENSION_FILE)
}

/// Any "mcpServers" object (at any depth) with a "context-capsule" entry.
pub fn contains_server(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Object(map) => {
            map.get("mcpServers").and_then(|s| s.as_object()).is_some_and(|s| s.contains_key(SERVER_NAME))
                || map.values().any(contains_server)
        }
        serde_json::Value::Array(list) => list.iter().any(contains_server),
        _ => false,
    }
}

/// Claude's data folders: `%APPDATA%\Claude`, and the Microsoft Store app's private copy under
/// `%LOCALAPPDATA%\Packages\<Claude…>\LocalCache\Roaming\Claude`.
pub fn claude_data_dirs(appdata: Option<&Path>, local_appdata: Option<&Path>) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = appdata.map(|a| a.join("Claude")).into_iter().collect();
    if let Some(Ok(entries)) = local_appdata.map(|l| std::fs::read_dir(l.join("Packages"))) {
        for e in entries.flatten() {
            if is_claude_package(&e.file_name().to_string_lossy()) {
                dirs.push(e.path().join("LocalCache").join("Roaming").join("Claude"));
            }
        }
    }
    dirs
}

/// Is Context Capsule connected in any of Claude's data folders?
pub fn is_connected_in(dirs: &[PathBuf]) -> bool {
    dirs.iter().any(|dir| {
        let listed = std::fs::read_dir(dir.join("Claude Extensions"))
            .map(|entries| entries.flatten().any(|e| e.file_name().to_string_lossy().to_lowercase().contains(SERVER_NAME)))
            .unwrap_or(false);
        listed
            || std::fs::read(dir.join("claude_desktop_config.json"))
                .ok()
                .and_then(|d| serde_json::from_slice::<serde_json::Value>(&d).ok())
                .is_some_and(|v| contains_server(&v))
    })
}

/// Is this program (a path, an app's friendly name, a ProgID or a packaged app id) the Claude app?
pub fn is_claude_app(name: &str) -> bool {
    let file = name.rsplit(['\\', '/']).next().unwrap_or(name).to_lowercase();
    file.contains("claude")
}

/// A packaged app's name or full name ("Claude_1.0.0.0_x64__pzs8sxrjxfjjc", "AnthropicPBC.Claude_…") or package
/// folder ("Claude_pzs8sxrjxfjjc"): is it Claude?
pub fn is_claude_package(name: &str) -> bool {
    name.split('_').next().is_some_and(|n| n.to_lowercase().contains("claude"))
}

/// "Name_Version_Arch_ResourceId_PublisherId" → "Name_PublisherId" (the package family name).
pub fn package_family(full_name: &str) -> Option<String> {
    let parts: Vec<&str> = full_name.split('_').collect();
    match parts.as_slice() {
        [name, .., publisher] if parts.len() >= 3 && !name.is_empty() && !publisher.is_empty() => Some(format!("{name}_{publisher}")),
        // Already a family name ("Claude_pzs8sxrjxfjjc").
        [name, publisher] if !name.is_empty() && !publisher.is_empty() => Some(full_name.to_string()),
        _ => None,
    }
}

/// The app inside a package, from the subkeys of its Repository\Packages entry (the app ids, plus a few others).
pub fn package_app_id(subkeys: &[String]) -> Option<String> {
    const NOT_APPS: [&str; 5] = ["capabilities", "applicationcontentrules", "packagedependencies", "packageextensions", "resources"];
    subkeys.iter().find(|k| !k.is_empty() && !NOT_APPS.contains(&k.to_lowercase().as_str())).cloned()
}

/// An "Add or remove programs" entry: is it the Claude app?
pub fn is_claude_uninstall(display_name: &str, publisher: &str) -> bool {
    let name = display_name.trim().to_lowercase();
    let by_anthropic = publisher.to_lowercase().contains("anthropic") || name.contains("anthropic");
    by_anthropic && (name == "claude" || name.starts_with("claude ") || name.contains("claude desktop") || name == "anthropic claude")
}

/// `"C:\…\claude.exe",0` → `C:\…\claude.exe` (an uninstall entry's DisplayIcon, or an App Paths value).
pub fn exe_from_value(value: &str) -> Option<PathBuf> {
    let v = value.trim();
    let v = match v.strip_prefix('"') {
        Some(rest) => rest.split('"').next().unwrap_or(""),
        None => v.rsplit_once(',').filter(|(_, n)| n.trim().parse::<i32>().is_ok()).map_or(v, |(p, _)| p),
    };
    let v = v.trim();
    v.to_lowercase().ends_with(".exe").then(|| PathBuf::from(v))
}

/// How to open the Claude app.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClaudeOpener {
    /// The claude: link scheme is registered: open a new chat (with the request typed in).
    Link,
    /// Start this program (claude.exe, or the app alias).
    App(PathBuf),
    /// A packaged app, started through shell:AppsFolder with this app id ("Claude_pzs8sxrjxfjjc!Claude").
    Package(String),
    /// Nothing that opens it was found.
    Missing,
}

/// What this PC has of the Claude app. Each field is one signal; any of them means it's installed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Found {
    /// .mcpb files open with Claude.
    pub opens_mcpb: bool,
    /// The claude: link scheme is registered (for this user or the PC).
    pub link: bool,
    /// claude.exe: the classic install, App Paths, or the uninstall entry.
    pub exe: Option<PathBuf>,
    /// The app alias %LOCALAPPDATA%\Microsoft\WindowsApps\claude.exe.
    pub alias: Option<PathBuf>,
    /// A packaged (MSIX / Store) Claude: its package family name.
    pub package: Option<String>,
    /// …and the app id to start it with, when known.
    pub package_app: Option<String>,
    /// An "Add or remove programs" entry for Claude.
    pub uninstall: bool,
    /// Claude's data folder exists: it has run on this PC.
    pub data: bool,
}

/// How "Connect" hands the extension to Claude.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConnectPlan {
    /// Open the .mcpb: Claude is its app.
    OpenFile,
    /// Run claude.exe with the .mcpb.
    OpenWith(PathBuf),
    /// Claude is there but can't take the file directly: open Claude, show the file, say where to install it.
    Manual(ClaudeOpener),
    /// Nothing found: ask whether Claude is installed.
    Ask,
}

impl Found {
    pub fn installed(&self) -> bool {
        self.opens_mcpb || self.link || self.exe.is_some() || self.alias.is_some() || self.package.is_some() || self.uninstall || self.data
    }

    /// The best way to open Claude: its link, its exe, its alias, its package.
    pub fn opener(&self) -> ClaudeOpener {
        if self.link {
            ClaudeOpener::Link
        } else if let Some(exe) = self.exe.as_ref().or(self.alias.as_ref()) {
            ClaudeOpener::App(exe.clone())
        } else if let (Some(family), Some(app)) = (&self.package, &self.package_app) {
            ClaudeOpener::Package(format!("{family}!{app}"))
        } else {
            ClaudeOpener::Missing
        }
    }

    pub fn connect_plan(&self) -> ConnectPlan {
        if self.opens_mcpb {
            ConnectPlan::OpenFile
        } else if let Some(exe) = &self.exe {
            ConnectPlan::OpenWith(exe.clone())
        } else if self.installed() {
            ConnectPlan::Manual(self.opener())
        } else {
            ConnectPlan::Ask
        }
    }

    /// One line for app.log: what was found.
    pub fn log_line(&self) -> String {
        let path = |p: &Option<PathBuf>| p.as_ref().map_or("no".to_string(), |p| p.display().to_string());
        let yes = |b: bool| if b { "yes" } else { "no" };
        format!(
            "claude app: .mcpb opens with Claude {}, claude: link {}, exe {}, alias {}, package {}{}, uninstall entry {}, data folder {}",
            yes(self.opens_mcpb),
            yes(self.link),
            path(&self.exe),
            path(&self.alias),
            self.package.as_deref().unwrap_or("no"),
            self.package_app.as_ref().map_or(String::new(), |a| format!(" (app {a})")),
            yes(self.uninstall),
            yes(self.data),
        )
    }
}

#[cfg(windows)]
pub use win::{connect, find, is_connected, open_claude};

#[cfg(windows)]
mod win {
    use super::*;
    use crate::win;
    use context_capsule_core::AppLog;
    use windows::core::{HSTRING, PCWSTR, PWSTR};
    use windows::Win32::UI::Shell::{
        AssocQueryStringW, ASSOCF, ASSOCF_IS_PROTOCOL, ASSOCF_NONE, ASSOCSTR, ASSOCSTR_APPID, ASSOCSTR_COMMAND, ASSOCSTR_EXECUTABLE,
        ASSOCSTR_FRIENDLYAPPNAME, ASSOCSTR_PROGID,
    };

    fn env_dir(name: &str) -> Option<PathBuf> {
        std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from)
    }

    pub fn is_connected() -> bool {
        is_connected_in(&claude_data_dirs(env_dir("APPDATA").as_deref(), env_dir("LOCALAPPDATA").as_deref()))
    }

    fn assoc(flags: ASSOCF, what: ASSOCSTR, of: &str) -> Option<String> {
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let of = HSTRING::from(of);
        // SAFETY: `buf` holds `len` chars; the answer is NUL-terminated within it.
        unsafe { AssocQueryStringW(flags, what, PCWSTR(of.as_ptr()), None, Some(PWSTR(buf.as_mut_ptr())), &mut len) }.ok().ok()?;
        let end = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
        Some(String::from_utf16_lossy(&buf[..end])).filter(|s| !s.trim().is_empty())
    }

    const UNINSTALL: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall";
    const UNINSTALL_32: &str = r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall";
    const PACKAGES: &str = r"Software\Classes\Local Settings\Software\Microsoft\Windows\CurrentVersion\AppModel\Repository\Packages";

    /// Look everywhere Claude can be. Takes a moment (the registry): not on the UI thread when it can be helped.
    pub fn find() -> Found {
        let local = env_dir("LOCALAPPDATA");
        // .mcpb opens with Claude (a classic install registers it; a packaged app may too).
        let opens_mcpb = [ASSOCSTR_EXECUTABLE, ASSOCSTR_FRIENDLYAPPNAME, ASSOCSTR_PROGID, ASSOCSTR_APPID]
            .into_iter()
            .filter_map(|what| assoc(ASSOCF_NONE, what, ".mcpb"))
            .any(|a| is_claude_app(&a));

        // The claude: link, for this user or the PC (HKCR merges both; checked one by one too).
        let link = win::reg_string_classes("claude", "URL Protocol").is_some()
            || win::reg_string_classes(r"claude\shell\open\command", "").is_some_and(|c| !c.trim().is_empty())
            || [false, true].into_iter().any(|machine| win::reg_string_at(machine, r"Software\Classes\claude", "URL Protocol").is_some())
            || assoc(ASSOCF_IS_PROTOCOL, ASSOCSTR_COMMAND, "claude").is_some()
            || assoc(ASSOCF_IS_PROTOCOL, ASSOCSTR_APPID, "claude").is_some();
        let mut f = Found { opens_mcpb, link, ..Default::default() };

        // claude.exe: the classic places, App Paths, an uninstall entry.
        let mut exes: Vec<PathBuf> = vec![];
        if let Some(l) = &local {
            exes.push(l.join("AnthropicClaude").join("claude.exe"));
            exes.push(l.join("Programs").join("Claude").join("Claude.exe"));
        }
        if let Some(p) = env_dir("ProgramFiles") {
            exes.push(p.join("Claude").join("Claude.exe"));
        }
        for machine in [false, true] {
            if let Some(e) = win::reg_string_at(machine, r"Software\Microsoft\Windows\CurrentVersion\App Paths\claude.exe", "").and_then(|v| exe_from_value(&v)) {
                exes.push(e);
            }
        }
        for (machine, key) in [(false, UNINSTALL), (true, UNINSTALL), (true, UNINSTALL_32)] {
            for sub in win::reg_subkeys(machine, key) {
                let at = format!(r"{key}\{sub}");
                let name = win::reg_string_at(machine, &at, "DisplayName").unwrap_or_default();
                let publisher = win::reg_string_at(machine, &at, "Publisher").unwrap_or_default();
                if !is_claude_uninstall(&name, &publisher) {
                    continue;
                }
                f.uninstall = true;
                if let Some(e) = win::reg_string_at(machine, &at, "DisplayIcon").and_then(|v| exe_from_value(&v)) {
                    exes.push(e);
                }
                if let Some(dir) = win::reg_string_at(machine, &at, "InstallLocation").filter(|d| !d.trim().is_empty()) {
                    exes.push(PathBuf::from(dir.trim().trim_matches('"')).join("claude.exe"));
                }
            }
        }
        f.exe = exes.into_iter().find(|e| e.is_file() && is_claude_app(&e.to_string_lossy()));

        // The app alias (a packaged or classic app may add one). A special link: its own entry is enough.
        f.alias = local
            .as_ref()
            .map(|l| l.join("Microsoft").join("WindowsApps").join("claude.exe"))
            .filter(|a| std::fs::symlink_metadata(a).is_ok());

        // A packaged (MSIX / Store) app: its registry entry, else its folder under %LOCALAPPDATA%\Packages.
        if let Some(full) = win::reg_subkeys(false, PACKAGES).into_iter().find(|p| is_claude_package(p)) {
            f.package = package_family(&full);
            f.package_app = package_app_id(&win::reg_subkeys(false, &format!(r"{PACKAGES}\{full}")));
        }
        if f.package.is_none() {
            if let Some(Ok(entries)) = local.as_ref().map(|l| std::fs::read_dir(l.join("Packages"))) {
                f.package = entries.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).find(|n| is_claude_package(n));
            }
        }

        f.data = claude_data_dirs(env_dir("APPDATA").as_deref(), local.as_deref()).iter().any(|d| d.is_dir());
        f
    }

    /// Start Claude the way `how` says (a new chat when it has the link). false: nothing opened.
    pub fn open_claude(how: &ClaudeOpener, link: &str) -> bool {
        match how {
            ClaudeOpener::Link => win::open(link),
            ClaudeOpener::App(exe) => win::open(&exe.to_string_lossy()),
            ClaudeOpener::Package(app) => win::open_with("explorer.exe", Some(&format!(r"shell:AppsFolder\{app}"))),
            ClaudeOpener::Missing => false,
        }
    }

    /// Claude can't take the file itself: open Claude (when it can be), show the file in its folder, say where to go.
    fn manual(how: &ClaudeOpener, file: &Path, log: &AppLog) {
        let opened = open_claude(how, CLAUDE_LINK);
        let shown = win::show_in_folder(file);
        log.write(&format!("connect: by hand (Claude {}, file {})", if opened { "opened" } else { "not opened" }, if shown { "shown" } else { "not shown" }));
        win::alert(None, MANUAL_TITLE, MANUAL_STEPS);
    }

    /// Open the extension in the Claude app (it asks the user to install). Not found: ask whether it's installed.
    pub fn connect(log: &AppLog) {
        let found = find();
        log.write(&found.log_line());
        let file = std::env::current_exe().map(|e| extension_next_to(&e)).unwrap_or_else(|_| PathBuf::from(EXTENSION_FILE));
        if !file.is_file() {
            log.write("connect: the extension file is missing next to the exe");
            win::alert(
                None,
                "Something is missing",
                &format!("{EXTENSION_FILE} should be in the same folder as context-capsule.exe. Please download Context Capsule again."),
            );
            return;
        }
        let path = file.to_string_lossy().into_owned();
        match found.connect_plan() {
            ConnectPlan::OpenFile => {
                log.write("connect: opening the extension with Claude");
                if !win::open(&path) {
                    log.write("connect: Windows could not open the extension");
                    manual(&found.opener(), &file, log);
                }
            }
            ConnectPlan::OpenWith(exe) => {
                log.write("connect: opening the extension with claude.exe");
                if !win::open_with(&exe.to_string_lossy(), Some(&format!("\"{path}\""))) {
                    log.write("connect: claude.exe could not open the extension");
                    manual(&found.opener(), &file, log);
                }
            }
            ConnectPlan::Manual(how) => manual(&how, &file, log),
            ConnectPlan::Ask => {
                if win::ask_yes_no(IS_IT_INSTALLED, IS_IT_INSTALLED_BODY) {
                    log.write("connect: not found, but the user says Claude is installed");
                    manual(&ClaudeOpener::Missing, &file, log);
                } else {
                    log.write("connect: Claude is not installed: download page");
                    win::open(DOWNLOAD_URL);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_anywhere_in_the_config() {
        let v = |s: &str| serde_json::from_str::<serde_json::Value>(s).unwrap();
        assert!(contains_server(&v(r#"{"mcpServers":{"context-capsule":{"command":"node"}}}"#)));
        assert!(contains_server(&v(r#"{"profiles":[{"mcpServers":{"context-capsule":{}}}]}"#)), "nested");
        assert!(!contains_server(&v(r#"{"mcpServers":{"other":{}}}"#)));
        assert!(!contains_server(&v(r#"{"context-capsule":{}}"#)), "only inside mcpServers");
        assert!(!contains_server(&v("[]")));
    }

    #[test]
    fn connected_from_extensions_or_config() {
        let root = std::env::temp_dir().join(format!("cc-connect-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (appdata, local) = (root.join("Roaming"), root.join("Local"));
        std::fs::create_dir_all(appdata.join("Claude").join("Claude Extensions").join("other-ext")).unwrap();
        let store = local.join("Packages").join("Claude_pzs8sxrjxfjjc");
        std::fs::create_dir_all(store.join("LocalCache").join("Roaming").join("Claude")).unwrap();
        std::fs::create_dir_all(local.join("Packages").join("Microsoft.Paint_8wekyb3d8bbwe")).unwrap();

        let dirs = claude_data_dirs(Some(&appdata), Some(&local));
        assert_eq!(dirs.len(), 2, "%APPDATA%\\Claude and the Store app's copy: {dirs:?}");
        assert!(!is_connected_in(&dirs));

        std::fs::create_dir_all(appdata.join("Claude").join("Claude Extensions").join("local.mcpb.context-capsule")).unwrap();
        assert!(is_connected_in(&dirs), "the extension is installed");

        std::fs::remove_dir_all(appdata.join("Claude")).unwrap();
        let config = store.join("LocalCache").join("Roaming").join("Claude").join("claude_desktop_config.json");
        std::fs::write(&config, r#"{"mcpServers":{"context-capsule":{"command":"node"}}}"#).unwrap();
        assert!(is_connected_in(&claude_data_dirs(Some(&appdata), Some(&local))), "a config entry in the Store app's folder");
        std::fs::write(&config, "not json").unwrap();
        assert!(!is_connected_in(&claude_data_dirs(Some(&appdata), Some(&local))));
        assert!(claude_data_dirs(None, None).is_empty());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn claude_app_names() {
        assert!(is_claude_app(r"C:\Users\s\AppData\Local\AnthropicClaude\app-0.14.10\claude.exe"));
        assert!(is_claude_app("Claude"));
        assert!(is_claude_app("Claude_pzs8sxrjxfjjc!Claude"));
        assert!(!is_claude_app(r"C:\Windows\System32\notepad.exe"));
        assert_eq!(extension_next_to(Path::new("/kit/context-capsule.exe")), Path::new("/kit/context-capsule.mcpb"));
    }

    #[test]
    fn packages() {
        assert!(is_claude_package("Claude_1.0.0.0_x64__pzs8sxrjxfjjc"));
        assert!(is_claude_package("AnthropicPBC.Claude_0.14.10.0_x64__abc"));
        assert!(is_claude_package("Claude_pzs8sxrjxfjjc"));
        assert!(!is_claude_package("Microsoft.Paint_11.0.0.0_x64__8wekyb3d8bbwe"));
        assert!(!is_claude_package("Contoso.App_1.0.0.0_x64__claude"), "only the name counts, not the publisher");
        assert_eq!(package_family("Claude_1.0.0.0_x64__pzs8sxrjxfjjc").as_deref(), Some("Claude_pzs8sxrjxfjjc"));
        assert_eq!(package_family("AnthropicPBC.Claude_0.14.10.0_neutral_~_abc").as_deref(), Some("AnthropicPBC.Claude_abc"));
        assert_eq!(package_family("Claude_pzs8sxrjxfjjc").as_deref(), Some("Claude_pzs8sxrjxfjjc"));
        assert_eq!(package_family("Claude"), None);
        let keys = |k: &[&str]| k.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(package_app_id(&keys(&["Capabilities", "Claude"])).as_deref(), Some("Claude"));
        assert_eq!(package_app_id(&keys(&["Capabilities"])), None);
    }

    #[test]
    fn uninstall_entries() {
        assert!(is_claude_uninstall("Claude", "Anthropic PBC"));
        assert!(is_claude_uninstall("Claude 0.14.10", "Anthropic, PBC"));
        assert!(!is_claude_uninstall("Claude", "Someone else"), "only Anthropic's");
        assert!(!is_claude_uninstall("Claudette", "Anthropic"));
        assert_eq!(exe_from_value(r#""C:\Users\s\AppData\Local\AnthropicClaude\claude.exe",0"#), Some(PathBuf::from(r"C:\Users\s\AppData\Local\AnthropicClaude\claude.exe")));
        assert_eq!(exe_from_value(r"C:\Program Files\Claude\Claude.exe,0"), Some(PathBuf::from(r"C:\Program Files\Claude\Claude.exe")));
        assert_eq!(exe_from_value(r"C:\Program Files\Claude\Claude.exe"), Some(PathBuf::from(r"C:\Program Files\Claude\Claude.exe")));
        assert_eq!(exe_from_value(r"C:\x\icon.ico"), None);
    }

    #[test]
    fn what_connect_does() {
        let exe = PathBuf::from(r"C:\x\claude.exe");
        assert_eq!(Found::default().connect_plan(), ConnectPlan::Ask, "nothing found: ask, don't assume");
        assert!(!Found::default().installed());
        assert_eq!(Found { opens_mcpb: true, link: true, ..Default::default() }.connect_plan(), ConnectPlan::OpenFile);
        assert_eq!(Found { exe: Some(exe.clone()), ..Default::default() }.connect_plan(), ConnectPlan::OpenWith(exe.clone()));
        // The tester's case: a packaged Claude with only its link registered.
        let store = Found { link: true, package: Some("Claude_pzs8sxrjxfjjc".into()), ..Default::default() };
        assert_eq!(store.connect_plan(), ConnectPlan::Manual(ClaudeOpener::Link));
        let alias = PathBuf::from(r"C:\Users\s\AppData\Local\Microsoft\WindowsApps\claude.exe");
        assert_eq!(Found { alias: Some(alias.clone()), ..Default::default() }.connect_plan(), ConnectPlan::Manual(ClaudeOpener::App(alias)));
        let pkg = Found { package: Some("Claude_p".into()), package_app: Some("Claude".into()), ..Default::default() };
        assert_eq!(pkg.opener(), ClaudeOpener::Package("Claude_p!Claude".into()));
        assert_eq!(Found { data: true, ..Default::default() }.connect_plan(), ConnectPlan::Manual(ClaudeOpener::Missing), "it has run here");
        assert_eq!(Found { link: true, exe: Some(exe.clone()), ..Default::default() }.opener(), ClaudeOpener::Link, "link first");
        let line = store.log_line();
        assert!(line.contains("claude: link yes") && line.contains("package Claude_pzs8sxrjxfjjc") && line.contains("exe no"), "{line}");
        assert!(MANUAL_STEPS.contains("Settings → Extensions → Advanced settings → Install Extension…"));
    }
}
