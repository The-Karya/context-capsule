// context-capsule — linux: voice notes, fully on this computer — the mic (cpal) opens only between the
// two presses, audio stays in memory (mono, resampled to 16 kHz) and goes to whisper.cpp. No audio on disk.

use context_capsule_core::{Replier, VoiceEngine, VoiceOutcome, VoiceReply};

/// Built without the "voice" feature: every note says so.
#[cfg(not(feature = "voice"))]
pub struct NoVoice;

#[cfg(not(feature = "voice"))]
impl VoiceEngine for NoVoice {
    fn prepare(&mut self, _locale: &str, reply: Replier) {
        reply.send(VoiceReply::Prepared(Some(VoiceOutcome::Failed(
            "voice notes are not part of this build (it was built without the \"voice\" feature)".into(),
        ))));
    }
    fn start(&mut self) -> Result<(), String> {
        Err("voice notes are not part of this build".into())
    }
    fn finish(&mut self, reply: Replier) {
        reply.send(VoiceReply::Finished(Err("voice notes are not part of this build".into())));
    }
    fn cancel(&mut self) {}
    fn device_name(&self) -> Option<String> {
        None
    }
}

#[cfg(feature = "voice")]
pub use whisper::WhisperVoice;

/// The whisper language code for a locale: "en-US" → "en", "nb-NO" → "no", "iw" → "he".
#[cfg(feature = "voice")]
pub fn whisper_lang(locale: &str) -> String {
    let lang = locale.split(['-', '_', '.', '@']).next().unwrap_or("").to_lowercase();
    match lang.as_str() {
        "nb" | "nn" => "no".into(),
        "iw" => "he".into(),
        "fil" => "tl".into(),
        _ => lang,
    }
}

/// Interleaved frames → mono (the mean of the channels).
#[cfg(feature = "voice")]
pub fn to_mono(interleaved: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return interleaved.to_vec();
    }
    interleaved.chunks_exact(channels).map(|f| f.iter().sum::<f32>() / channels as f32).collect()
}

/// Mono audio at `from` Hz → 16 kHz. Downsampling averages each output sample's span of input
/// (a simple low-pass); upsampling interpolates linearly.
#[cfg(feature = "voice")]
pub fn resample_16k(input: &[f32], from: u32) -> Vec<f32> {
    const TO: f64 = 16_000.0;
    if from == 0 || input.is_empty() {
        return vec![];
    }
    if from == 16_000 {
        return input.to_vec();
    }
    let ratio = from as f64 / TO;
    let n = ((input.len() as f64) / ratio).floor() as usize;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let start = i as f64 * ratio;
        if ratio > 1.0 {
            let (a, b) = (start.floor() as usize, ((start + ratio).floor() as usize).min(input.len()));
            let span = &input[a..b.max(a + 1).min(input.len())];
            out.push(span.iter().sum::<f32>() / span.len() as f32);
        } else {
            let a = start.floor() as usize;
            let t = (start - a as f64) as f32;
            let x0 = input[a];
            let x1 = *input.get(a + 1).unwrap_or(&x0);
            out.push(x0 + (x1 - x0) * t);
        }
    }
    out
}

/// whisper's segments → the note's text, without non-speech tags like "[BLANK_AUDIO]" or "(music)".
#[cfg(feature = "voice")]
pub fn clean_transcript<'a>(segments: impl IntoIterator<Item = &'a str>) -> String {
    let tag = |s: &str| (s.starts_with('[') && s.ends_with(']')) || (s.starts_with('(') && s.ends_with(')'));
    segments.into_iter().map(str::trim).filter(|s| !s.is_empty() && !tag(s)).collect::<Vec<_>>().join(" ")
}

#[cfg(feature = "voice")]
mod whisper {
    use super::*;
    use crate::model;
    use crate::notify::Notifier;
    use context_capsule_core::{Voice, VoiceClip};
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

