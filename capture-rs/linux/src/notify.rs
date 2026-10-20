// context-capsule — linux: messages for the user — toasts through org.freedesktop.Notifications (no
// permission needed), alerts, yes/no questions and a line of text through zenity or kdialog when installed.

use std::collections::HashMap;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use tokio::runtime::Handle;
use zbus::zvariant::Value;

const APP: &str = "Context Capsule";
const ICON: &str = "media-record";

#[derive(Clone)]
pub struct Notifier {
    rt: Handle,
    /// The last toast's id, so a new one replaces it instead of piling up.
    last: Arc<Mutex<u32>>,
}

/// Which dialog tool is installed, if any.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dialogs {
    Zenity,
    Kdialog,
}

impl Dialogs {
    pub fn find() -> Option<Dialogs> {
        if on_path("zenity") {
            Some(Dialogs::Zenity)
        } else if on_path("kdialog") {
            Some(Dialogs::Kdialog)
        } else {
            None
        }
    }

    pub fn alert_args(self, title: &str, body: &str) -> Vec<String> {
        match self {
            Dialogs::Zenity => vec![
                "--info".into(),
                format!("--title={title}"),
                format!("--text={body}"),
                "--no-markup".into(),
                "--width=420".into(),
            ],
            Dialogs::Kdialog => vec!["--title".into(), title.into(), "--msgbox".into(), body.into()],
        }
    }

    pub fn question_args(self, title: &str, body: &str, yes: &str, no: &str) -> Vec<String> {
        match self {
            Dialogs::Zenity => vec![
                "--question".into(),
                format!("--title={title}"),
                format!("--text={body}"),
                format!("--ok-label={yes}"),
                format!("--cancel-label={no}"),
                "--no-markup".into(),
                "--width=420".into(),
            ],
            Dialogs::Kdialog => vec![
                "--title".into(),
                title.into(),
                "--yes-label".into(),
                yes.into(),
                "--no-label".into(),
                no.into(),
                "--yesno".into(),
                body.into(),
            ],
        }
    }

    /// Ask for a line of text, prefilled with `initial`. The answer comes on stdout.
    pub fn entry_args(self, title: &str, body: &str, initial: &str) -> Vec<String> {
        match self {
            Dialogs::Zenity => vec![
                "--entry".into(),
                format!("--title={title}"),
                format!("--text={}", markup_escape(body)), // entry labels take markup
                format!("--entry-text={initial}"),
                "--width=420".into(),
            ],
            Dialogs::Kdialog => vec!["--title".into(), title.into(), "--inputbox".into(), body.into(), initial.into()],
        }
    }

    fn program(self) -> &'static str {
        match self {
            Dialogs::Zenity => "zenity",
            Dialogs::Kdialog => "kdialog",
        }
    }
}

pub fn on_path(program: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(program).is_file()))
}

/// `<`, `>` and `&` as Pango markup entities.
pub fn markup_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// How long a toast stays: a saved note or a project change briefly, anything else long enough to
/// read (like the Mac app).
pub fn toast_ms(text: &str) -> i32 {
    if text == "Voice note saved" {
        2000
    } else if text.starts_with("Project: ") {
        3000
    } else {
        6000
    }
}

impl Notifier {
    pub fn new(rt: Handle) -> Notifier {
        Notifier { rt, last: Arc::new(Mutex::new(0)) }
    }

    /// A short message that fades by itself.
    pub fn toast(&self, text: &str) {
        let (last, text) = (self.last.clone(), text.to_string());
        self.rt.spawn(async move {
            let replaces = *last.lock().unwrap();
            match notify(&text, "", replaces, toast_ms(&text), 1, &[]).await {
                Ok(id) => *last.lock().unwrap() = id,
                Err(e) => eprintln!("Context Capsule: {text} (no notification: {e})"),
            }
        });
    }

    /// A message the user must dismiss: a dialog if we can, else a notification that stays.
    pub fn alert(&self, title: &str, body: &str) {
        if let Some(d) = Dialogs::find() {
            let spawned = Command::new(d.program())
                .args(d.alert_args(title, body))
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn();
            if let Ok(mut child) = spawned {
                std::thread::spawn(move || child.wait()); // reap it
                return;
            }
        }
        let (title, body) = (title.to_string(), body.to_string());
        self.rt.spawn(async move {
            if let Err(e) = notify(&title, &body, 0, 0, 2, &[]).await {
                eprintln!("Context Capsule: {title}: {body} (no notification: {e})");
            }
        });
    }

