// context-capsule — core: the words and numbers of the Context Capsule window (Home, Capsules, Projects, Settings,
// Help), the same on Windows and Linux as on the Mac (capture/Sources/MainWindow.swift). The platforms only draw.
//! Plain, short words. Raw recordings show only on the Timeline (`timeline`), on this computer; everywhere else
//! the window shows time per project, counts and capsules.

use crate::anyai::YourAi;
use crate::capsules::{project_name, CapsuleSummary};
use crate::look;
use crate::platform::Status;
use crate::projects::Project;
use crate::settings::ShareScreens;
use crate::shortcuts::Hotkeys;
use crate::stats::{Totals, WorkStats};
use crate::timeline::Colour;
use std::time::SystemTime;

/// The app's name where people see it as a name: window title, taskbar, Start menu, Settings → Apps, dialog titles,
/// the tray's tooltip, notifications. In sentences it stays "Context Capsule".
pub const APP_NAME: &str = "Karya Context Capsule";
/// The tray / menu item that opens the window.
pub const OPEN_WINDOW: &str = "Open Context Capsule";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Section {
    Home,
    Timeline,
    Capsules,
    Projects,
    Settings,
    Help,
}

impl Section {
    pub const ALL: [Section; 6] = [Section::Home, Section::Timeline, Section::Capsules, Section::Projects, Section::Settings, Section::Help];

    pub fn title(self) -> &'static str {
        match self {
            Section::Home => "Home",
            Section::Timeline => crate::timeline::TIMELINE,
            Section::Capsules => "Capsules",
            Section::Projects => "Projects",
            Section::Settings => "Settings",
            Section::Help => "Help",
        }
    }

    /// The sidebar group it sits in.
    pub fn group(self) -> &'static str {
        match self {
            Section::Home | Section::Timeline | Section::Capsules | Section::Projects => GROUP_WORK,
            Section::Settings | Section::Help => GROUP_APP,
        }
    }

    /// A short id ("home"), for settings and logs.
    pub fn id(self) -> &'static str {
        match self {
            Section::Home => "home",
            Section::Timeline => "timeline",
            Section::Capsules => "capsules",
            Section::Projects => "projects",
            Section::Settings => "settings",
            Section::Help => "help",
        }
    }
}

/// The sidebar's two groups.
pub const GROUP_WORK: &str = "Your work";
pub const GROUP_APP: &str = "Karya Context Capsule";

/// Window sizes, in device-independent pixels: content is centered and never wider than this.
pub const MAX_CONTENT: f64 = 760.0;
pub const MAX_CONTENT_WIDE: f64 = 980.0;
/// From this content width up, Home has a side column (Shortcuts, Working on, Make) next to the main one.
pub const WIDE_FROM: f64 = 860.0;
/// The side column's width, when wide.
pub const SIDE_WIDTH: f64 = 280.0;
pub const MIN_WIDTH: f64 = 640.0;
pub const MIN_HEIGHT: f64 = 480.0;
pub const START_WIDTH: f64 = 900.0;
pub const START_HEIGHT: f64 = 640.0;
/// Bars shown per block.
pub const MAX_BARS: usize = 5;

/// How wide the content column is in an area `available` wide, and whether it's wide (two columns).
pub fn content_width(available: f64) -> (f64, bool) {
    let wide = available > WIDE_FROM;
    let max = if wide { MAX_CONTENT_WIDE } else { MAX_CONTENT };
    (available.min(max).max(0.0), wide)
}

/// Keys and words that differ per platform. `record` / `voice`: the current shortcuts ("Alt+C").
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Keys {
    pub record: String,
    pub voice: String,
    /// "Ctrl+V, then Enter"
    pub paste: &'static str,
    /// "Show in folder" (Windows: File Explorer)
    pub show_in_folder: &'static str,
    /// "this PC" / "this computer"
    pub this_computer: &'static str,
}

pub const SHOW_IN_FOLDER: &str = "Show in folder";