    /// Longest clip kept in memory (a little over the core's 2-minute limit).
    const MAX_SECONDS: usize = 130;

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Download {
        Idle,
        Asking,
        Running,
    }

    #[derive(Default)]
    struct Captured {
        mono: Vec<f32>,
        peak: f32,
    }

    struct Mic {
        _stream: cpal::Stream, // dropping it closes the mic
        captured: Arc<Mutex<Captured>>,
        rate: u32,
    }

    pub struct WhisperVoice {
        root: PathBuf,
        notifier: Notifier,
        ctx: Arc<Mutex<Option<Arc<WhisperContext>>>>,
        lang: String,
        download: Arc<Mutex<Download>>,
        mic: Option<Mic>,
    }

    impl WhisperVoice {
        pub fn new(root: PathBuf, notifier: Notifier) -> WhisperVoice {
            whisper_rs::install_logging_hooks(); // keep whisper.cpp quiet
            WhisperVoice {
                root,
                notifier,
                ctx: Arc::default(),
                lang: "en".into(),
                download: Arc::new(Mutex::new(Download::Idle)),
                mic: None,
            }
        }

        fn model_path(&self) -> PathBuf {
            model::path(&self.root)
        }

        /// Ask once (until answered) to download the model; then fetch it in the background.
        fn offer_download(&self) {
            {
                let mut d = self.download.lock().unwrap();
                if *d != Download::Idle {
                    return;
                }
                *d = Download::Asking;
            }
            let (download, notifier, path) = (self.download.clone(), self.notifier.clone(), self.model_path());
            std::thread::spawn(move || {
                let body = format!(
                    "Voice notes are turned into text on this computer by whisper.cpp. That needs a one-time download of \
                     its speech model (about 150 MB, from Hugging Face) into {}. Your voice is never sent anywhere.",
                    path.parent().map(|p| p.display().to_string()).unwrap_or_default()
                );
                if !notifier.ask("Download the speech model?", &body, "Download", "Not now") {
                    *download.lock().unwrap() = Download::Idle;
                    return;
                }
                *download.lock().unwrap() = Download::Running;
                notifier.toast("Downloading the speech model (about 150 MB)…");
                let mut shown = 0;
                let result = model::download(&path, |n| {
                    let pct = n * 100 / model::BYTES;
                    if pct >= shown + 25 && pct < 100 {
                        shown = pct - pct % 25;
                        eprintln!("Context Capsule: speech model {shown}%");
                    }
                });
                *download.lock().unwrap() = Download::Idle;
                match result {
                    Ok(()) => {
                        let voice_key = crate::hotkeys::current().voice.label();
                        notifier.toast(&format!("Voice notes are ready. Press {voice_key} while contextualizing."))
                    }
                    Err(e) => notifier.toast(&format!("Speech model: {e}")),
                }
            });
        }

        fn missing_message(&self) -> String {
            if *self.download.lock().unwrap() == Download::Running {
                return "The speech model is still downloading. Try the voice note again in a minute.".into();
            }
            format!(
                "Voice notes need the on-device speech model first. Allow the download (about 150 MB) when asked, or put {} into {}.",
                model::FILE,
                self.model_path().parent().map(|p| p.display().to_string()).unwrap_or_default()
            )
        }
    }

