// context-capsule — core: the platform-neutral half of the Windows and Linux recorders.
//! A faithful port of `capture/Sources/Core.swift` and `Recorder.swift`: the same session files,
//! byte for byte (see SPEC.md). Everything that touches the OS goes through [`platform::Platform`]
//! and [`voice::VoiceEngine`]; nothing here is platform code.

#![forbid(unsafe_code)]

pub mod clock;
pub mod engine;
pub mod events;
pub mod exclusion;
pub mod geometry;
pub mod image;
pub mod json;
pub mod membership;
pub mod paths;
pub mod platform;
pub mod projects;
pub mod redact;
pub mod state;
pub mod text;
pub mod voice;

pub use engine::{Engine, Options};
pub use events::{Event, EventLog, Recorded};
pub use exclusion::{Config, Exclusion};
pub use geometry::Rect;
pub use image::Image;
pub use membership::{
    Freshness, ListedWindow, Membership, OcrDecision, PickStyle, PickedContent, Skip, WindowList,
};
pub use paths::Paths;
pub use platform::{project_message, Frame, Front, Indicator, Msg, PickOutcome, Platform, ProjectItem, Status};
pub use projects::{Location, Project, ProjectChoice, Projects};
pub use redact::Redactor;
pub use state::{session_json, ControlCommand, ControlKind, StateJson};
pub use text::{NodeKind, TextNode, TextSource, UiText};
pub use voice::{
    PlatformText, Replier, Voice, VoiceClip, VoiceEngine, VoiceNotes, VoiceOutcome, VoicePermission,
    VoicePhase, VoiceReply, VoiceSignal, VoiceTarget,
};