    /// Ask for a line of text (blocking: call off the engine's thread and off the tray's).
    /// Err: no dialog tool is installed. Ok(None): cancelled or left empty.
    pub fn ask_text(&self, title: &str, body: &str, initial: &str) -> Result<Option<String>, ()> {
        let d = Dialogs::find().ok_or(())?;
        let out = Command::new(d.program())
            .args(d.entry_args(title, body, initial))
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .map_err(|_| ())?;
        let typed = String::from_utf8_lossy(&out.stdout).trim().to_string();
        Ok((out.status.success() && !typed.is_empty()).then_some(typed))
    }

    /// Ask yes/no (blocking: call off the engine's thread). A dialog if we can, else a notification
    /// with two buttons. Anything but a clear yes is a no.
    pub fn ask(&self, title: &str, body: &str, yes: &str, no: &str) -> bool {
        if let Some(d) = Dialogs::find() {
            if let Ok(status) = Command::new(d.program())
                .args(d.question_args(title, body, yes, no))
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
            {
                return status.success();
            }
        }
        let (title, body, yes, no) = (title.to_string(), body.to_string(), yes.to_string(), no.to_string());
        self.rt.block_on(async move { ask_by_notification(&title, &body, &yes, &no).await.unwrap_or(false) })
    }
}

async fn proxy(bus: &zbus::Connection) -> zbus::Result<zbus::Proxy<'static>> {
    zbus::Proxy::new(
        bus,
        "org.freedesktop.Notifications",
        "/org/freedesktop/Notifications",
        "org.freedesktop.Notifications",
    )
    .await
}

/// org.freedesktop.Notifications.Notify. `urgency` 0 low, 1 normal, 2 critical; `timeout` 0 = stays.
async fn notify(
    summary: &str,
    body: &str,
    replaces: u32,
    timeout: i32,
    urgency: u8,
    actions: &[&str],
) -> zbus::Result<u32> {
    let bus = zbus::Connection::session().await?;
    notify_on(&bus, summary, body, replaces, timeout, urgency, actions).await
}

async fn notify_on(
    bus: &zbus::Connection,
    summary: &str,
    body: &str,
    replaces: u32,
    timeout: i32,
    urgency: u8,
    actions: &[&str],
) -> zbus::Result<u32> {
    let p = proxy(bus).await?;
    let mut hints: HashMap<&str, Value<'_>> = HashMap::new();
    hints.insert("urgency", Value::U8(urgency));
    hints.insert("desktop-entry", Value::from(crate::APP_ID));
    p.call("Notify", &(APP, replaces, ICON, summary, body, actions.to_vec(), hints, timeout)).await
}

async fn ask_by_notification(title: &str, body: &str, yes: &str, no: &str) -> zbus::Result<bool> {
    use futures_util::StreamExt;
    let bus = zbus::Connection::session().await?;
    let p = proxy(&bus).await?;
    let mut invoked = p.receive_signal("ActionInvoked").await?;
    let mut closed = p.receive_signal("NotificationClosed").await?;
    let id = notify_on(&bus, title, body, 0, 0, 2, &["yes", yes, "no", no]).await?;
    let answer = async {
        loop {
            tokio::select! {
                Some(m) = invoked.next() => {
                    if let Ok((nid, action)) = m.body().deserialize::<(u32, String)>() {
                        if nid == id { return action == "yes"; }
                    }
                }
                Some(m) = closed.next() => {
                    if let Ok((nid, _)) = m.body().deserialize::<(u32, u32)>() {
                        if nid == id { return false; }
                    }
                }
                else => return false,
            }
        }
    };
    Ok(tokio::time::timeout(std::time::Duration::from_secs(600), answer).await.unwrap_or(false))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dialog_arguments() {
        let z = Dialogs::Zenity.question_args("T", "Body", "Download", "Not now");
        assert!(z.contains(&"--question".to_string()) && z.contains(&"--ok-label=Download".to_string()));
        let k = Dialogs::Kdialog.alert_args("T", "Body");
        assert_eq!(k, vec!["--title", "T", "--msgbox", "Body"]);
        assert_eq!(toast_ms("Voice note saved"), 2000);
        assert_eq!(toast_ms("Didn't catch that."), 6000);
        assert_eq!(toast_ms("Project: Client report"), 3000);
        let z = Dialogs::Zenity.entry_args("New project", "Windows with “a&b” count", "a&b");
        assert_eq!(z, vec!["--entry", "--title=New project", "--text=Windows with “a&amp;b” count", "--entry-text=a&b", "--width=420"]);
        let k = Dialogs::Kdialog.entry_args("New project", "Name it.", "");
        assert_eq!(k, vec!["--title", "New project", "--inputbox", "Name it.", ""]);
        assert_eq!(markup_escape("<b>&"), "&lt;b&gt;&amp;");
    }
}