    impl VoiceEngine for WhisperVoice {
        fn prepare(&mut self, locale: &str, reply: Replier) {
            let lang = whisper_lang(locale);
            if whisper_rs::get_lang_id(&lang).is_none() {
                return reply.send(VoiceReply::Prepared(Some(VoiceOutcome::Unavailable { locale: locale.into() })));
            }
            self.lang = lang;
            if self.ctx.lock().unwrap().is_some() {
                return reply.send(VoiceReply::Prepared(None));
            }
            let path = self.model_path();
            if !model::is_present(&path) {
                self.offer_download();
                return reply.send(VoiceReply::Prepared(Some(VoiceOutcome::Notice(self.missing_message()))));
            }
            // Check and load the model off the engine's thread (about a second).
            let ctx = self.ctx.clone();
            std::thread::spawn(move || {
                let loaded = model::verify(&path, model::SHA256).and_then(|_| {
                    WhisperContext::new_with_params(&path, WhisperContextParameters::default())
                        .map_err(|e| format!("cannot load the speech model: {e}"))
                });
                match loaded {
                    Ok(c) => {
                        *ctx.lock().unwrap() = Some(Arc::new(c));
                        reply.send(VoiceReply::Prepared(None));
                    }
                    Err(e) => reply.send(VoiceReply::Prepared(Some(VoiceOutcome::Failed(e)))),
                }
            });
        }

        fn start(&mut self) -> Result<(), String> {
            let host = cpal::default_host();
            let device = host.default_input_device().ok_or("no microphone found")?;
            let config = device.default_input_config().map_err(|e| format!("the microphone could not start ({e})"))?;
            let (rate, channels) = (config.sample_rate(), config.channels() as usize);
            let captured = Arc::new(Mutex::new(Captured::default()));
            let cap = rate as usize * MAX_SECONDS;
            let sink = captured.clone();
            let push = move |samples: &[f32]| {
                let mono = to_mono(samples, channels);
                let level = Voice::peak(&mono);
                Voice::set_level(level); // the tray's moving bars; never stored
                let mut c = sink.lock().unwrap();
                c.peak = c.peak.max(level);
                let room = cap.saturating_sub(c.mono.len());
                c.mono.extend_from_slice(&mono[..mono.len().min(room)]);
            };
            let err = |e: cpal::Error| eprintln!("Context Capsule: microphone: {e}");
            let stream = match config.sample_format() {
                cpal::SampleFormat::F32 => {
                    device.build_input_stream(config.into(), move |d: &[f32], _: &_| push(d), err, None)
                }
                cpal::SampleFormat::I16 => device.build_input_stream(
                    config.into(),
                    move |d: &[i16], _: &_| push(&d.iter().map(|s| *s as f32 / 32768.0).collect::<Vec<_>>()),
                    err,
                    None,
                ),
                cpal::SampleFormat::I32 => device.build_input_stream(
                    config.into(),
                    move |d: &[i32], _: &_| push(&d.iter().map(|s| *s as f32 / 2_147_483_648.0).collect::<Vec<_>>()),
                    err,
                    None,
                ),
                cpal::SampleFormat::U16 => device.build_input_stream(
                    config.into(),
                    move |d: &[u16], _: &_| {
                        push(&d.iter().map(|s| (*s as f32 - 32768.0) / 32768.0).collect::<Vec<_>>())
                    },
                    err,
                    None,
                ),
                f => return Err(format!("the microphone's sample format ({f}) is not supported")),
            }
            .map_err(|e| format!("the microphone could not start ({e})"))?;
            stream.play().map_err(|e| format!("the microphone could not start ({e})"))?;
            self.mic = Some(Mic { _stream: stream, captured, rate });
            Ok(())
        }

        fn finish(&mut self, reply: Replier) {
            let Some(mic) = self.mic.take() else {
                return reply.send(VoiceReply::Finished(Err("the microphone was not open".into())));
            };
            let Mic { _stream, captured, rate } = mic;
            drop(_stream); // the mic closes now
            Voice::set_level(0.0);
            let Captured { mono, peak } = std::mem::take(&mut *captured.lock().unwrap());
            let seconds = mono.len() as f64 / rate.max(1) as f64;
            let ctx = self.ctx.lock().unwrap().clone();
            let lang = self.lang.clone();
            std::thread::spawn(move || {
                let clip = |text: String| VoiceClip { text, peak, seconds };
                // Silent or too short: nothing to recognize (the core says "Didn't catch that").
                if peak < Voice::SILENT_PEAK || seconds < Voice::MIN_SECONDS {
                    return reply.send(VoiceReply::Finished(Ok(clip(String::new()))));
                }
                let Some(ctx) = ctx else {
                    return reply.send(VoiceReply::Finished(Err("the speech model is not loaded".into())));
                };
                let audio = resample_16k(&mono, rate);
                drop(mono);
                reply.send(VoiceReply::Finished(transcribe(&ctx, &audio, &lang).map(clip)));
            });
        }

