// context-capsule — core: what the tray icon shows, the same on Windows and Linux — a running timer while
// recording, a red mic with bars that move with the voice while listening — and the soft start/stop sounds.
// Takes no screen space: no badge, no window.

use crate::platform::Status;
use std::time::SystemTime;

/// "12:34" (or "1:02:03").
pub fn clock(seconds: u64) -> String {
    let s = seconds;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

/// How long the recording has run, as `clock` shows it. None when not recording.
pub fn elapsed(status: &Status, now: SystemTime) -> Option<String> {
    let since = status.started_at.filter(|_| status.recording)?;
    Some(clock(now.duration_since(since).map(|d| d.as_secs()).unwrap_or(0)))
}

/// The listening icon comes in this many steps, from quiet to loud.
pub const LEVEL_FRAMES: usize = 4;

/// Which listening step shows for a mic level (0…1, the loudest sample of the latest buffer).
pub fn level_frame(level: f32) -> usize {
    let l = (level * 2.5).clamp(0.0, 1.0);
    (l * (LEVEL_FRAMES - 1) as f32).round() as usize
}

/// A red mic followed by three bars whose height follows the step: coverage (0…255) of a `size`×`size`
/// icon, top row first, anti-aliased by 4×4 supersampling. Colour it as the tray needs.
pub fn mic_bars(size: usize, frame: usize) -> Vec<u8> {
    let l = frame.min(LEVEL_FRAMES - 1) as f64 / (LEVEL_FRAMES - 1) as f64;
    let mut out = Vec::with_capacity(size * size);
    for py in 0..size {
        for px in 0..size {
            let mut hits = 0u32;
            for sy in 0..4 {
                for sx in 0..4 {
                    let x = (px as f64 + (sx as f64 + 0.5) / 4.0) / size as f64;
                    let y = (py as f64 + (sy as f64 + 0.5) / 4.0) / size as f64;
                    hits += covers(x, y, l) as u32;
                }
            }
            out.push((hits * 255 / 16) as u8);
        }
    }
    out
}

/// Bar heights (share of the icon) for a loudness 0…1.
pub fn bar_heights(l: f64) -> [f64; 3] {
    let weights = [0.6, 1.0, 0.75];
    weights.map(|w| 0.16 + 0.66 * l * w)
}

/// Is (x, y) — 0…1 across the icon, top left origin — inside the mic or a bar?
fn covers(x: f64, y: f64, l: f64) -> bool {
    // The mic, on the left: a capsule, a U-shaped holder, a stem and a base.
    let (mx, t) = (0.28, 0.08);
    let cy = y.clamp(0.20, 0.42);
    let body = (x - mx).hypot(y - cy) <= 0.14;
    let hd = (x - mx).hypot(y - 0.42);
    let holder = y >= 0.42 && hd <= 0.24 + t / 2.0 && hd >= 0.24 - t / 2.0;
    let stem = (x - mx).abs() <= t / 2.0 && (0.66..=0.84).contains(&y);
    let base = (mx - 0.15..=mx + 0.15).contains(&x) && (0.82..=0.82 + t).contains(&y);
    // Three bars on the right, centred, rounded ends.
    let bars = bar_heights(l).iter().zip([0.62, 0.76, 0.90]).any(|(h, bx)| {
        let (half_w, half_h) = (0.045, h / 2.0);
        let dy = ((y - 0.5).abs() - (half_h - half_w)).max(0.0);
        (x - bx).abs() <= half_w && (dy == 0.0 || (x - bx).hypot(dy) <= half_w)
    });
    body || holder || stem || base || bars
}

/// A soft sound at the start or end of a recording or a voice note (off with "sounds": false).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cue {
    RecordingStarted,
    RecordingStopped,
    MicOpened,
    MicClosed,
}

impl Cue {
    pub const ALL: [Cue; 4] = [Cue::RecordingStarted, Cue::RecordingStopped, Cue::MicOpened, Cue::MicClosed];
    pub const RATE: u32 = 22_050;
    /// Loudest sample, as a share of full scale: soft.
    pub const VOLUME: f64 = 0.22;

    /// The sounds for a change of status: the mic first, then recording (like the Mac).
    pub fn changes(prev: &Status, next: &Status) -> Vec<Cue> {
        let mut out = vec![];
        if prev.listening != next.listening {
            out.push(if next.listening { Cue::MicOpened } else { Cue::MicClosed });
        }
        if prev.recording != next.recording {
            out.push(if next.recording { Cue::RecordingStarted } else { Cue::RecordingStopped });
        }
        out
    }

    /// Short notes (Hz, ms): two rising for start, two falling for stop, one high tick for the mic.
    fn notes(self) -> &'static [(f64, u32)] {
        match self {
            Cue::RecordingStarted => &[(660.0, 70), (880.0, 110)],
            Cue::RecordingStopped => &[(880.0, 70), (587.0, 120)],
            Cue::MicOpened => &[(1319.0, 70)],
            Cue::MicClosed => &[(988.0, 80)],
        }
    }

    /// The sound as a WAV file (16-bit mono PCM), made in code: no sound files to ship.
    pub fn wav(self) -> Vec<u8> {
        let mut samples: Vec<i16> = vec![];
        for (freq, ms) in self.notes() {
            let n = (Self::RATE as u64 * *ms as u64 / 1000) as usize;
            let attack = (Self::RATE as usize * 4 / 1000).max(1);
            for i in 0..n {
                let t = i as f64 / Self::RATE as f64;
                // A quick fade in, then a smooth decay to silence: no clicks.
                let env = (i as f64 / attack as f64).min(1.0) * (1.0 - i as f64 / n as f64).powi(2);
                let v = (2.0 * std::f64::consts::PI * freq * t).sin() * env * Self::VOLUME;
                samples.push((v * i16::MAX as f64) as i16);
            }
        }
        let data_len = (samples.len() * 2) as u32;
        let mut out = Vec::with_capacity(44 + data_len as usize);
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + data_len).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk size
        out.extend_from_slice(&1u16.to_le_bytes()); // PCM
        out.extend_from_slice(&1u16.to_le_bytes()); // mono
        out.extend_from_slice(&Self::RATE.to_le_bytes());
        out.extend_from_slice(&(Self::RATE * 2).to_le_bytes()); // bytes per second
        out.extend_from_slice(&2u16.to_le_bytes()); // block align
        out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
        out.extend_from_slice(b"data");
        out.extend_from_slice(&data_len.to_le_bytes());
        for s in samples {
            out.extend_from_slice(&s.to_le_bytes());
        }
        out
    }

    /// A short name, for the file a player is handed on Linux.
    pub fn name(self) -> &'static str {
        match self {
            Cue::RecordingStarted => "recording-started",
            Cue::RecordingStopped => "recording-stopped",
            Cue::MicOpened => "mic-opened",
            Cue::MicClosed => "mic-closed",
        }
    }
}
