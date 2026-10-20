// context-capsule — windows: entry point — the tray app on the UI thread, the core engine on its own thread.
//! Pure rules (picked-item matching, UIA roles, window classes, resampling, model checks, menu text,
//! the setup window's layout, Claude detection) build and test on any OS; everything that calls Windows is behind `cfg(windows)`.

#![cfg_attr(windows, windows_subsystem = "windows")]
#![cfg_attr(not(windows), allow(dead_code))]

mod connect;
mod hotkeys;
mod input;
// Only used by voice notes.
#[cfg_attr(not(feature = "voice"), allow(dead_code, unused_imports))]
mod model;
mod ocr;
mod picker;
mod pointer;
mod reminder;
mod setup;
mod tray;
mod toast;
mod uia;
#[cfg_attr(not(feature = "voice"), allow(dead_code))]
mod voice;
mod windows_list;

#[cfg(windows)]
mod app;
#[cfg(windows)]
mod changes;
#[cfg(windows)]
mod platform;
#[cfg(windows)]
mod win;

#[cfg(windows)]
fn main() {
    app::run();
}

#[cfg(not(windows))]
fn main() {
    eprintln!("context-capsule-windows only runs on Windows (macOS: capture/, Linux: capture-rs/linux/).");
    std::process::exit(1);
}