        fn cancel(&mut self) {
            self.mic = None;
            Voice::set_level(0.0);
        }

        fn device_name(&self) -> Option<String> {
            let device = cpal::default_host().default_input_device()?;
            device.description().ok().map(|d| d.name().to_string())
        }
    }

    fn transcribe(ctx: &WhisperContext, audio: &[f32], lang: &str) -> Result<String, String> {
        let mut state = ctx.create_state().map_err(|e| format!("speech recognition failed: {e}"))?;
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(Some(lang));
        params.set_translate(false);
        params.set_no_context(true);
        params.set_suppress_blank(true);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        let threads = std::thread::available_parallelism().map_or(2, |n| n.get().min(4));
        params.set_n_threads(threads as i32);
        state.full(params, audio).map_err(|e| format!("speech recognition failed: {e}"))?;
        let segments: Vec<String> =
            state.as_iter().map(|s| s.to_str_lossy().map(|c| c.into_owned()).unwrap_or_default()).collect();
        Ok(clean_transcript(segments.iter().map(String::as_str)))
    }
}

#[cfg(all(test, feature = "voice"))]
mod tests {
    use super::*;

    #[test]
    fn languages() {
        assert_eq!(whisper_lang("en-US"), "en");
        assert_eq!(whisper_lang("de_DE.UTF-8"), "de");
        assert_eq!(whisper_lang("nb-NO"), "no");
        assert_eq!(whisper_lang("iw"), "he");
        assert!(whisper_rs::get_lang_id("de").is_some());
        assert!(whisper_rs::get_lang_id("xx").is_none());
    }

    #[test]
    fn mono_and_16k() {
        assert_eq!(to_mono(&[1.0, 0.0, 0.5, 0.5], 2), vec![0.5, 0.5]);
        assert_eq!(to_mono(&[0.25, 0.5], 1), vec![0.25, 0.5]);
        // 48 kHz → 16 kHz: every 3 samples become their mean.
        let x: Vec<f32> = (0..48_000).map(|i| (i % 3) as f32).collect();
        let y = resample_16k(&x, 48_000);
        assert_eq!(y.len(), 16_000);
        assert!(y.iter().all(|v| (*v - 1.0).abs() < 1e-6));
        // 8 kHz → 16 kHz: twice as many, interpolated.
        let up = resample_16k(&[0.0, 1.0, 0.0], 8_000);
        assert_eq!(up.len(), 6);
        assert!((up[1] - 0.5).abs() < 1e-6 && (up[2] - 1.0).abs() < 1e-6);
        // 44.1 kHz keeps the length ratio.
        assert_eq!(resample_16k(&vec![0.1; 44_100], 44_100).len(), 16_000);
        assert_eq!(resample_16k(&[0.3; 10], 16_000), vec![0.3; 10]);
        assert!(resample_16k(&[], 48_000).is_empty());
        // The peak survives for the silence rule.
        let loud = resample_16k(&[0.0, 0.9, 0.0, 0.0], 16_000);
        assert!((context_capsule_core::Voice::peak(&loud) - 0.9).abs() < 1e-6);
    }

    #[test]
    fn transcript_tags_dropped() {
        assert_eq!(
            clean_transcript([" We keep annual.", "[BLANK_AUDIO]", " (music) ", " Ship it. "]),
            "We keep annual. Ship it."
        );
        assert_eq!(clean_transcript(["[BLANK_AUDIO]"]), "");
    }
}
