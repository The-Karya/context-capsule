// context-capsule — linux: the Linux recorder — tray, hotkeys and portal on a small async runtime; the
// shared core engine on its own thread, fed through one message channel; the window (GTK) on the main
// thread. Writes the same files as macOS.

mod ai;
mod apps;
mod appimage;
mod autostart;
mod atspi_front;
mod atspi_text;
mod connect;
mod desktop;
mod hotkeys;
#[cfg(any(feature = "webview", test))]
#[cfg_attr(not(feature = "webview"), allow(dead_code))]
mod icon_match;
mod net;
mod link;
#[cfg(feature = "voice")]
mod model;
mod notify;
mod ocr;
mod pipewire;
mod platform;
mod portal;
mod probe;
mod program;
mod reminder;
mod tray;
mod update;
#[cfg(any(feature = "window", test))]
mod ui;
mod voice;
#[cfg(feature = "window")]
mod window;
mod x11;

use context_capsule_core::{claude, Engine, Hotkeys, Msg, Options, Paths, PlatformText, VoiceEngine};
use desktop::{Overview, SessionKind};
use link::WindowLink;
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

/// The locale's clock (LC_TIME, else LANG): its time format (`nl_langinfo(T_FMT)`, e.g. en_US "%r", en_GB "%T")
/// says 12- or 24-hour; the window writes times and dates to match (`formats`). Call first, before any thread.
fn set_time_formats() {
    // SAFETY: setlocale with an empty, NUL-terminated string (the environment's locale) for LC_TIME only, before
    // other threads exist; nl_langinfo returns a NUL-terminated string valid until the next locale call, copied
    // out at once.
    let t_fmt = unsafe {
        libc::setlocale(libc::LC_TIME, c"".as_ptr());
        let p = libc::nl_langinfo(libc::T_FMT);
        if p.is_null() {
            String::new()
        } else {
            std::ffi::CStr::from_ptr(p).to_string_lossy().into_owned()
        }
    };
    let twelve = context_capsule_core::formats::c_format_is_12_hour(&t_fmt);
    context_capsule_core::formats::set(if twelve { context_capsule_core::formats::Formats::H12 } else { context_capsule_core::formats::Formats::H24 });
}

