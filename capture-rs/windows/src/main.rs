// context-capsule — windows: entry point — the tray app on the UI thread, the core engine on its own thread.
//! Pure rules (picked-item matching, UIA roles, window classes, resampling, model checks, menu text,
//! the window's layout, Claude detection) build and test on any OS; everything that calls Windows is behind `cfg(windows)`.

#![cfg_attr(windows, windows_subsystem = "windows")]
#![cfg_attr(not(windows), allow(dead_code))]

mod ai;
mod app_icon;
mod connect;
mod hotkeys;
mod input;
mod install;
mod layout;
// Only used by voice notes.
#[cfg_attr(not(feature = "voice"), allow(dead_code, unused_imports))]
mod model;
mod net;
mod ocr;
mod palette;
mod picker;
mod pointer;
mod program;
mod reminder;
// The exe's icon and version info: written by build.rs; here for its id and tests.
#[cfg_attr(not(test), allow(dead_code))]
mod res;
mod setup;
mod tray;
mod toast;
mod uia;
mod update;
#[cfg_attr(not(feature = "voice"), allow(dead_code))]
mod voice;
mod web_window;
mod windows_list;

#[cfg(windows)]
mod app;
#[cfg(windows)]
mod changes;
#[cfg(windows)]
mod main_window;
#[cfg(windows)]
mod paint;
#[cfg(windows)]
mod platform;
#[cfg(windows)]
mod win;

#[cfg(windows)]
fn main() {
    // Install / uninstall / finish an update first; false: this process is done.
    if install::startup() {
        app::run();
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("context-capsule-windows only runs on Windows (macOS: capture/, Linux: capture-rs/linux/).");
    std::process::exit(1);
}