impl Keys {
    /// Windows, with the current shortcuts.
    pub fn windows(h: &Hotkeys) -> Keys {
        Keys { record: h.record.label(), voice: h.voice.label(), paste: "Ctrl+V, then Enter", show_in_folder: SHOW_IN_FOLDER, this_computer: "this PC" }
    }

    /// Linux, with the current shortcuts.
    pub fn linux(h: &Hotkeys) -> Keys {
        Keys { record: h.record.label(), voice: h.voice.label(), paste: "Ctrl+V, then Enter", show_in_folder: SHOW_IN_FOLDER, this_computer: "this computer" }
    }
}

// MARK: Home

/// "Good evening, Saachi." (or just "Good evening.").
pub fn greeting(hour: u32, first_name: &str) -> String {
    let hello = WorkStats::greeting(hour);
    if first_name.trim().is_empty() {
        format!("{hello}.")
    } else {
        format!("{hello}, {}.", first_name.trim())
    }
}

/// The small pill top right: "● 0:12" while contextualizing, else "Today 2 min".
pub fn status_pill(status: &Status, now: SystemTime, today_seconds: f64) -> String {
    match look::elapsed(status, now) {
        Some(time) => format!("● {time}"),
        None => format!("Today {}", WorkStats::duration(today_seconds)),
    }
}

/// One big number in the stat strip.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stat {
    pub label: &'static str,
    pub value: String,
    pub detail: String,
}

/// Today (time, "contextualized"), This week (time, "N capsules"), Streak ("N days", "in a row").
pub fn stats(n: &HomeNumbers) -> [Stat; 3] {
    let plural = |k: usize, one: &str, many: &str| format!("{k} {}", if k == 1 { one } else { many });
    [
        Stat { label: "Today", value: WorkStats::duration(n.today.seconds), detail: "contextualized".into() },
        Stat { label: "This week", value: WorkStats::duration(n.week.seconds), detail: plural(n.week_capsules, "capsule", "capsules") },
        Stat { label: "Streak", value: plural(n.streak as usize, "day", "days"), detail: "in a row".into() },
    ]
}

/// The status card's big line: "Not contextualizing", "Pick a window or screen", "Contextualizing  0:12".
pub fn status_title(status: &Status, now: SystemTime) -> String {
    if status.recording {
        format!("Contextualizing  {}", look::elapsed(status, now).unwrap_or_else(|| "0:00".into()))
    } else if status.picking {
        "Pick a window or screen".into()
    } else {
        "Not contextualizing".into()
    }
}

/// The status card's main button.
pub fn toggle_button(status: &Status) -> &'static str {
    if status.recording {
        "Stop"
    } else if status.picking {
        "Cancel"
    } else {
        "Start contextualizing"
    }
}

/// Under the big line: "12 moments saved · 2 screens shared" while recording; else how to start, following
/// Settings → When you start.
pub fn status_detail(status: &Status, keys: &Keys, share: ShareScreens) -> String {
    if status.recording {
        let n = status.events;
        let screens = if status.sources > 1 { format!(" · {} screens shared", status.sources) } else { String::new() };
        format!("{n} moment{} saved{screens}", if n == 1 { "" } else { "s" })
    } else {
        format!("{} to start. {}", keys.record, idle_line(share))
    }
}

/// The status card's line while not contextualizing (the Mac's, word for word).
pub fn idle_line(share: ShareScreens) -> &'static str {
    match share {
        ShareScreens::Pick => "Pick the screen or window you work on. Only that is read.",
        ShareScreens::All => "Shares all your screens. Private apps are never read.",
    }
}

pub fn voice_button(status: &Status) -> &'static str {
    if status.listening {
        "Done speaking"
    } else {
        "Speak a note"
    }
}

pub const ADD_SCREEN: &str = "Add a screen";
pub const WORKING_ON: &str = "Working on";
pub const FIND_BY_ITSELF: &str = "Find by itself";
pub const NEW_PROJECT_HINT: &str = "New project from this window";

