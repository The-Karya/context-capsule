// context-capsule — windows: setting up — Voice notes (the microphone allowed), Connect your AI and Open at login:
// what's done and what each step's button does. Shown as the "Set up" rows in the window's Settings, which opens
// there on first launch. No "Read your apps" row: UI Automation needs no permission on Windows.

use context_capsule_core::window::{SetupRow, ROW_AI, ROW_LOGIN, ROW_VOICE};

/// The registry key and value that say the window was shown once (first launch opens it on Settings).
pub const APP_KEY: &str = r"Software\Context Capsule";
pub const SEEN_VALUE: &str = "SetupShown";
/// Set once the app has turned "open at login" on by itself.
pub const LOGIN_SET_VALUE: &str = "LoginSet";

/// One setup step.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Step {
    Voice,
    /// "Connect your AI": the AI apps are listed under it (`ai`); Claude's own Connect is `connect::connect`.
    Claude,
    Login,
}

impl Step {
    pub const ALL: [Step; 3] = [Step::Voice, Step::Claude, Step::Login];

    /// Its words (the same as the Mac's).
    pub fn row(self) -> SetupRow {
        match self {
            Step::Voice => ROW_VOICE,
            Step::Claude => ROW_AI,
            Step::Login => ROW_LOGIN,
        }
    }
}

/// What's done, checked every second while Settings is open.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Done {
    pub voice: bool,
    /// Claude's extension is installed (the app's own detection; any other connected AI also shows the ✓).
    pub claude: bool,
    pub login: bool,
}

impl Done {
    pub fn get(&self, step: Step) -> bool {
        match step {
            Step::Voice => self.voice,
            Step::Claude => self.claude,
            Step::Login => self.login,
        }
    }
}

// MARK: open at login (HKCU\...\Run)

pub const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
/// Task Manager's Startup tab turns an entry off here without removing it.
pub const STARTUP_APPROVED_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
pub const RUN_VALUE: &str = "Context Capsule";

/// The Run value for this exe (`--login`: started at login, so it doesn't ask to install).
pub fn run_command(exe: &str) -> String {
    format!("\"{exe}\" {}", crate::install::LOGIN_ARG)
}

/// Does the Run value start this exe?
pub fn run_value_matches(value: &str, exe: &str) -> bool {
    let v = value.trim();
    let v = v.strip_prefix('"').and_then(|v| v.split_once('"')).map_or(v, |(path, _)| path);
    v.trim().eq_ignore_ascii_case(exe.trim())
}

/// StartupApproved data: none = on; else the first byte's lowest bit set = turned off.
pub fn startup_approved(data: Option<&[u8]>) -> bool {
    data.and_then(|d| d.first()).is_none_or(|b| b & 1 == 0)
}

#[cfg(windows)]
pub use win::{act, done, mark_seen, open_at_login_once, seen};

#[cfg(windows)]
mod win {
    use super::*;
    use crate::{connect, voice, win};
    use context_capsule_core::AppLog;

    /// Open at login by default, so people never forget to start the app — once only: if they turn it off
    /// (Task Manager → Startup apps, or Settings), it stays off.
    pub fn open_at_login_once(log: &AppLog) {
        if win::reg_dword(APP_KEY, LOGIN_SET_VALUE) == Some(1) {
            return;
        }
        let _ = win::set_reg_dword(APP_KEY, LOGIN_SET_VALUE, 1);
        let on = win::set_reg_string(RUN_KEY, RUN_VALUE, &run_command(&exe()));
        log.write(if on { "open at login: on (first launch)" } else { "open at login: cannot change" });
    }

    /// The window was shown once already (first launch shows it by itself, on Settings).
    pub fn seen() -> bool {
        win::reg_dword(APP_KEY, SEEN_VALUE) == Some(1)
    }

    pub fn mark_seen() {
        let _ = win::set_reg_dword(APP_KEY, SEEN_VALUE, 1);
    }

    fn exe() -> String {
        std::env::current_exe().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default()
    }

    pub fn done() -> Done {
        let login = win::reg_string(RUN_KEY, RUN_VALUE).is_some_and(|v| run_value_matches(&v, &exe()))
            && startup_approved(win::reg_binary(STARTUP_APPROVED_KEY, RUN_VALUE).as_deref());
        Done { voice: !voice::mic_blocked(), claude: connect::is_connected(), login }
    }

    /// A step's button. May show a message box (its own modal loop): call it with nothing borrowed.
    pub fn act(step: Step, log: &AppLog) {
        match step {
            Step::Voice => {
                log.write("setup: opening microphone settings");
                win::open(voice::MIC_SETTINGS_URI);
            }
            Step::Claude => connect::connect(log),
            Step::Login => {
                let on = win::set_reg_string(RUN_KEY, RUN_VALUE, &run_command(&exe()));
                win::delete_reg_value(STARTUP_APPROVED_KEY, RUN_VALUE); // undo "Disabled" in Task Manager
                log.write(if on { "open at login: on" } else { "open at login: cannot change" });
                if !on {
                    win::alert(None, "Open at login", "Windows didn't allow this change. Try again, or start Context Capsule yourself.");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_and_words() {
        let labels: Vec<_> = Step::ALL.iter().map(|s| (s.row().title, s.row().button)).collect();
        assert_eq!(labels, [("Voice notes", "Allow"), ("Connect your AI", "Connect"), ("Open at login", "Turn On")]);
        let d = Done { voice: true, claude: false, login: true };
        assert!(d.get(Step::Voice) && !d.get(Step::Claude) && d.get(Step::Login));
    }

    #[test]
    fn run_at_login_values() {
        let exe = r"C:\Users\s\Desktop\kit\context-capsule.exe";
        assert_eq!(run_command(exe), format!("\"{exe}\" --login"));
        assert!(run_value_matches(&run_command(exe), exe));
        assert!(run_value_matches(r#""C:\USERS\S\Desktop\kit\context-capsule.exe" --quiet"#, exe), "case and arguments");
        assert!(run_value_matches(exe, exe), "no quotes");
        assert!(!run_value_matches(r#""C:\old\context-capsule.exe""#, exe), "another copy");
        assert!(startup_approved(None), "never touched: on");
        assert!(startup_approved(Some(&[2, 0, 0, 0])));
        assert!(!startup_approved(Some(&[3, 0, 0, 0])), "turned off in Task Manager");
        assert!(startup_approved(Some(&[6])) && !startup_approved(Some(&[7])));
    }
}
