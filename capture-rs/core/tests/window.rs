// context-capsule — core: golden vectors for the Context Capsule window, ported from capture/tests/main.swift
// ("capsule files", "settings written by the app", "time per project"): same inputs, same answers as the Mac.

use context_capsule_core::*;
use std::path::PathBuf;
use std::time::{Duration, UNIX_EPOCH};

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

#[test]
fn time_per_project() {
    let root = tmp("stat-sessions");
    let dir = root.join("20261019-100000");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("events.jsonl"),
        r#"{"t":"2026-10-19T10:00:00Z","end":"2026-10-19T10:30:00Z","app":"Code","bundle":"c","title":"x","project":"web","project_by":"auto","src":"ax","text":"a"}
{"t":"2026-10-19T10:30:00Z","end":"2026-10-19T10:30:00Z","app":"Code","bundle":"c","title":"x","project":"web","project_by":"auto","src":"ax","text":"b"}
{"t":"2026-10-19T10:31:00Z","end":"2026-10-19T10:41:00Z","app":"Slack","bundle":"s","title":"y","src":"ax","text":"c"}
{"t":"2026-10-19T10:35:00Z","end":"2026-10-19T10:45:00Z","app":"Safari","bundle":"f","title":"z","focus":"pointer","src":"ax","text":"d"}
{"t":"2026-10-19T10:40:00Z","end":"2026-10-19T10:41:00Z","kind":"voice","src":"voice","app":"Code","bundle":"c","title":"x","text":"why"}
{"t":"2026-10-20T09:00:00Z","end":"2026-10-20T09:10:00Z","app":"Code","bundle":"c","title":"x","project":"web","project_by":"auto","src":"ax","text":"e"}"#,
    )
    .unwrap();
    let day_start = UNIX_EPOCH + Duration::from_secs(1_792_368_000); // 2026-10-19T00:00:00Z
    let day_end = day_start + Duration::from_secs(86_400);
    let ids = |t: &Totals| t.by_project.iter().map(|(id, _)| id.clone()).collect::<Vec<_>>();
    let stats = WorkStats::totals(&root, day_start, day_end, &[]);
    assert_eq!(ids(&stats), ["web", ""], "time per project: notes and looked-at windows don't count");
    assert_eq!((stats.by_project[0].1, stats.by_project[1].1), (1803.0, 600.0), "a moment counts at least 3 s");
    assert_eq!(stats.seconds, 2403.0, "total time");
    std::fs::write(dir.join("assign.json"), r#"{"assign":[{"project":"chat","from":"2026-10-19T10:31:00Z","to":"2026-10-19T10:32:00Z"}]}"#)
        .unwrap();
    let sorted = WorkStats::totals(&root, day_start, day_end, &[]);
    assert_eq!(ids(&sorted), ["web", "chat"], "Claude's corrections count");
    std::fs::remove_file(dir.join("assign.json")).unwrap();
    let ids_of = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let oct20 = chrono::NaiveDate::from_ymd_opt(2026, 10, 20).unwrap();
    assert_eq!(WorkStats::streak(&ids_of(&["20261018-090000", "20261019-100000", "20261020-080000", "20261020-120000"]), oct20), 3, "streak: three days in a row");
    assert_eq!(WorkStats::streak(&ids_of(&["20261018-090000", "20261019-100000"]), oct20), 2, "streak: nothing yet today still counts up to yesterday");
    assert_eq!(WorkStats::streak(&ids_of(&["20261017-090000", "20261020-100000"]), oct20), 1, "streak: a gap breaks it");
    assert_eq!(WorkStats::streak(&ids_of(&["20261015-090000", "x"]), oct20), 0, "streak: none");
    let team = Project { id: "team".into(), name: "Team".into(), clues: vec!["Slack".into()] };
    let by_clue = WorkStats::totals(&root, day_start, day_end, &[team]);
    assert_eq!(ids(&by_clue), ["web", "team"], "clues made later count too");
    assert!(
        WorkStats::duration(40.0) == "under 1 min"
            && WorkStats::duration(0.0) == "under 1 min"
            && WorkStats::duration(720.0) == "12 min"
            && WorkStats::duration(3900.0) == "1 h 5 min"
            && WorkStats::duration(7200.0) == "2 h",
        "durations"
    );
    assert!(
        WorkStats::greeting(9) == "Good morning" && WorkStats::greeting(14) == "Good afternoon" && WorkStats::greeting(20) == "Good evening",
        "greetings"
    );
    let _ = std::fs::remove_dir_all(&root);
}