/// One "Working on" button.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chip {
    /// None: find by itself.
    pub id: Option<String>,
    pub label: String,
    pub selected: bool,
}

/// "Find by itself", then each project; the picked one (or "Find by itself") is selected.
pub fn chips(status: &Status) -> Vec<Chip> {
    let mut list = vec![Chip { id: None, label: FIND_BY_ITSELF.into(), selected: status.pinned.is_none() }];
    for p in &status.projects {
        list.push(Chip { id: Some(p.id.clone()), label: p.name.clone(), selected: status.pinned.as_deref() == Some(p.id.as_str()) });
    }
    list
}

/// "Now: Website", while recording and finding by itself, once a project is found.
pub fn now_line(status: &Status) -> Option<String> {
    if !status.recording || status.pinned.is_some() {
        return None;
    }
    status.current_project().map(|p| format!("Now: {}", p.name))
}

/// One bar: a project's name, its share of the longest bar (0–1), the time, muted for "Not sorted", and its colour
/// (the same as on the Timeline).
#[derive(Clone, Debug, PartialEq)]
pub struct Bar {
    pub name: String,
    pub fraction: f64,
    pub time: String,
    pub muted: bool,
    pub colour: Colour,
}

/// The same words as the Timeline's.
pub const NOT_SORTED: &str = crate::timeline::NOT_SORTED;

/// The top projects as bars (at most `MAX_BARS`), each relative to the longest. `colours`: each project's colour on
/// today's Timeline (`HomeNumbers::colours`); a project it doesn't have gets the next colour not taken; Not sorted grey.
pub fn bars(totals: &Totals, projects: &[Project], colours: &[(String, Colour)]) -> Vec<Bar> {
    let longest = totals.by_project.first().map_or(1.0, |(_, s)| *s).max(1e-9);
    let mut spare = Colour::ORDER.iter().copied().filter(|c| !colours.iter().any(|(_, t)| t == c)).collect::<Vec<_>>().into_iter().cycle();
    totals
        .by_project
        .iter()
        .take(MAX_BARS)
        .map(|(id, seconds)| Bar {
            name: if id.is_empty() { NOT_SORTED.into() } else { project_name(projects, id) },
            fraction: (seconds / longest).clamp(0.0, 1.0),
            time: WorkStats::duration(*seconds),
            muted: id.is_empty(),
            colour: if id.is_empty() {
                Colour::Grey
            } else {
                colours.iter().find(|(k, _)| k == id).map(|(_, c)| *c).or_else(|| spare.next()).unwrap_or(Colour::Blue)
            },
        })
        .collect()
}

/// The caption above today's bars.
pub const TODAY: &str = "Today";
pub const TODAY_EMPTY: &str = "Nothing yet today. Start contextualizing when you begin work.";
pub const RECENT_CAPSULES: &str = "Recent capsules";
/// How many capsules Home shows.
pub const RECENT_COUNT: usize = 3;
pub const SEE_ALL: &str = "See all";
pub const NEW_PROJECT: &str = "New project";

