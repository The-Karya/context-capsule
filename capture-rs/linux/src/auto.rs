// context-capsule — linux: Automatic capsules (SPEC "Automatic capsules"; the decision and the run are core::auto).
// Here: the program (the any-AI one), whether a run's process is still there (/proc), the window's `auto` and
// `data-changed` events, and the notification when a run ends: "3 capsules ready" (click: the Capsules page), or
// "Couldn't make today's capsules" with the reason and **Try again** (a run by click). The engine checks once a
// minute; after a suspend the next minute's check catches up (it compares the wall clock).

use crate::link::WindowLink;
use crate::notify::Notifier;
use context_capsule_core::anyai::Launch;
use context_capsule_core::auto::{self, Auto, AutoHost, By, MakeResult};
use context_capsule_core::window::Section;
use context_capsule_core::{AppLog, Paths};
use serde_json::{json, Value};
use std::sync::{Arc, OnceLock};

static AUTO: OnceLock<Arc<Auto>> = OnceLock::new();

/// The app's Automatic capsules (None before `start`).
pub fn get() -> Option<Arc<Auto>> {
    AUTO.get().cloned()
}

/// At launch (the engine's first check is right away).
pub fn start(paths: Paths, notifier: Notifier, link: WindowLink) -> Arc<Auto> {
    let log = AppLog::new(paths.log());
    AUTO.get_or_init(|| Auto::new(paths, log.clone(), Box::new(LinuxAuto { notifier, link, log }))).clone()
}

/// The notification's actions: clicking it opens the Capsules page (when there is a window); a failure also offers
/// Try again.
pub fn actions(failed: bool, window: bool) -> Vec<(String, String)> {
    let mut out = vec![];
    if window {
        out.push(("default".to_string(), "Open".to_string()));
    }
    if failed {
        out.push(("retry".to_string(), "Try again".to_string()));
    }
    out
}

struct LinuxAuto {
    notifier: Notifier,
    link: WindowLink,
    log: AppLog,
}

impl AutoHost for LinuxAuto {
    fn launch(&self) -> Option<Launch> {
        crate::ai::launch()
    }

    fn alive(&self, pid: u32) -> bool {
        auto::proc_alive(pid)
    }

    fn changed(&self, status: Value) {
        self.link.event("auto", &status);
    }

    fn finished(&self, by: By, result: &MakeResult) {
        self.link.event("data-changed", &json!({ "what": ["capsules"] }));
        let Some((title, body)) = auto::notification(result) else {
            if by == By::Click {
                self.log.write(&format!("capsules by click: not made ({})", result.status));
            }
            return;
        };
        let (link, log) = (self.link.clone(), self.log.clone());
        let acts = actions(result.status == "failed", link.is_on());
        self.notifier.notify_actions(&title, &body, acts, move |action| match action.as_str() {
            "retry" => {
                if let Some(auto) = get() {
                    if let Err(why) = auto.run_click() {
                        log.write(&format!("capsules: try again: {why}"));
                    }
                }
            }
            _ => link.open(Some(Section::Capsules)),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notification_actions() {
        let keys = |a: Vec<(String, String)>| a.into_iter().map(|(k, _)| k).collect::<Vec<_>>();
        assert_eq!(keys(actions(false, true)), ["default"]);
        assert_eq!(keys(actions(true, true)), ["default", "retry"]);
        assert_eq!(keys(actions(true, false)), ["retry"], "no window: Try again only");
        assert!(actions(false, false).is_empty());
    }
}
