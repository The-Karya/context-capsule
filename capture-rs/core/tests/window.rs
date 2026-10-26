// context-capsule — core: settings written by the app, ported from capture/tests/main.swift: same inputs, same
// answers as the Mac.

use context_capsule_core::*;
use std::path::PathBuf;

fn tmp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("capsule-window-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn settings_written_by_the_app() {
    let dir = tmp("settings");
    let set = dir.join("settings/config.json");
    ConfigFile::set(&set, "sounds", Some(false.into())).unwrap();
    ConfigFile::set(&set, "remind_at", Some("19:30".into())).unwrap();
    ConfigFile::set(&set, "exclude_apps", Some(serde_json::json!(["com.example.app"]))).unwrap();
    let cfg = Config::load(&set);
    assert!(!cfg.sounds && cfg.remind_at.as_deref() == Some("19:30") && cfg.exclude_apps == ["com.example.app"], "settings saved from the app");
    std::fs::write(&set, r#"{"author": "Saachi", "future_key": 7}"#).unwrap();
    ConfigFile::set(&set, "remind_at", Some(false.into())).unwrap();
    let text = std::fs::read_to_string(&set).unwrap();
    assert!(text.contains("\"author\": \"Saachi\"") && text.contains("future_key"), "other keys kept: {text}");
    assert_eq!(Config::load(&set).remind_at, None, "reminder turned off");
    let _ = std::fs::remove_dir_all(&dir);
}
