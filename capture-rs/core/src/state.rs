// context-capsule — core: session.json, state.json and the `control` commands the MCP server writes.

use crate::clock;
use crate::json::{json_object, JsonValue};
use crate::membership::Skip;
use std::time::SystemTime;

/// `{"id","started_at","ended_at"|null,"host","version":1}`
pub fn session_json(id: &str, started_at: SystemTime, ended_at: Option<SystemTime>, host: &str) -> String {
    json_object(&[
        ("id", JsonValue::Str(id.to_string())),
        ("started_at", JsonValue::Str(clock::iso(started_at))),
        ("ended_at", JsonValue::opt_str(ended_at.map(clock::iso))),
        ("host", JsonValue::Str(host.to_string())),
        ("version", JsonValue::Int(1)),
    ])
}

/// state.json. `picking`: the picker is open, waiting for the user. `last_error`: why the last start did
/// not happen ("cancelled" when the user closed the picker); cleared on the next start.
/// While recording: `events` written so far and `skip`, why the latest sample wrote nothing (None if it
/// wrote something). After a stop they describe the session that just ended.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StateJson {
    pub recording: bool,
    pub session_id: Option<String>,
    pub started_at: Option<SystemTime>,
    pub pid: u32,
    pub picking: bool,
    pub last_error: Option<String>,
    pub events: usize,
    pub skip: Option<Skip>,
    pub listening: bool,
    pub note_at: Option<i64>,
    /// The current project's id, and how it was chosen ("auto" / "you").
    pub project: Option<String>,
    pub project_by: Option<String>,
}

impl StateJson {
    pub fn json(&self) -> String {
        json_object(&[
            ("recording", JsonValue::Bool(self.recording)),
            ("session_id", JsonValue::opt_str(self.session_id.clone())),
            ("started_at", JsonValue::opt_str(self.started_at.map(clock::iso))),
            ("pid", JsonValue::Int(self.pid as i64)),
            ("picking", JsonValue::Bool(self.picking)),
            ("last_error", JsonValue::opt_str(self.last_error.clone())),
            ("events", JsonValue::Int(self.events as i64)),
            ("skip", JsonValue::opt_str(self.skip.map(|s| s.as_str()))),
            ("listening", JsonValue::Bool(self.listening)),
            ("note_at", self.note_at.map(JsonValue::Int).unwrap_or(JsonValue::Null)),
            ("project", JsonValue::opt_str(self.project.clone())),
            ("project_by", JsonValue::opt_str(self.project_by.clone())),
        ])
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlKind {
    Start,
    Stop,
    Note,
    Project,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ControlCommand {
    pub kind: ControlKind,
    pub at: i64,
    /// The typed note, for `note`.
    pub text: Option<String>,
    /// A project id, or "auto", for `project`.
    pub project: Option<String>,
}

impl ControlCommand {
    /// `{"command":"start"|"stop","at":<ms>}`, `{"command":"note","text":"…","at":<ms>}` or
    /// `{"command":"project","project":"<id>"|"auto","at":<ms>}`; anything else (a note without text,
    /// a project without an id) is ignored.
    pub fn parse(data: &[u8]) -> Option<ControlCommand> {
        let serde_json::Value::Object(obj) = serde_json::from_slice(data).ok()? else { return None };
        let kind = match obj.get("command")?.as_str()?.to_lowercase().as_str() {
            "start" => ControlKind::Start,
            "stop" => ControlKind::Stop,
            "note" => ControlKind::Note,
            "project" => ControlKind::Project,
            _ => return None,
        };
        // Like NSNumber.int64Value: floats truncate, booleans are 1 / 0, anything else is 0.
        let at = match obj.get("at") {
            Some(serde_json::Value::Number(n)) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)).unwrap_or(0),
            Some(serde_json::Value::Bool(b)) => *b as i64,
            _ => 0,
        };
        let mut text = None;
        if kind == ControlKind::Note {
            let t = obj.get("text")?.as_str()?.trim();
            if t.is_empty() {
                return None;
            }
            text = Some(t.to_string());
        }
        let mut project = None;
        if kind == ControlKind::Project {
            let p = crate::projects::trim_spaces(obj.get("project")?.as_str()?);
            if p.is_empty() {
                return None;
            }
            project = Some(p.to_string());
        }
        Some(ControlCommand { kind, at, text, project })
    }
}
