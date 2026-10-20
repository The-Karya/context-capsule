// context-capsule — linux: the Linux recorder — tray, hotkeys and portal on a small async runtime; the
// shared core engine on its own thread, fed through one message channel. Writes the same files as macOS.

mod apps;
mod atspi_front;
mod atspi_text;
mod desktop;
mod hotkeys;
#[cfg(feature = "voice")]
mod model;
mod notify;
mod ocr;
mod pipewire;
mod platform;
mod portal;
mod probe;
mod reminder;
mod tray;
mod voice;
mod x11;

use context_capsule_core::{Engine, Msg, Options, Paths, PlatformText, VoiceEngine};
use desktop::{Overview, SessionKind};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

/// Our app id: the .desktop file name, and what portals know us by.
pub const APP_ID: &str = "com.thekarya.contextcapsule";

/// The user's locale from the environment, as a BCP 47 tag: "de_DE.UTF-8" → "de-DE". Default en-US.
pub fn os_locale(env: &dyn Fn(&str) -> Option<String>) -> String {
    let raw = ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|k| env(k))
        .find(|v| !v.trim().is_empty())
        .unwrap_or_default();
    let tag = raw.split(['.', '@']).next().unwrap_or("").trim().replace('_', "-");
    if tag.is_empty() || tag == "C" || tag == "POSIX" {
        "en-US".into()
    } else {
        tag
    }
}

fn main() {
    let paths = Paths::standard();
    let env = |k: &str| std::env::var(k).ok();
    let session = SessionKind::detect(&env);
    let locale = os_locale(&env);
    let probe = std::env::args().any(|a| a == "--probe");
    let rt = match tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("Context Capsule: cannot start: {e}");
            std::process::exit(1);
        }
    };
    let handle = rt.handle().clone();
    eprintln!("Context Capsule: {session:?} session, data in {}", paths.root.display());

    // Portals know host apps by the id registered here (before any other portal call).
    rt.block_on(async {
        if let Ok(id) = ashpd::AppID::try_from(APP_ID) {
            let _ = tokio::time::timeout(Duration::from_secs(2), ashpd::register_host_app(id)).await;
        }
    });

    let (tx, rx) = mpsc::channel::<Msg>();
    let changed = Arc::new(Mutex::new(None));
    let notifier = notify::Notifier::new(handle.clone());
    let tray = if probe { tray::Tray::none(&handle) } else { tray::Tray::spawn(&handle, tx.clone(), notifier.clone(), paths.sessions()) };
    if !probe && !tray.is_shown() {
        notifier.toast("Context Capsule is running, but this desktop shows no tray icon (on GNOME, install the AppIndicator extension). Use the hotkeys: Ctrl+Alt+Shift+C to record.");
    }
    if !probe {
        hotkeys::start(&handle, session, tx.clone(), changed.clone());
    }

    let a11y = rt.block_on(async {
        match tokio::time::timeout(Duration::from_secs(3), atspi_front::Accessibility::connect(changed.clone())).await {
            Ok(Ok(a)) => Some(a),
            Ok(Err(e)) => {
                eprintln!("Context Capsule: no accessibility bus ({e}); window text comes from OCR only");
                None
            }
            Err(_) => {
                eprintln!("Context Capsule: the accessibility bus did not answer; window text comes from OCR only");
                None
            }
        }
    });
    let overview = rt.block_on(Overview::connect());
    // On X11 the window stack; on Wayland, Xwayland (if any) still lists the monitors.
    let x11 = match session {
        SessionKind::Unknown => None,
        _ => x11::Desktop::connect(),
    };
    if session == SessionKind::X11 && x11.is_none() {
        eprintln!("Context Capsule: cannot open the X display");
    }

    let capture = portal::Capture::new(handle.clone(), tx.clone());
    if probe {
        let platform = platform::LinuxPlatform::new(
            handle.clone(),
            session,
            capture,
            a11y,
            x11,
            overview,
            changed,
            notifier,
            tray,
            &locale,
            paths.config(),
        );
        let ok = probe::run(platform);
        rt.shutdown_timeout(Duration::from_secs(1));
        std::process::exit(if ok { 0 } else { 1 });
    }

    // Ctrl+C / SIGTERM: stop any session cleanly, like Quit.
    let quit = tx.clone();
    handle.spawn(async move {
        use tokio::signal::unix::{signal, SignalKind};
        let (Ok(mut int), Ok(mut term)) = (signal(SignalKind::interrupt()), signal(SignalKind::terminate())) else {
            return;
        };
        tokio::select! { _ = int.recv() => {}, _ = term.recv() => {} }
        let _ = quit.send(Msg::Quit);
    });

    let opts =
        Options { own_app_id: Some("context-capsule".into()), text: PlatformText::LINUX, locale: locale.clone() };
    let wake = tx.clone();
    let root = paths.root.clone();
    let engine_rt = handle.clone();
    let engine = std::thread::Builder::new().name("engine".into()).spawn(move || {
        let voice: Box<dyn VoiceEngine> = make_voice(root, notifier.clone());
        let config = paths.config();
        let platform = platform::LinuxPlatform::new(
            engine_rt, session, capture, a11y, x11, overview, changed, notifier, tray, &locale, config,
        );
        let mut engine = Engine::new(paths, platform, voice, opts);
        engine.set_voice_wake(Arc::new(move || {
            let _ = wake.send(Msg::VoiceReady);
        }));
        engine.run(rx);
        engine.platform().tray.shutdown();
    });
    drop(tx);
    match engine {
        Ok(t) => {
            let _ = t.join();
        }
        Err(e) => eprintln!("Context Capsule: cannot start the engine: {e}"),
    }
    rt.shutdown_timeout(Duration::from_secs(1));
}

#[cfg(feature = "voice")]
fn make_voice(root: std::path::PathBuf, notifier: notify::Notifier) -> Box<dyn VoiceEngine> {
    Box::new(voice::WhisperVoice::new(root, notifier, tray::VOICE_KEY))
}

#[cfg(not(feature = "voice"))]
fn make_voice(_root: std::path::PathBuf, _notifier: notify::Notifier) -> Box<dyn VoiceEngine> {
    Box::new(voice::NoVoice)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locale_from_env() {
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |k: &str| pairs.iter().find(|(n, _)| *n == k).map(|(_, v)| v.to_string())
        };
        assert_eq!(os_locale(&env(&[("LANG", "de_DE.UTF-8")])), "de-DE");
        assert_eq!(os_locale(&env(&[("LANG", "en_GB.UTF-8"), ("LC_ALL", "fr_FR@euro")])), "fr-FR");
        assert_eq!(os_locale(&env(&[("LANG", "C.UTF-8")])), "en-US");
        assert_eq!(os_locale(&env(&[("LC_ALL", ""), ("LANG", "pt_BR")])), "pt-BR");
        assert_eq!(os_locale(&env(&[])), "en-US");
    }
}
