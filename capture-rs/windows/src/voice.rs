// context-capsule — windows: voice notes, fully on this PC — the mic through cpal (open only between the
// two presses) and whisper.cpp for the words. Audio stays in memory and is never written anywhere.
//! The speech model is downloaded once, after the user agrees (see `model.rs`).

/// whisper.cpp wants mono f32 at 16 kHz.
pub const WHISPER_RATE: u32 = 16_000;

/// Interleaved frames → mono (the mean of the channels).
pub fn downmix(interleaved: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return interleaved.to_vec();
    }
    interleaved.chunks_exact(channels).map(|f| f.iter().sum::<f32>() / channels as f32).collect()
}

/// Resample mono audio. Down: the mean of each output sample's span (a box filter, so little
/// aliasing); up: linear interpolation.
pub fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || from == 0 || to == 0 || input.is_empty() {
        return input.to_vec();
    }
    let (from_n, to_n) = (from as usize, to as usize);
    let n = input.len() * to_n / from_n;
    if from > to {
        (0..n)
            .map(|i| {
                let a = i * from_n / to_n;
                let b = ((i + 1) * from_n / to_n).clamp(a + 1, input.len());
                input[a..b].iter().sum::<f32>() / (b - a) as f32
            })
            .collect()
    } else {
        let ratio = from as f64 / to as f64;
        (0..n)
            .map(|i| {
                let pos = i as f64 * ratio;
                let j = pos.floor() as usize;
                let frac = (pos - j as f64) as f32;
                let a = input[j.min(input.len() - 1)];
                let b = *input.get(j + 1).unwrap_or(&a);
                a + (b - a) * frac
            })
            .collect()
    }
}

/// The whisper language code for a locale ("en-US" → "en", "pt_BR" → "pt", "nb-NO" → "no").
pub fn whisper_lang(locale: &str) -> Option<String> {
    let code = locale.trim().split(['-', '_', '.', '@']).next()?.to_ascii_lowercase();
    if !(2..=3).contains(&code.len()) || !code.chars().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    let code = match code.as_str() {
        "nb" | "nn" => "no",
        "iw" => "he",
        "in" => "id",
        "ji" => "yi",
        "fil" => "tl",
        other => other,
    };
    Some(code.to_string())
}

/// Windows' microphone privacy switches (all apps, and desktop apps) as stored in the registry.
pub fn mic_denied(all_apps: Option<&str>, desktop_apps: Option<&str>) -> bool {
    [all_apps, desktop_apps].iter().any(|v| v.is_some_and(|v| v.eq_ignore_ascii_case("deny")))
}

/// whisper's segments → the note. Markers it makes up for silence or noise ("[BLANK_AUDIO]",
/// "(music)") are dropped.
pub fn clean_transcript<S: AsRef<str>>(segments: &[S]) -> String {
    let kept: Vec<&str> = segments
        .iter()
        .map(|s| s.as_ref().trim())
        .filter(|s| {
            let marker = (s.starts_with('[') && s.ends_with(']')) || (s.starts_with('(') && s.ends_with(')'));
            !s.is_empty() && !marker
        })
        .collect();
    kept.join(" ")
}

#[cfg(windows)]
pub use builder::{engine, Notify};

#[cfg(windows)]
mod builder {
    use std::path::Path;
    use std::sync::Arc;

    /// Shows a toast from any thread (for the download finishing).
    pub type Notify = Arc<dyn Fn(String) + Send + Sync>;

    /// The voice engine for this build.
    pub fn engine(root: &Path, voice_hotkey: &'static str, notify: Notify) -> Box<dyn context_capsule_core::VoiceEngine> {
        #[cfg(feature = "voice")]
        return Box::new(super::whisper::WhisperVoice::new(root, voice_hotkey, notify));
        #[cfg(not(feature = "voice"))]
        {
            let _ = (root, voice_hotkey, notify);
            Box::new(NoVoice)
        }
    }