/// The Shortcuts card: (keys, what).
pub fn home_shortcuts(keys: &Keys) -> [(String, &'static str); 2] {
    [(keys.record.clone(), crate::shortcuts::START_OR_STOP), (keys.voice.clone(), crate::shortcuts::SPEAK_A_NOTE)]
}
pub const NONE_YET: &str = "None yet.";
pub const MAKE_TITLE: &str = "Make today's capsules";
pub const MAKE_BUTTON: &str = "Make";

/// The line under "Make today's capsules": it follows Your AI (Claude with the request typed in, another app,
/// or Copy my day). `day_line`: shown after Copy my day until the reply is saved.
pub fn make_hint(ai: &YourAi, name: &str, day_line: Option<&str>) -> String {
    crate::anyai::make_line(ai, name, day_line, "Enter")
}

/// Everything Home shows, computed off the UI thread from the files.
#[derive(Clone, Debug, Default)]
pub struct HomeNumbers {
    pub first_name: String,
    pub today: Totals,
    pub week: Totals,
    pub week_capsules: usize,
    /// Days in a row with a recording.
    pub streak: u32,
    /// The newest capsules (at most `RECENT_COUNT`).
    pub recent: Vec<CapsuleSummary>,
    /// Each project's colour on today's Timeline, for the bars.
    pub colours: Vec<(String, Colour)>,
}

impl HomeNumbers {
    /// Today, this week, the streak, the newest capsules and the name for the greeting. `os_name`: the account's full name,
    /// used when config.json has no "author".
    pub fn load(paths: &crate::paths::Paths, now: SystemTime, sunday_first: bool, os_name: &str) -> HomeNumbers {
        let projects = crate::projects::Projects::load(&paths.projects());
        let day = WorkStats::day_start(now);
        let week = WorkStats::week_start(now, sunday_first);
        let until = now + std::time::Duration::from_secs(60);
        let capsules = CapsuleSummary::all(&paths.capsules());
        let since = chrono::DateTime::<chrono::Utc>::from(week);
        let config = crate::exclusion::Config::load(&paths.config());
        HomeNumbers {
            first_name: crate::settings::first_name(config.author.as_deref().unwrap_or(os_name)),
            today: WorkStats::totals(&paths.sessions(), day, until, &projects),
            week: WorkStats::totals(&paths.sessions(), week, until, &projects),
            week_capsules: capsules.iter().filter(|c| c.made_since(since)).count(),
            streak: WorkStats::streak(&WorkStats::session_ids(&paths.sessions()), chrono::DateTime::<chrono::Local>::from(now).date_naive()),
            recent: capsules.into_iter().take(RECENT_COUNT).collect(),
            colours: crate::timeline::load_day(paths, chrono::DateTime::<chrono::Local>::from(now).date_naive()).project_colours(),
        }
    }
}

// MARK: Capsules and Projects

pub const MAKE_TODAYS: &str = "Make today's capsules";

pub fn no_capsules(keys: &Keys) -> String {
    format!("No capsules yet. Start contextualizing ({}), then make today's capsules.", keys.record)
}

pub const DRAFT: &str = "Draft";
pub const OPEN: &str = "Open";
pub const COPY_FILE: &str = "Copy file";
pub const COPY_TEXT: &str = "Copy text";
pub const SAVE_COPY: &str = "Save a copy…";
pub const COPIED_FILE: &str = "Copied. Paste it in WhatsApp, Slack, email or any chat.";
pub const COPIED_TEXT: &str = "Text copied. Paste it into a chat, or into your AI.";

/// Capsules: open one someone sent, search, and ask your AI (the button: `anyai::ask_button`).
pub const OPEN_CAPSULE: &str = "Open a capsule…";
pub const SEARCH_CAPSULES: &str = "Search capsules";
pub const NO_MATCHES: &str = "No capsules match. Ask your AI to search inside them.";

/// Before a capsule is shared (Copy file, Copy text, Save a copy…).
pub const CHECK_TITLE: &str = "Check before sharing";
pub const CHECK_INTRO: &str =
    "These might be private. Checked ones become [removed] in the copy you share. Your saved capsule stays as it is.";
pub const SHARE: &str = "Share";

pub const PROJECTS_INTRO: &str = "Work is sorted into these by itself, from words in window names and web addresses.";
pub const NO_PROJECTS: &str = "No projects yet. Your AI makes them when it sorts your day.";
pub const EDIT: &str = "Edit";
pub const DELETE: &str = "Delete";
pub const SAVE: &str = "Save";
pub const CANCEL: &str = "Cancel";
pub const NAME_LABEL: &str = "Name";
pub const CLUES_LABEL: &str = "Found by (words in window names or web addresses, with commas between)";
pub const DELETE_DETAIL: &str = "Its capsules stay. New work won't be sorted into it anymore.";

/// "Found by: acme.com, figma" / "Not found automatically yet"
pub fn found_by(project: &Project) -> String {
    if project.clues.is_empty() {
        "Not found automatically yet".into()
    } else {
        format!("Found by: {}", project.clues.join(", "))
    }
}

/// "Delete “Website”?"
pub fn delete_question(project: &Project) -> String {
    format!("Delete “{}”?", project.name)
}

// MARK: Settings

pub const YOU: &str = "You";
pub const YOUR_NAME: &str = "Your name";
pub const NAME_HELP: &str = "Shown on Home, and written on your capsules as the author.";
pub const SET_UP: &str = "Set up";
pub const EVERY_DAY: &str = "Every day";
pub const REMIND: &str = "Remind me to make capsules";
pub const REMIND_AT: &str = "At";
pub const KEEP: &str = "Delete old recordings after";
pub const SOUNDS: &str = "Sounds";
pub const KEPT: &str = "Capsules are never deleted.";
pub const VOICE_NOTES: &str = "Voice notes";
pub const LANGUAGE: &str = "Language";
pub const NEVER_RECORD: &str = "Never record";
pub const NEVER_RECORD_HELP: &str = "Password managers, WhatsApp and Signal are never recorded. Add other apps here.";
pub const ADD_APP: &str = "Add an app";
pub const CHOOSE: &str = "Choose…";
pub const REMOVE: &str = "Remove";
pub const UPDATES: &str = "Updates";
pub const APPEARANCE: &str = "Appearance";
/// Settings → Every day: what starting shares (config.json "share_screens").
pub const WHEN_YOU_START: &str = "When you start";
pub const SHARE_PICK: &str = "Pick a screen or window";
pub const SHARE_ALL: &str = "All my screens";
/// Shown when "Update automatically" can't be saved (config.json not writable).
pub const UPDATES_NOT_SAVED: &str = "Couldn't save this setting. Try again.";

/// "Version 0.2.0", in Settings → Updates.
pub fn version_line(version: &str) -> String {
    format!("Version {version}")
}

/// One setup row: what it is, its button while not done, and a short help line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SetupRow {
    pub title: &'static str,
    pub button: &'static str,
    pub help: &'static str,
}