fn main() {
    set_time_formats();
    let paths = Paths::standard();
    let env = |k: &str| std::env::var(k).ok();
    let args: Vec<String> = std::env::args().collect();
    let session = SessionKind::detect(&env);
    let locale = os_locale(&env);
    // Your own shortcuts (config.json "hotkeys"): every place that names them reads these.
    let keys = Hotkeys::load(&paths.config());
    hotkeys::set_current(keys);
    // Your AI (config.json "ai"); the AI apps are read on a thread, which also finds the context-capsule-mcp
    // program (from an AppImage: copied to its stable place now, so connected apps keep working).
    ai::init(paths.config());
    std::thread::spawn(|| {
        ai::refresh();
    });
    #[cfg(feature = "window")]
    let web = ui::new_window(&args, &std::fs::read(paths.config()).unwrap_or_default());
    #[cfg(feature = "window")]
    if let Some(section) = ui::window_only_arg(&args) {
        std::process::exit(window::run_alone(paths, section, locale, web));
    }
    let probe = args.iter().any(|a| a == "--probe");
    // Opened with a capsule someone sent (Open with → Context Capsule: `%f` in the desktop entry, or `--open`).
    let open = claude::open_arg(&args, &std::env::current_dir().unwrap_or_default());

    // The window's GTK lives on this (main) thread. Start it first: if Context Capsule is already running,
    // this only opens that one's window (or hands it the capsule).
    #[cfg(feature = "window")]
    let gtk = if probe { None } else { window::Gtk::start(open.as_deref()) };
    #[cfg(feature = "window")]
    if gtk.as_ref().is_some_and(window::Gtk::is_remote) {
        eprintln!("Context Capsule is already running: {}", if open.is_some() { "it opens the capsule" } else { "opened its window" });
        return;
    }
    #[cfg(feature = "window")]
    let link = gtk.as_ref().map_or_else(WindowLink::off, window::Gtk::link);
    #[cfg(not(feature = "window"))]
    let link = WindowLink::off();
    // With the window: it opens the capsule once it's ready. Without: below, once notifications work.
    let open_later = match open {
        Some(path) if link.is_on() => {
            link.open_capsule(path);
            None
        }
        other => other,
    };

    if !probe {
        autostart::refresh(&env); // an entry from 0.2.0: today's name and icon
        if let Some(line) = autostart::install_once(&paths.root, &env) {
            eprintln!("Context Capsule: {line}");
        }
    }
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
    let scrolled = Arc::new(Mutex::new(None));
    let notifier = notify::Notifier::new(handle.clone());
    let tray = if probe {
        tray::Tray::none(&handle)
    } else {
        tray::Tray::spawn(&handle, tx.clone(), notifier.clone(), link.clone())
    };
    if let Some(path) = open_later {
        open_without_window(&path, &notifier);
    }
    if !probe && !tray.is_shown() {
        notifier.toast(&no_tray_message(&keys));
    }
    // The window opens by itself on the first launch (on Settings), and when there is no tray to open it from.
    #[cfg(feature = "window")]
    let open_at_start = match (link.is_on(), ui::first_launch(&paths.root), tray.is_shown()) {
        (false, _, _) => None,
        (true, true, _) => Some(context_capsule_core::window::Section::Settings),
        (true, false, false) => Some(context_capsule_core::window::Section::Home),
        (true, false, true) => None,
    };
    #[cfg(feature = "window")]
    let ui_paths = paths.clone();
    let control = if probe {
        hotkeys::Control::none()
    } else {
        let poke = tray.poke();
        hotkeys::start(&handle, session, keys, tx.clone(), changed.clone(), scrolled.clone(), notifier.clone(), poke)
    };
    if !probe {
        update::start(&paths, notifier.clone(), tx.clone());
        let (root, n) = (paths.root.clone(), notifier.clone());
        std::thread::spawn(move || appimage::offer_once(&root, &n));
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
            scrolled,
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
        Options { own_app_id: Some("context-capsule".into()), text: PlatformText::LINUX.with_shortcuts(&keys), locale: locale.clone() };
    let wake = tx.clone();
    let root = paths.root.clone();
    let engine_rt = handle.clone();
    let engine_done = link.clone();
    #[cfg(feature = "window")]
    let ui_locale = locale.clone();
    let engine = std::thread::Builder::new().name("engine".into()).spawn(move || {
        let voice: Box<dyn VoiceEngine> = make_voice(root, notifier.clone());
        let config = paths.config();
        let platform = platform::LinuxPlatform::new(
            engine_rt, session, capture, a11y, x11, overview, changed, scrolled, notifier, tray, &locale, config,
        );
        let mut engine = Engine::new(paths, platform, voice, opts);
        engine.set_voice_wake(Arc::new(move || {
            let _ = wake.send(Msg::VoiceReady);
        }));
        engine.run(rx);
        engine.platform().tray.shutdown();
        engine_done.quit(); // the window's loop returns
    });
    // The window: GTK's loop runs here until the engine stops (Quit, Ctrl+C). Closing the window only hides it.
    #[cfg(feature = "window")]
    if let (Some(gtk), true) = (gtk, engine.is_ok()) {
        gtk.run(window::Setup { paths: ui_paths, tx: tx.clone(), open: open_at_start, locale: ui_locale, hotkeys: control, web });
    }
    #[cfg(not(feature = "window"))]
    drop(control);
    drop(tx);
    match engine {
        Ok(t) => {
            let _ = t.join();
        }
        Err(e) => eprintln!("Context Capsule: cannot start the engine: {e}"),
    }
    rt.shutdown_timeout(Duration::from_secs(1));
    update::relaunch_if_pending();
}

/// A capsule someone sent, with no window (no display, or built without it): your AI carries on from it (on a
/// thread: it may ask); else a notification says what's wrong.
fn open_without_window(path: &std::path::Path, notifier: &notify::Notifier) {
    match claude::open_file(path) {
        claude::Opened::Capsule { prompt } => {
            let n = notifier.clone();
            std::thread::spawn(move || {
                ai::ensure_loaded();
                ai::open_alone(&n, &prompt, ai::WHY_CAPSULE, ai::AGAIN_CAPSULE);
            });
        }
        claude::Opened::NotACapsule => notifier.toast(claude::NOT_A_CAPSULE_BODY),
        claude::Opened::Unreadable => notifier.toast(claude::CANT_READ),
    }
}

/// Said once when the desktop shows no tray icon.
fn no_tray_message(keys: &Hotkeys) -> String {
    format!(
        "Context Capsule is running, but this desktop shows no tray icon (on GNOME, install the AppIndicator extension). Use the hotkeys: {} to start contextualizing.",
        keys.record
    )
}

#[cfg(feature = "voice")]
fn make_voice(root: std::path::PathBuf, notifier: notify::Notifier) -> Box<dyn VoiceEngine> {
    Box::new(voice::WhisperVoice::new(root, notifier))
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

    #[test]
    fn no_tray_names_the_current_shortcut() {
        assert!(no_tray_message(&Hotkeys::DEFAULT).ends_with("Use the hotkeys: Alt+C to start contextualizing."));
        let mine = Hotkeys { record: context_capsule_core::Shortcut::parse("Ctrl+F2").unwrap(), ..Hotkeys::DEFAULT };
        assert!(no_tray_message(&mine).contains("Ctrl+F2 to start"));
    }
}