    /// Built without the `voice` feature.
    #[cfg(not(feature = "voice"))]
    struct NoVoice;

    #[cfg(not(feature = "voice"))]
    impl context_capsule_core::VoiceEngine for NoVoice {
        fn prepare(&mut self, _locale: &str, reply: context_capsule_core::Replier) {
            let why = "this build has no voice notes (built without the \"voice\" feature)".to_string();
            reply.send(context_capsule_core::VoiceReply::Prepared(Some(context_capsule_core::VoiceOutcome::Failed(why))));
        }
        fn start(&mut self) -> Result<(), String> {
            Err("voice notes are not in this build".into())
        }
        fn finish(&mut self, reply: context_capsule_core::Replier) {
            reply.send(context_capsule_core::VoiceReply::Finished(Err("voice notes are not in this build".into())));
        }
        fn cancel(&mut self) {}
        fn device_name(&self) -> Option<String> {
            None
        }
    }
}

#[cfg(all(windows, feature = "voice"))]
mod whisper {
    use super::builder::Notify;
    use super::*;
    use crate::{model, win};
    use context_capsule_core::{Replier, Voice, VoiceClip, VoiceEngine, VoiceOutcome, VoicePermission, VoiceReply};
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use cpal::{FromSample, SampleFormat, SizedSample};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};
    use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

    /// Give up on a transcription that takes longer than this.
    const MAX_TRANSCRIBE: Duration = Duration::from_secs(60);
    const CONSENT: &str = r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone";

    enum ModelState {
        Missing,
        Asking,
        Downloading(Arc<AtomicU64>),
        Loading,
        Ready(Arc<WhisperContext>),
    }

    /// What the mic gave so far (mono, at the device rate).
    #[derive(Default)]
    struct Clip {
        samples: Vec<f32>,
        rate: u32,
        peak: f32,
    }

    pub struct WhisperVoice {
        root: PathBuf,
        voice_hotkey: &'static str,
        notify: Notify,
        model: Arc<Mutex<ModelState>>,
        lang: String,
        stream: Option<cpal::Stream>,
        clip: Arc<Mutex<Clip>>,
    }

    impl WhisperVoice {
        pub fn new(root: &Path, voice_hotkey: &'static str, notify: Notify) -> WhisperVoice {
            whisper_rs::install_logging_hooks(); // whisper.cpp prints nothing (and never the words)
            WhisperVoice {
                root: root.to_path_buf(),
                voice_hotkey,
                notify,
                model: Arc::new(Mutex::new(ModelState::Missing)),
                lang: "en".into(),
                stream: None,
                clip: Arc::new(Mutex::new(Clip::default())),
            }
        }

        fn models_dir(&self) -> PathBuf {
            model::path(&self.root).parent().map(Path::to_path_buf).unwrap_or_else(|| self.root.clone())
        }

        /// Ready → None. Otherwise load it, or ask to download it; the reply comes when that is decided.
        fn prepare_model(&mut self, reply: Replier) {
            let mut state = self.model.lock().unwrap_or_else(|e| e.into_inner());
            match &*state {
                ModelState::Ready(_) => reply.send(VoiceReply::Prepared(None)),
                ModelState::Loading => reply.send(failed("the speech model is loading, try again in a moment")),
                ModelState::Asking => reply.send(failed("answer the speech model question first")),
                ModelState::Downloading(done) => {
                    let pct = done.load(Ordering::Relaxed) * 100 / model::SIZE;
                    reply.send(failed(&format!("the speech model is still downloading ({pct}%)")));
                }
                ModelState::Missing if model::path(&self.root).is_file() => {
                    *state = ModelState::Loading;
                    let (shared, path, hotkey) = (self.model.clone(), model::path(&self.root), self.voice_hotkey);
                    std::thread::spawn(move || {
                        let loaded = load(&path);
                        let mut state = shared.lock().unwrap_or_else(|e| e.into_inner());
                        match loaded {
                            Ok(ctx) => {
                                *state = ModelState::Ready(ctx);
                                reply.send(VoiceReply::Prepared(None));
                            }
                            Err(e) => {
                                *state = ModelState::Missing;
                                reply.send(failed(&format!("{e}. Press {hotkey} again to download it")));
                            }
                        }
                    });
                }
                ModelState::Missing => {
                    *state = ModelState::Asking;
                    let (shared, root, dir, hotkey, notify) =
                        (self.model.clone(), self.root.clone(), self.models_dir(), self.voice_hotkey, self.notify.clone());
                    std::thread::spawn(move || ask_and_download(shared, &root, &dir, hotkey, notify, reply));
                }
            }
        }
    }

    fn failed(why: &str) -> VoiceReply {
        VoiceReply::Prepared(Some(VoiceOutcome::Failed(why.to_string())))
    }

    /// Verify, then load. A file that does not check out is removed (it is downloaded again next time).
    fn load(path: &Path) -> Result<Arc<WhisperContext>, String> {
        if !model::verify(path, model::SIZE, model::SHA256) {
            let _ = std::fs::remove_file(path);
            return Err("the speech model file was damaged and has been removed".into());
        }
        WhisperContext::new_with_params(path, WhisperContextParameters::default())
            .map(Arc::new)
            .map_err(|e| format!("cannot load the speech model ({e})"))
    }

    /// The consent dialog; on yes, the one-time download, then load and say voice notes are ready.
    fn ask_and_download(shared: Arc<Mutex<ModelState>>, root: &Path, dir: &Path, hotkey: &str, notify: Notify, reply: Replier) {
        let set = |s: ModelState| *shared.lock().unwrap_or_else(|e| e.into_inner()) = s;
        if !win::ask_yes_no("Download the speech model?", &model::consent_text(dir)) {
            set(ModelState::Missing);
            return reply.send(failed(&model::missing_text(hotkey, dir)));
        }
        let done = Arc::new(AtomicU64::new(0));
        set(ModelState::Downloading(done.clone()));
        reply.send(failed(&format!(
            "downloading the speech model ({}). Voice notes will work when it is done",
            model::size_label(model::SIZE)
        )));
        eprintln!("Context Capsule: downloading the speech model");
        let result = model::download(root, &done).and_then(|path| load(&path));
        match result {
            Ok(ctx) => {
                set(ModelState::Ready(ctx));
                notify(format!("Voice notes are ready. Press {hotkey} while recording."));
            }
            Err(e) => {
                set(ModelState::Missing);
                eprintln!("Context Capsule: speech model download failed: {e}");
                notify(format!("Speech model download failed: {e}. Press {hotkey} to try again."));
            }
        }
    }

    fn input_device() -> Option<cpal::Device> {
        cpal::default_host().default_input_device()
    }

    /// Build the input stream for one sample format: mono, peak tracked, kept in memory.
    fn build<T>(device: &cpal::Device, config: cpal::StreamConfig, clip: Arc<Mutex<Clip>>) -> Result<cpal::Stream, cpal::Error>
    where
        T: SizedSample,
        f32: FromSample<T>,
    {
        let channels = config.channels.max(1) as usize;
        let cap = config.sample_rate as usize * (Voice::MAX_DURATION.as_secs() as usize + 10);
        device.build_input_stream(
            config,
            move |data: &[T], _: &cpal::InputCallbackInfo| {
                let samples: Vec<f32> = data.iter().map(|s| s.to_sample::<f32>()).collect();
                let mono = downmix(&samples, channels);
                let mut clip = clip.lock().unwrap_or_else(|e| e.into_inner());
                clip.peak = clip.peak.max(Voice::peak(&mono));
                let room = cap.saturating_sub(clip.samples.len());
                clip.samples.extend_from_slice(&mono[..mono.len().min(room)]);
            },
            |e| eprintln!("Context Capsule: microphone error: {e}"),
            Some(Duration::from_secs(5)),
        )
    }

    fn transcribe(ctx: &WhisperContext, lang: &str, audio: &[f32]) -> Result<String, String> {
        let mut state = ctx.create_state().map_err(|e| format!("speech recognition could not start ({e})"))?;
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(1, 8);
        params.set_n_threads(threads as i32);
        params.set_language(Some(lang));
        params.set_translate(false);
        params.set_no_context(true);
        params.set_no_timestamps(true);
        params.set_suppress_blank(true);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        let started = Instant::now();
        params.set_abort_callback_safe(move || started.elapsed() > MAX_TRANSCRIBE);
        state.full(params, audio).map_err(|e| format!("speech recognition failed ({e})"))?;
        let segments: Vec<String> =
            state.as_iter().map(|s| s.to_str_lossy().map(|t| t.into_owned()).unwrap_or_default()).collect();
        Ok(clean_transcript(&segments))
    }

    impl VoiceEngine for WhisperVoice {
        fn prepare(&mut self, locale: &str, reply: Replier) {
            let (all, desktop) = (win::reg_string(CONSENT, "Value"), win::reg_string(&format!(r"{CONSENT}\NonPackaged"), "Value"));
            if mic_denied(all.as_deref(), desktop.as_deref()) {
                return reply.send(VoiceReply::Prepared(Some(VoiceOutcome::Denied(VoicePermission::Microphone))));
            }
            let Some(lang) = whisper_lang(locale).filter(|l| whisper_rs::get_lang_id(l).is_some()) else {
                return reply.send(VoiceReply::Prepared(Some(VoiceOutcome::Unavailable { locale: locale.to_string() })));
            };
            self.lang = lang;
            if input_device().is_none() {
                return reply.send(failed("no microphone found. Check Settings → System → Sound → Input"));
            }
            self.prepare_model(reply);
        }

        fn start(&mut self) -> Result<(), String> {
            let device = input_device().ok_or("no microphone found")?;
            let supported = device.default_input_config().map_err(|e| format!("the microphone could not start ({e})"))?;
            let config = supported.config();
            *self.clip.lock().unwrap_or_else(|e| e.into_inner()) = Clip { samples: vec![], rate: config.sample_rate, peak: 0.0 };
            let clip = self.clip.clone();
            let stream = match supported.sample_format() {
                SampleFormat::F32 => build::<f32>(&device, config, clip),
                SampleFormat::I16 => build::<i16>(&device, config, clip),
                SampleFormat::U16 => build::<u16>(&device, config, clip),
                SampleFormat::I32 => build::<i32>(&device, config, clip),
                other => return Err(format!("the microphone uses an unsupported format ({other})")),
            }
            .map_err(|e| match e.kind() {
                cpal::ErrorKind::PermissionDenied => {
                    "the microphone is blocked. Turn it on in Settings → Privacy & security → Microphone".to_string()
                }
                _ => format!("the microphone could not start ({e})"),
            })?;
            stream.play().map_err(|e| format!("the microphone could not start ({e})"))?;
            self.stream = Some(stream);
            Ok(())
        }

        fn finish(&mut self, reply: Replier) {
            self.stream = None; // the mic closes here
            let clip = std::mem::take(&mut *self.clip.lock().unwrap_or_else(|e| e.into_inner()));
            let seconds = if clip.rate > 0 { clip.samples.len() as f64 / clip.rate as f64 } else { 0.0 };
            let ctx = match &*self.model.lock().unwrap_or_else(|e| e.into_inner()) {
                ModelState::Ready(ctx) => Some(ctx.clone()),
                _ => None,
            };
            let lang = self.lang.clone();
            std::thread::spawn(move || {
                let empty = VoiceClip { text: String::new(), peak: clip.peak, seconds };
                // Silent or too short: nothing to recognize.
                if clip.peak < Voice::SILENT_PEAK || seconds < Voice::MIN_SECONDS {
                    return reply.send(VoiceReply::Finished(Ok(empty)));
                }
                let Some(ctx) = ctx else { return reply.send(VoiceReply::Finished(Err("the speech model is not loaded".into()))) };
                let audio = resample(&clip.samples, clip.rate, WHISPER_RATE);
                let result = transcribe(&ctx, &lang, &audio).map(|text| VoiceClip { text, ..empty });
                reply.send(VoiceReply::Finished(result));
            });
        }

        fn cancel(&mut self) {
            self.stream = None;
            *self.clip.lock().unwrap_or_else(|e| e.into_inner()) = Clip::default();
        }

        fn device_name(&self) -> Option<String> {
            input_device()?.description().ok().map(|d| d.name().to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downmix_averages_channels() {
        assert_eq!(downmix(&[1.0, 0.0, 0.5, 0.5], 2), vec![0.5, 0.5]);
        assert_eq!(downmix(&[0.1, 0.2], 1), vec![0.1, 0.2]);
        assert_eq!(downmix(&[1.0, 1.0, 1.0, 9.0], 3), vec![1.0], "a partial frame is dropped");
    }

    #[test]
    fn resample_to_16k() {
        let tone: Vec<f32> = vec![0.25; 48_000];
        let out = resample(&tone, 48_000, WHISPER_RATE);
        assert_eq!(out.len(), 16_000, "one second stays one second");
        assert!(out.iter().all(|s| (s - 0.25).abs() < 1e-6), "a constant stays constant");

        let ramp: Vec<f32> = (0..9).map(|i| i as f32).collect();
        assert_eq!(resample(&ramp, 48_000, 16_000), vec![1.0, 4.0, 7.0], "each output is its span's mean");

        let at441 = resample(&vec![0.5; 44_100], 44_100, WHISPER_RATE);
        assert_eq!(at441.len(), 16_000);

        let up = resample(&[0.0, 1.0], 8_000, 16_000);
        assert_eq!(up, vec![0.0, 0.5, 1.0, 1.0], "linear when going up");
        assert_eq!(resample(&[0.3, 0.4], 16_000, 16_000), vec![0.3, 0.4]);
        assert!(resample(&[], 48_000, 16_000).is_empty());
    }

    #[test]
    fn languages_from_locales() {
        assert_eq!(whisper_lang("en-US").as_deref(), Some("en"));
        assert_eq!(whisper_lang("pt_BR").as_deref(), Some("pt"));
        assert_eq!(whisper_lang("DE").as_deref(), Some("de"));
        assert_eq!(whisper_lang("zh-Hans-CN").as_deref(), Some("zh"));
        assert_eq!(whisper_lang("nb-NO").as_deref(), Some("no"));
        assert_eq!(whisper_lang("fil-PH").as_deref(), Some("tl"));
        assert_eq!(whisper_lang(""), None);
        assert_eq!(whisper_lang("x"), None);
        assert_eq!(whisper_lang("12-34"), None);
    }

    #[test]
    fn mic_privacy() {
        assert!(!mic_denied(Some("Allow"), Some("Allow")));
        assert!(!mic_denied(None, None), "never set: allowed");
        assert!(mic_denied(Some("Deny"), Some("Allow")), "off for all apps");
        assert!(mic_denied(Some("Allow"), Some("Deny")), "off for desktop apps");
    }

    #[test]
    fn transcript_markers_dropped() {
        assert_eq!(clean_transcript(&[" We keep annual.", " [BLANK_AUDIO]", " (music) ", " Then ship."]), "We keep annual. Then ship.");
        assert_eq!(clean_transcript(&["[BLANK_AUDIO]"]), "");
        assert_eq!(clean_transcript::<&str>(&[]), "");
    }
}