pub const ROW_VOICE: SetupRow = SetupRow {
    title: "Voice notes",
    button: "Allow",
    help: "Say what you're doing and why. Turned into text on this computer; audio is never saved.",
};
pub const ROW_VOICE_MODEL: SetupRow = SetupRow {
    title: "Voice notes",
    button: "Get",
    help: "A one-time download (about 150 MB) so speech becomes text on this computer. Audio is never saved.",
};
/// Connect your AI: the AI apps listed under it (`anyai`); a ✓ when at least one is connected.
pub const ROW_AI: SetupRow = SetupRow { title: crate::anyai::CONNECT_YOUR_AI, button: crate::anyai::CONNECT, help: crate::anyai::CONNECT_HELP };
pub const ROW_LOGIN: SetupRow = SetupRow { title: "Open at login", button: "Turn On", help: "Always ready in the tray." };

// MARK: Help

pub const SHORTCUTS: &str = "Shortcuts";
pub const HOW_IT_WORKS: &str = "How it works";
pub const PRIVACY: &str = "Privacy";

/// (what, keys). The last one follows Your AI ("in your AI", or "Copy my day").
pub fn shortcuts(keys: &Keys, ai: &YourAi) -> [(&'static str, String); 3] {
    [
        ("Start or stop contextualizing", keys.record.clone()),
        ("Speak a note, then again when done", keys.voice.clone()),
        ("Make my capsules for today", crate::anyai::help_make_keys(ai).into()),
    ]
}

pub const HOW: [&str; 4] = [
    "While you work, Context Capsule saves the text of the screen you picked, and what you say in voice notes.",
    "At the end of the day, your AI sorts it into projects and writes a capsule for each: what you did, what you decided and why, and what's next.",
    "Any AI works. Apps like Claude or Cursor connect in Settings. For any other AI, such as a website: Copy my day, paste it in, then Save from clipboard.",
    "Send a capsule to anyone. Their AI reads it and can carry on your work.",
];

pub fn privacy(keys: &Keys) -> [String; 4] {
    [
        format!("Recordings stay on {}. No account, no cloud.", keys.this_computer),
        "Only text is saved: no screenshots, no audio.".into(),
        "Password managers, WhatsApp, Signal and password fields are never recorded.".into(),
        "Text goes to your AI only when you ask it to make or read a capsule, or paste your day into it yourself.".into(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn wk() -> Keys {
        Keys::windows(&Hotkeys::DEFAULT)
    }

    #[test]
    fn home_words() {
        assert_eq!(greeting(20, "Saachi"), "Good evening, Saachi.");
        assert_eq!(greeting(9, " "), "Good morning.");
        let now = SystemTime::now();
        let mut s = Status::default();
        assert_eq!(status_title(&s, now), "Not contextualizing");
        assert_eq!(toggle_button(&s), "Start contextualizing");
        assert_eq!(status_detail(&s, &wk(), ShareScreens::Pick), "Alt+C to start. Pick the screen or window you work on. Only that is read.");
        assert_eq!(status_detail(&s, &wk(), ShareScreens::All), "Alt+C to start. Shares all your screens. Private apps are never read.");
        s.picking = true;
        assert_eq!((status_title(&s, now).as_str(), toggle_button(&s)), ("Pick a window or screen", "Cancel"));
        s = Status { recording: true, session_id: Some("x".into()), started_at: Some(now), events: 1, sources: 1, ..Default::default() };
        assert_eq!(status_title(&s, now + Duration::from_secs(12)), "Contextualizing  0:12");
        assert_eq!(toggle_button(&s), "Stop");
        assert_eq!(status_detail(&s, &wk(), ShareScreens::Pick), "1 moment saved");
        s.events = 12;
        s.sources = 2;
        assert_eq!(status_detail(&s, &wk(), ShareScreens::All), "12 moments saved · 2 screens shared");
        assert_eq!(voice_button(&s), "Speak a note");
        s.listening = true;
        assert_eq!(voice_button(&s), "Done speaking");
        assert_eq!(make_hint(&YourAi::Claude, "Claude", None), "Claude opens with the request typed in. Press Enter.");
        assert_eq!(make_hint(&YourAi::Other, "your AI", None), "Copy your day, paste it into your AI, then save its reply here.");
    }

    #[test]
    fn working_on() {
        let web = Project { id: "web".into(), name: "Website".into(), clues: vec![] };
        let mut s = Status { projects: vec![web], ..Default::default() };
        let c = chips(&s);
        assert_eq!((c[0].label.as_str(), c[0].selected, c[1].selected), ("Find by itself", true, false));
        s.pinned = Some("web".into());
        assert!(!chips(&s)[0].selected && chips(&s)[1].selected);
        s.project = Some("web".into());
        s.recording = true;
        assert_eq!(now_line(&s), None, "picked: no need to say");
        s.pinned = None;
        assert_eq!(now_line(&s).as_deref(), Some("Now: Website"));
    }

    #[test]
    fn bars_and_captions() {
        let projects = vec![Project { id: "web".into(), name: "Website".into(), clues: vec![] }];
        let t = Totals { seconds: 2403.0, by_project: vec![("web".into(), 1803.0), ("".into(), 600.0)] };
        let b = bars(&t, &projects, &[]);
        assert_eq!(b[0], Bar { name: "Website".into(), fraction: 1.0, time: "30 min".into(), muted: false, colour: Colour::Blue });
        assert_eq!((b[1].name.as_str(), b[1].muted, b[1].time.as_str(), b[1].colour), ("Not sorted", true, "10 min", Colour::Grey));
        assert!((b[1].fraction - 600.0 / 1803.0).abs() < 1e-9);
        let many = Totals { seconds: 0.0, by_project: (0..8).map(|i| (format!("p{i}"), 10.0)).collect() };
        assert_eq!(bars(&many, &[], &[]).len(), MAX_BARS);
        // The Timeline's colours; one it doesn't have gets the next colour not taken.
        let t3 = Totals { seconds: 0.0, by_project: vec![("a".into(), 30.0), ("b".into(), 20.0), ("c".into(), 10.0)] };
        let timeline = [("b".to_string(), Colour::Blue), ("a".to_string(), Colour::Teal)];
        let colours: Vec<Colour> = bars(&t3, &[], &timeline).iter().map(|b| b.colour).collect();
        assert_eq!(colours, [Colour::Teal, Colour::Blue, Colour::Orange]);
        let n = HomeNumbers { today: t.clone(), week: t.clone(), week_capsules: 1, streak: 3, ..Default::default() };
        let s = stats(&n);
        assert_eq!((s[0].label, s[0].value.as_str(), s[0].detail.as_str()), ("Today", "40 min", "contextualized"));
        assert_eq!((s[1].label, s[1].detail.as_str()), ("This week", "1 capsule"));
        assert_eq!((s[2].label, s[2].value.as_str(), s[2].detail.as_str()), ("Streak", "3 days", "in a row"));
        let none = stats(&HomeNumbers::default());
        assert_eq!((none[1].detail.as_str(), none[2].value.as_str()), ("0 capsules", "0 days"));
        let now = SystemTime::now();
        assert_eq!(status_pill(&Status::default(), now, 120.0), "Today 2 min");
        assert_eq!(status_pill(&Status::default(), now, 0.0), "Today 0 min");
        assert_eq!(status_pill(&Status::default(), now, 30.0), "Today under 1 min");
        assert_eq!(status_pill(&Status::default(), now, 3900.0), "Today 1 h 5 min");
        let rec = Status { recording: true, started_at: Some(now), ..Default::default() };
        assert_eq!(status_pill(&rec, now + std::time::Duration::from_secs(12), 0.0), "● 0:12");
        assert_eq!(home_shortcuts(&wk()), [("Alt+C".into(), "Start or stop"), ("Alt+V".into(), "Speak a note")]);
        let mine = Hotkeys::DEFAULT.with(crate::shortcuts::Which::Record, crate::Shortcut::parse("Ctrl+Shift+R").unwrap());
        assert_eq!(home_shortcuts(&Keys::linux(&mine))[0].0, "Ctrl+Shift+R", "the current shortcut, not the default");
        assert!(status_detail(&Status::default(), &Keys::windows(&mine), ShareScreens::Pick).starts_with("Ctrl+Shift+R to start."));
    }

    #[test]
    fn sizes_and_words() {
        assert_eq!(content_width(600.0), (600.0, false));
        assert_eq!(content_width(800.0), (760.0, false));
        assert_eq!(content_width(1400.0), (980.0, true));
        assert_eq!(Section::ALL.map(|s| s.title()), ["Home", "Timeline", "Capsules", "Projects", "Settings", "Help"]);
        assert_eq!(Section::ALL.map(|s| s.group()), ["Your work", "Your work", "Your work", "Your work", "Karya Context Capsule", "Karya Context Capsule"]);
        let p = Project { id: "web".into(), name: "Website".into(), clues: vec!["acme.com".into(), "figma".into()] };
        assert_eq!(found_by(&p), "Found by: acme.com, figma");
        assert_eq!(delete_question(&p), "Delete “Website”?");
        assert_eq!(shortcuts(&wk(), &YourAi::Claude)[0], ("Start or stop contextualizing", "Alt+C".to_string()));
        assert_eq!(shortcuts(&wk(), &YourAi::Other)[2].1, "Copy my day");
        assert_eq!(shortcuts(&wk(), &YourAi::Claude)[2].1, "in your AI");
        assert_eq!(privacy(&wk())[0], "Recordings stay on this PC. No account, no cloud.");
        assert!(no_capsules(&Keys::linux(&Hotkeys::DEFAULT)).contains("(Alt+C)"));
    }
}
