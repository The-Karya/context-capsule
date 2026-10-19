// context-capsule — linux: `context-capsule --probe` — prints what the recorder would see right now (session,
// monitors, overview, front app and window, title, accessibility text, OCR languages) and exits. No capture.

use crate::platform::LinuxPlatform;
use context_capsule_core::{Exclusion, Platform, UiText};

pub fn run(mut p: LinuxPlatform) -> bool {
    println!("session: {:?}", p.session);
    println!("accessibility bus: {}", if p.a11y.is_some() { "connected" } else { "not available" });
    match &p.x11 {
        Some(x) => println!("monitors: {:?}", x.monitors()),
        None => println!("monitors: unknown (no X server / Xwayland)"),
    }
    let is_excluded = |id: Option<&str>, name: &str| Exclusion::is_excluded(id, Some(name), &[]);
    let Some(front) = p.front(&is_excluded) else {
        println!("front: none");
        return false;
    };
    println!("overview open: {}", front.overlay);
    println!("front app: {:?} id {:?} pid {}", front.app_name, front.app_id, front.pid);
    println!("private (never recorded): {}", is_excluded(front.app_id.as_deref(), &front.app_name));
    match &front.window {
        Some(w) => println!("front window: id {} frame {:?} name {:?}", w.id, w.frame, w.name),
        None => println!("front window: none"),
    }
    println!(
        "windows listed: {} ({} private, {} notifications)",
        front.list.len(),
        front.list.iter().filter(|w| w.excluded).count(),
        front.list.iter().filter(|w| w.notification).count()
    );
    println!("title: {:?}", p.ui_title(&front));
    if is_excluded(front.app_id.as_deref(), &front.app_name) {
        println!("text: not read (private app)");
        return true;
    }
    match p.ui_text(&front) {
        Some(t) => {
            let source = UiText::choose(true, &t);
            println!(
                "text: {} chars, {} letters/digits → would use {}",
                t.chars().count(),
                UiText::useful_count(&t),
                source.as_str()
            );
            println!("--- text ---\n{}\n---", t.chars().take(1500).collect::<String>());
        }
        None => println!("text: accessibility not available → OCR"),
    }
    println!("where: {:?}", p.location(&front));
    true
}
