// context-capsule — core: the platform-neutral half of the Windows and Linux recorders.
//! A faithful port of `capture/Sources/Core.swift` and `Recorder.swift`: the same session files,
//! byte for byte (see SPEC.md). Everything that touches the OS goes through [`platform::Platform`]
//! and [`voice::VoiceEngine`]; nothing here is platform code.

#![forbid(unsafe_code)]

pub mod applog;
pub mod capsules;
pub mod clock;
pub mod engine;
pub mod events;
pub mod exclusion;
pub mod geometry;
pub mod image;
pub mod json;
pub mod look;
pub mod membership;
pub mod newlines;
pub mod paths;
pub mod platform;
pub mod pointer;
pub mod projects;
pub mod redact;
pub mod reminder;
pub mod settings;
pub mod state;
pub mod stats;
pub mod text;
pub mod update;
pub mod voice;
pub mod window;

pub use applog::AppLog;
pub use capsules::{CapsuleSummary, Group};
pub use settings::{ConfigFile, ShareScreens};
pub use stats::{Assignments, Fix, Totals, WorkStats};
pub use engine::{Engine, Options};
pub use events::{Event, EventLog, Recorded};
pub use exclusion::{Config, Exclusion};
pub use geometry::Rect;
pub use image::Image;
pub use membership::{
    Freshness, ListedWindow, Membership, OcrDecision, PickStyle, PickedContent, Skip, WindowList,
};
pub use look::Cue;
pub use newlines::NewLines;
pub use paths::Paths;
pub use platform::{
    hotkeys_taken_message, listening_message, project_message, recording_started_message, recording_stopped_message, screens_message, Frame, Front, Indicator,
    Msg, PickOutcome, Platform, ProjectItem, Status, ALREADY_SHARED, MAX_SOURCES,
};
pub use reminder::{Cleanup, DailyJobs, Reminder};
pub use pointer::PointerRule;
pub use projects::{Location, Project, ProjectChoice, Projects};
pub use redact::Redactor;
pub use state::{session_json, ControlCommand, ControlKind, StateJson};
pub use text::{NodeKind, TextNode, TextSource, UiText};
pub use voice::{
    busy_listening_message, PlatformText, Replier, Voice, VoiceClip, VoiceEngine, VoiceNotes, VoiceOutcome, VoicePermission,
    VoicePhase, VoiceReply, VoiceSignal, VoiceTarget,
};
