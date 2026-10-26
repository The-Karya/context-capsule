// context-capsule — core: golden vectors ported from capture/tests/main.swift ("capsule files", "settings written
// by the app"): same inputs, same answers as the Mac.

use context_capsule_core::*;
use std::path::PathBuf;

fn tmp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("capsule-window-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn capsule_files() {
    let cap_text = "---\ncapsule: 1\ntitle: \"Website — 2026-10-19\"\nauthor: Saachi            # from config\ncreated: 2026-10-19T18:00:00Z\nproject: website\nkind: daily\ndate: 2026-10-19\ndraft: true\n---\n\n## In short\nWe redesigned the pricing page.\nTeams is now $12.\n## For Claude\nYou're picking up…\n";
    let dir = tmp("capsules");
    let cap_path = dir.join("capsules/website-2026-10-19.capsule.md");
    let cs = CapsuleSummary::parse(cap_text, &cap_path);
    assert_eq!(
        (cs.title.as_str(), cs.project.as_str(), cs.kind.as_str(), cs.date.as_str()),
        ("Website — 2026-10-19", "website", "daily", "2026-10-19"),
        "capsule front matter"
    );
    assert!(cs.author == "Saachi" && cs.draft, "comments stripped, draft read");
    assert_eq!(cs.in_short, "We redesigned the pricing page. Teams is now $12.", "In short line");
    assert_eq!(CapsuleSummary::parse("no front matter", &cap_path).title, "website-2026-10-19", "title falls back to the file name");
    let folder = cap_path.parent().unwrap();
    std::fs::create_dir_all(folder.join(".history")).unwrap();
    std::fs::write(&cap_path, cap_text).unwrap();
    std::fs::write(folder.join("newer.capsule.md"), cap_text.replace("2026-10-19T18", "2026-10-20T09")).unwrap();
    std::fs::write(folder.join("notes.md"), "x").unwrap();
    let listed: Vec<String> =
        CapsuleSummary::all(folder).iter().map(|c| c.path.file_name().unwrap().to_string_lossy().into_owned()).collect();
    assert_eq!(listed, ["newer.capsule.md", "website-2026-10-19.capsule.md"], "capsules listed newest first");
    assert!(reminder::DAY_PROMPT.contains("Context Capsule"), "the request names the app");
    let _ = std::fs::remove_dir_all(&dir);
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
