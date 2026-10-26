// context-capsule — core: what the Timeline needs from this side: which recording is going now, and deleting a whole
// recording (the bridge's `timeline.delete`). The Timeline itself is drawn by the shared page (`ui/js/core/timeline.js`).
//! Nothing here leaves the computer. Session ids are local time ("20261019-100103").

use crate::paths::Paths;

/// "YYYYMMDD-HHMMSS" and nothing else (never a path).
pub fn is_session_id(id: &str) -> bool {
    let b = id.as_bytes();
    b.len() == 15 && b.iter().enumerate().all(|(i, c)| if i == 8 { *c == b'-' } else { c.is_ascii_digit() })
}

/// What state.json says is recording now (its session id).
pub fn recording_session(paths: &Paths) -> Option<String> {
    let data = std::fs::read(paths.state()).ok()?;
    let v: serde_json::Value = serde_json::from_slice(&data).ok()?;
    if v.get("recording")?.as_bool()? {
        v.get("session_id")?.as_str().filter(|s| !s.is_empty()).map(String::from)
    } else {
        None
    }
}

/// Why a delete didn't happen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refused {
    /// That recording is still going: stop first.
    Recording,
    /// It's not there (any more), or the name isn't a recording's.
    NotFound,
    Io(String),
}

/// Remove a whole recording (its folder). Not while recording it.
pub fn delete_session(paths: &Paths, session: &str) -> Result<(), Refused> {
    if !is_session_id(session) {
        return Err(Refused::NotFound);
    }
    if recording_session(paths).as_deref() == Some(session) {
        return Err(Refused::Recording);
    }
    let dir = paths.session_dir(session);
    if !dir.is_dir() {
        return Err(Refused::NotFound);
    }
    std::fs::remove_dir_all(&dir).map_err(|e| Refused::Io(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delete_a_recording() {
        let root = std::env::temp_dir().join(format!("cc-timeline-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let paths = Paths::new(root.clone());
        for id in ["20261020-093000", "20261020-120000"] {
            std::fs::create_dir_all(paths.session_dir(id)).unwrap();
        }
        std::fs::write(paths.state(), r#"{"recording": true, "session_id": "20261020-120000"}"#).unwrap();
        assert_eq!(recording_session(&paths).as_deref(), Some("20261020-120000"));
        assert_eq!(delete_session(&paths, "20261020-120000"), Err(Refused::Recording));
        delete_session(&paths, "20261020-093000").unwrap();
        assert!(!paths.session_dir("20261020-093000").exists());
        assert_eq!(delete_session(&paths, "20261020-093000"), Err(Refused::NotFound));
        assert_eq!(delete_session(&paths, ".."), Err(Refused::NotFound), "never outside sessions/");
        assert_eq!(delete_session(&paths, "notes"), Err(Refused::NotFound));
        assert!(is_session_id("20261019-231500") && !is_session_id("2026101-9231500") && !is_session_id("../../etc/x"));
        std::fs::write(paths.state(), r#"{"recording": false, "session_id": "20261020-120000"}"#).unwrap();
        assert_eq!(recording_session(&paths), None);
        std::fs::remove_dir_all(&root).unwrap();
    }
}
