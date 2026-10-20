// context-capsule — core: updates — the signed feed (latest.json + latest.json.sig), which version is newer,
// the check schedule, the "auto_update" / "update_feed" settings and a checked download. The same contract as
// the Mac app. No network code here: the platform crates bring a `Downloader`.
//! Order, always: verify the signature, then compare versions, then download over https, then check size and
//! SHA-256, and only then hand the file to the platform to install. Any failure keeps the current version.

use crate::AppLog;
use base64::Engine as _;
use sha2::{Digest, Sha256};
use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The release key (raw 32-byte Ed25519 public key, base64). The private half never leaves the release Mac.
pub const PUBLIC_KEY: &str = "WHTBrAjhTOQIG3S15PKYqHHdj5nbon4J/eEam7LrXd4=";
/// Where `latest.json` and `latest.json.sig` live. config.json `"update_feed"` (a base URL) overrides it.
pub const DEFAULT_FEED: &str = "https://github.com/The-Karya/context-capsule/releases/latest/download";
pub const FEED_FILE: &str = "latest.json";
pub const SIG_FILE: &str = "latest.json.sig";

/// The first check waits this long after launch, then once a day.
pub const FIRST_CHECK_AFTER: Duration = Duration::from_secs(60);
pub const CHECK_EVERY: Duration = Duration::from_secs(24 * 60 * 60);
/// After a failed check (no network), try again sooner than a day.
pub const RETRY_AFTER: Duration = Duration::from_secs(60 * 60);

const MAX_FEED_BYTES: u64 = 64 * 1024;
const MAX_SIG_BYTES: u64 = 1024;
/// No release is this big; a feed saying so is wrong.
const MAX_ASSET_BYTES: u64 = 2 * 1024 * 1024 * 1024;

// MARK: words people see (short and plain)

pub const CHECK_LABEL: &str = "Check for updates";
pub const AUTO_LABEL: &str = "Update automatically";
pub const RESTART_NOW: &str = "Restart now";
pub const LATER: &str = "Later";
pub const UP_TO_DATE: &str = "You have the latest version.";
pub const CHECK_FAILED: &str = "Couldn't check for updates. Try again later.";
pub const CHECKING: &str = "Checking for updates…";

/// "Context Capsule 0.2.0 is ready. Restart to update."
pub fn ready_message(version: &str) -> String {
    format!("Context Capsule {version} is ready. Restart to update.")
}

/// When this copy can't update itself (built from source): only say a new version is out.
pub fn available_message(version: &str) -> String {
    format!("A new version is out: Context Capsule {version}.")
}

// MARK: platforms

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Mac,
    Windows,
    Linux,
}

impl Target {
    /// The key in latest.json.
    pub fn key(self) -> &'static str {
        match self {
            Target::Mac => "mac",
            Target::Windows => "windows",
            Target::Linux => "linux",
        }
    }
}

// MARK: errors

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateError {
    /// No network, a 404 (no release yet), a server error.
    Network(String),
    BadSignature,
    BadFeed(String),
    NotHttps,
    WrongSize { expected: u64, got: u64 },
    WrongHash,
    Disk(String),
}

impl std::fmt::Display for UpdateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UpdateError::Network(e) => write!(f, "network: {e}"),
            UpdateError::BadSignature => write!(f, "the feed's signature does not check out"),
            UpdateError::BadFeed(e) => write!(f, "the feed is not valid: {e}"),
            UpdateError::NotHttps => write!(f, "the download is not https"),
            UpdateError::WrongSize { expected, got } => write!(f, "the download has the wrong size ({got}, expected {expected})"),
            UpdateError::WrongHash => write!(f, "the download's SHA-256 does not match"),
            UpdateError::Disk(e) => write!(f, "disk: {e}"),
        }
    }
}

// MARK: versions

/// A semantic version: 1.2.3 or 1.2.3-beta.1 (build metadata after `+` is ignored).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pub pre: Vec<String>,
}

impl Version {
    /// "0.2.0", "v0.2.0", "1.0.0-rc.1". None for anything else.
    pub fn parse(s: &str) -> Option<Version> {
        let s = s.trim();
        let s = s.strip_prefix('v').unwrap_or(s);
        let s = s.split('+').next()?;
        let (core, pre) = match s.split_once('-') {
            Some((c, p)) => (c, Some(p)),
            None => (s, None),
        };
        let nums: Vec<u64> = core
            .split('.')
            .map(|p| if !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()) { p.parse().ok() } else { None })
            .collect::<Option<_>>()?;
        let [major, minor, patch] = nums[..] else { return None };
        let pre = match pre {
            Some(p) => {
                let ids: Vec<String> = p.split('.').map(str::to_string).collect();
                if ids.iter().any(|i| i.is_empty() || !i.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')) {
                    return None;
                }
                ids
            }
            None => vec![],
        };
        Some(Version { major, minor, patch, pre })
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if !self.pre.is_empty() {
            write!(f, "-{}", self.pre.join("."))?;
        }
        Ok(())
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.major, self.minor, self.patch).cmp(&(other.major, other.minor, other.patch)).then_with(|| {
            // A pre-release comes before its release; identifiers compare numerically when both are numbers.
            match (self.pre.is_empty(), other.pre.is_empty()) {
                (true, true) => Ordering::Equal,
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                (false, false) => {
                    for (a, b) in self.pre.iter().zip(&other.pre) {
                        let o = match (a.parse::<u64>(), b.parse::<u64>()) {
                            (Ok(x), Ok(y)) => x.cmp(&y),
                            (Ok(_), Err(_)) => Ordering::Less,
                            (Err(_), Ok(_)) => Ordering::Greater,
                            (Err(_), Err(_)) => a.cmp(b),
                        };
                        if o != Ordering::Equal {
                            return o;
                        }
                    }
                    self.pre.len().cmp(&other.pre.len())
                }
            }
        })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Is `candidate` newer than `current`? Anything that doesn't parse is not newer.
pub fn is_newer(candidate: &str, current: &str) -> bool {
    match (Version::parse(candidate), Version::parse(current)) {
        (Some(a), Some(b)) => a > b,
        _ => false,
    }
}

// MARK: the feed

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Asset {
    pub url: String,
    /// Lowercase hex.
    pub sha256: String,
    pub size: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    pub version: String,
    pub date: String,
    pub notes: String,
    pub mac: Option<Asset>,
    pub windows: Option<Asset>,
    pub linux: Option<Asset>,
}

impl Release {
    pub fn asset(&self, target: Target) -> Option<&Asset> {
        match target {
            Target::Mac => self.mac.as_ref(),
            Target::Windows => self.windows.as_ref(),
            Target::Linux => self.linux.as_ref(),
        }
    }
}

/// A newer version for this platform.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Offer {
    pub version: String,
    pub notes: String,
    pub asset: Asset,
}

/// The release key, decoded.
pub fn public_key() -> [u8; 32] {
    decode_key(PUBLIC_KEY).expect("the built-in release key is valid")
}

/// A base64 raw 32-byte key.
pub fn decode_key(b64: &str) -> Option<[u8; 32]> {
    base64::engine::general_purpose::STANDARD.decode(b64.trim()).ok()?.try_into().ok()
}

/// `sig` is base64 of the 64-byte Ed25519 signature over exactly `data`.
pub fn verify_signature(data: &[u8], sig: &[u8], key: &[u8; 32]) -> Result<(), UpdateError> {
    let text = std::str::from_utf8(sig).map_err(|_| UpdateError::BadSignature)?;
    let raw = base64::engine::general_purpose::STANDARD.decode(text.trim()).map_err(|_| UpdateError::BadSignature)?;
    let raw: [u8; 64] = raw.try_into().map_err(|_| UpdateError::BadSignature)?;
    let key = ed25519_dalek::VerifyingKey::from_bytes(key).map_err(|_| UpdateError::BadSignature)?;
    key.verify_strict(data, &ed25519_dalek::Signature::from_bytes(&raw)).map_err(|_| UpdateError::BadSignature)
}

/// Parse latest.json (after its signature checked out). A platform entry that is broken counts as missing.
pub fn parse_release(data: &[u8]) -> Result<Release, UpdateError> {
    let v: serde_json::Value = serde_json::from_slice(data).map_err(|e| UpdateError::BadFeed(e.to_string()))?;
    let obj = v.as_object().ok_or_else(|| UpdateError::BadFeed("not an object".into()))?;
    let text = |k: &str| obj.get(k).and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    let version = text("version");
    if Version::parse(&version).is_none() {
        return Err(UpdateError::BadFeed(format!("version {version:?}")));
    }
    let asset = |k: &str| -> Option<Asset> {
        let a = obj.get(k)?.as_object()?;
        let url = a.get("url")?.as_str()?.trim().to_string();
        let sha256 = a.get("sha256")?.as_str()?.trim().to_lowercase();
        let size = a.get("size")?.as_u64()?;
        let hex_ok = sha256.len() == 64 && sha256.bytes().all(|b| b.is_ascii_hexdigit());
        (hex_ok && !url.is_empty() && size > 0 && size <= MAX_ASSET_BYTES).then_some(Asset { url, sha256, size })
    };
    Ok(Release {
        version,
        date: text("date"),
        notes: text("notes"),
        mac: asset("mac"),
        windows: asset("windows"),
        linux: asset("linux"),
    })
}

/// The whole decision from the two downloaded files: signature first, then the version (only newer), then
/// this platform's entry. Ok(None): nothing to do.
pub fn offer(feed: &[u8], sig: &[u8], key: &[u8; 32], current: &str, target: Target) -> Result<Option<Offer>, UpdateError> {
    verify_signature(feed, sig, key)?;
    let release = parse_release(feed)?;
    if !is_newer(&release.version, current) {
        return Ok(None);
    }
    let Some(asset) = release.asset(target) else { return Ok(None) };
    if !is_https(&asset.url) {
        return Err(UpdateError::NotHttps);
    }
    Ok(Some(Offer { version: release.version.clone(), notes: release.notes.clone(), asset: asset.clone() }))
}

pub fn is_https(url: &str) -> bool {
    url.get(..8).is_some_and(|s| s.eq_ignore_ascii_case("https://")) && url.len() > 8
}

/// `<base>/latest.json` and `<base>/latest.json.sig`.
pub fn feed_urls(base: &str) -> (String, String) {
    let base = base.trim().trim_end_matches('/');
    (format!("{base}/{FEED_FILE}"), format!("{base}/{SIG_FILE}"))
}

// MARK: files

pub fn sha256_hex(mut reader: impl std::io::Read) -> std::io::Result<String> {
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// The file has exactly `size` bytes and this SHA-256 (size first: cheap, and catches partial files).
pub fn verify_file(path: &Path, size: u64, sha256: &str) -> Result<(), UpdateError> {
    let got = std::fs::metadata(path).map_err(|e| UpdateError::Disk(e.to_string()))?.len();
    if got != size {
        return Err(UpdateError::WrongSize { expected: size, got });
    }
    let f = std::fs::File::open(path).map_err(|e| UpdateError::Disk(e.to_string()))?;
    let hash = sha256_hex(std::io::BufReader::new(f)).map_err(|e| UpdateError::Disk(e.to_string()))?;
    if hash.eq_ignore_ascii_case(sha256) {
        Ok(())
    } else {
        Err(UpdateError::WrongHash)
    }
}

/// Fetches files over https. The platform crates bring the real one; tests bring a fake.
pub trait Downloader: Send + Sync {
    /// The whole body, at most `max_bytes`. Err for no network, a 404 or any other failure.
    fn get(&self, url: &str, max_bytes: u64) -> Result<Vec<u8>, String>;

    /// Stream the body into `dest` (created or replaced), at most `max_bytes`.
    fn save(&self, url: &str, dest: &Path, max_bytes: u64) -> Result<(), String> {
        let body = self.get(url, max_bytes)?;
        std::fs::write(dest, body).map_err(|e| e.to_string())
    }
}

/// Download `offer` to `<dest>.part`, check it, then move it to `dest`. A file that doesn't check out is deleted.
pub fn download(dl: &dyn Downloader, offer: &Offer, dest: &Path) -> Result<(), UpdateError> {
    let a = &offer.asset;
    if !is_https(&a.url) {
        return Err(UpdateError::NotHttps);
    }
    let part = part_path(dest);
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir).map_err(|e| UpdateError::Disk(e.to_string()))?;
    }
    let result = dl
        .save(&a.url, &part, a.size)
        .map_err(UpdateError::Network)
        .and_then(|()| verify_file(&part, a.size, &a.sha256))
        .and_then(|()| std::fs::rename(&part, dest).map_err(|e| UpdateError::Disk(e.to_string())));
    if result.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    result
}

pub fn part_path(dest: &Path) -> PathBuf {
    let mut name = dest.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(".part");
    dest.with_file_name(name)
}

// MARK: settings in config.json

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    /// "auto_update": check by itself and get the new version ready (default true).
    pub auto: bool,
    /// "update_feed": the base URL of latest.json (default `DEFAULT_FEED`).
    pub feed: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { auto: true, feed: DEFAULT_FEED.into() }
    }
}

impl Settings {
    pub fn load(config: &Path) -> Settings {
        std::fs::read(config).map(|d| Settings::parse(&d)).unwrap_or_default()
    }

    pub fn parse(data: &[u8]) -> Settings {
        let Ok(serde_json::Value::Object(obj)) = serde_json::from_slice::<serde_json::Value>(data) else {
            return Settings::default();
        };
        let auto = obj.get("auto_update").and_then(|v| v.as_bool()).unwrap_or(true);
        let feed = obj
            .get("update_feed")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|f| is_https(f))
            .unwrap_or(DEFAULT_FEED)
            .to_string();
        Settings { auto, feed }
    }
}

/// Turn "Update automatically" on or off in config.json, keeping every other key. A broken config.json is left
/// alone (Err), so a typo never wipes someone's settings.
pub fn set_auto_update(config: &Path, on: bool) -> std::io::Result<()> {
    let mut obj = match std::fs::read(config) {
        Ok(d) => match serde_json::from_slice::<serde_json::Value>(&d) {
            Ok(serde_json::Value::Object(o)) => o,
            _ => return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "config.json is not a JSON object")),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => serde_json::Map::new(),
        Err(e) => return Err(e),
    };
    obj.insert("auto_update".into(), serde_json::Value::Bool(on));
    let text = serde_json::to_string_pretty(&serde_json::Value::Object(obj)).map_err(std::io::Error::other)? + "\n";
    crate::json::write_atomically(config, &text)
}

// MARK: when to check

/// Launch + 1 minute, then daily (an hour after a failure); never while busy (contextualizing or a voice note).
#[derive(Clone, Debug)]
pub struct Schedule {
    launched: Instant,
    next: Option<Instant>,
}

impl Schedule {
    pub fn new(launched: Instant) -> Schedule {
        Schedule { launched, next: None }
    }

    pub fn due(&self, now: Instant, busy: bool) -> bool {
        !busy && now >= self.next.unwrap_or(self.launched + FIRST_CHECK_AFTER)
    }

    /// A check ran at `now`; `ok` false = it failed (try again sooner).
    pub fn checked(&mut self, now: Instant, ok: bool) {
        self.next = Some(now + if ok { CHECK_EVERY } else { RETRY_AFTER });
    }
}

// MARK: the updater

/// A new version, downloaded and checked, waiting to be installed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ready {
    pub version: String,
    pub notes: String,
    pub file: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    UpToDate,
    Ready(Ready),
    /// Newer, but this copy can't install it (built from source): just say so.
    Available { version: String, notes: String },
    Failed(UpdateError),
}

impl Outcome {
    /// What to tell someone who clicked "Check for updates".
    pub fn message(&self) -> String {
        match self {
            Outcome::UpToDate => UP_TO_DATE.into(),
            Outcome::Ready(r) => ready_message(&r.version),
            Outcome::Available { version, .. } => available_message(version),
            Outcome::Failed(_) => CHECK_FAILED.into(),
        }
    }
}

struct State {
    schedule: Schedule,
    ready: Option<Ready>,
    /// The version we last told about by ourselves (an automatic check tells once per version per run).
    told: Option<String>,
}

/// Checks the feed and gets new versions ready. One per app; shared between the timer thread and the menu.
pub struct Updater {
    current: String,
    target: Target,
    key: [u8; 32],
    config: PathBuf,
    /// Where the checked new file goes; None = this copy can't update itself.
    staging: Option<PathBuf>,
    downloader: Box<dyn Downloader>,
    log: AppLog,
    state: Mutex<State>,
    /// One check at a time.
    busy_checking: Mutex<()>,
}

impl Updater {
    pub fn new(
        current: &str,
        target: Target,
        config: PathBuf,
        staging: Option<PathBuf>,
        downloader: Box<dyn Downloader>,
        log: AppLog,
    ) -> Updater {
        Updater {
            current: current.into(),
            target,
            key: public_key(),
            config,
            staging,
            downloader,
            log,
            state: Mutex::new(State { schedule: Schedule::new(Instant::now()), ready: None, told: None }),
            busy_checking: Mutex::new(()),
        }
    }

    /// Another release key (tests).
    pub fn with_key(mut self, key: [u8; 32]) -> Updater {
        self.key = key;
        self
    }

    pub fn current(&self) -> &str {
        &self.current
    }

    pub fn can_install(&self) -> bool {
        self.staging.is_some()
    }

    pub fn auto_update(&self) -> bool {
        Settings::load(&self.config).auto
    }

    pub fn set_auto_update(&self, on: bool) -> std::io::Result<()> {
        set_auto_update(&self.config, on)
    }

    /// The version waiting to be installed, if any.
    pub fn ready(&self) -> Option<Ready> {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).ready.clone()
    }

    /// Time for an automatic check? (Update automatically is on, the schedule says so, and not busy.)
    pub fn due(&self, now: Instant, busy: bool) -> bool {
        self.auto_update() && self.state.lock().unwrap_or_else(|e| e.into_inner()).schedule.due(now, busy)
    }

    /// Check now (blocking: a few small requests, and the download when there is a new version).
    pub fn check(&self) -> Outcome {
        let _one = self.busy_checking.lock().unwrap_or_else(|e| e.into_inner());
        let outcome = self.check_inner();
        let ok = !matches!(outcome, Outcome::Failed(_));
        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        st.schedule.checked(Instant::now(), ok);
        if let Outcome::Ready(r) = &outcome {
            st.ready = Some(r.clone());
        }
        drop(st);
        match &outcome {
            Outcome::UpToDate => self.log.write("update: up to date"),
            Outcome::Ready(r) => self.log.write(&format!("update: {} is ready", r.version)),
            Outcome::Available { version, .. } => self.log.write(&format!("update: {version} is out (this copy can't update itself)")),
            Outcome::Failed(e) => self.log.write(&format!("update: check failed: {e}")),
        }
        outcome
    }

    fn check_inner(&self) -> Outcome {
        let settings = Settings::load(&self.config);
        let (feed_url, sig_url) = feed_urls(&settings.feed);
        let feed = match self.downloader.get(&feed_url, MAX_FEED_BYTES) {
            Ok(f) => f,
            Err(e) => return Outcome::Failed(UpdateError::Network(e)),
        };
        let sig = match self.downloader.get(&sig_url, MAX_SIG_BYTES) {
            Ok(s) => s,
            Err(e) => return Outcome::Failed(UpdateError::Network(e)),
        };
        let offer = match offer(&feed, &sig, &self.key, &self.current, self.target) {
            Ok(Some(o)) => o,
            Ok(None) => return Outcome::UpToDate,
            Err(e) => return Outcome::Failed(e),
        };
        let Some(dest) = &self.staging else {
            return Outcome::Available { version: offer.version, notes: offer.notes };
        };
        let ready = Ready { version: offer.version.clone(), notes: offer.notes.clone(), file: dest.clone() };
        // Already here from an earlier check (this run or the last)?
        if verify_file(dest, offer.asset.size, &offer.asset.sha256).is_ok() {
            return Outcome::Ready(ready);
        }
        match download(self.downloader.as_ref(), &offer, dest) {
            Ok(()) => Outcome::Ready(ready),
            Err(e) => Outcome::Failed(e),
        }
    }

    /// An automatic check found something: tell about it? Once per version per run.
    pub fn should_tell(&self, outcome: &Outcome) -> bool {
        let version = match outcome {
            Outcome::Ready(r) => &r.version,
            Outcome::Available { version, .. } => version,
            _ => return false,
        };
        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if st.told.as_deref() == Some(version.as_str()) {
            return false;
        }
        st.told = Some(version.clone());
        true
    }

    /// The automatic checks on a thread of their own: every 30 seconds, see if one is due (`busy` = contextualizing
    /// or a voice note: wait). `found` hears about a ready or available version (once per version).
    pub fn spawn(self: &Arc<Self>, busy: impl Fn() -> bool + Send + 'static, found: impl Fn(Outcome) + Send + 'static) {
        let me = self.clone();
        let _ = std::thread::Builder::new().name("updates".into()).spawn(move || loop {
            std::thread::sleep(Duration::from_secs(30));
            if !me.due(Instant::now(), busy()) {
                continue;
            }
            let outcome = me.check();
            if me.should_tell(&outcome) {
                found(outcome);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use std::collections::HashMap;

    fn test_key() -> SigningKey {
        // A throwaway key made for the test (not the release key).
        let seed: [u8; 32] = Sha256::digest(b"context-capsule update test key").into();
        SigningKey::from_bytes(&seed)
    }

    fn sign(key: &SigningKey, data: &[u8]) -> Vec<u8> {
        base64::engine::general_purpose::STANDARD.encode(key.sign(data).to_bytes()).into_bytes()
    }

    fn feed(version: &str, linux: Option<(&str, &str, u64)>) -> Vec<u8> {
        let mut v = serde_json::json!({ "version": version, "date": "2026-10-21", "notes": "Faster and smaller." });
        if let Some((url, sha, size)) = linux {
            v["linux"] = serde_json::json!({ "url": url, "sha256": sha, "size": size });
        }
        serde_json::to_vec_pretty(&v).unwrap()
    }

    fn hex_of(data: &[u8]) -> String {
        sha256_hex(data).unwrap()
    }

    #[test]
    fn release_key_decodes() {
        assert_eq!(public_key().len(), 32);
        assert!(ed25519_dalek::VerifyingKey::from_bytes(&public_key()).is_ok());
        assert!(decode_key("not base64!").is_none());
        assert!(decode_key("AAAA").is_none(), "wrong length");
    }

    #[test]
    fn signature() {
        let k = test_key();
        let pk = k.verifying_key().to_bytes();
        let data = feed("0.2.0", None);
        let sig = sign(&k, &data);
        assert_eq!(verify_signature(&data, &sig, &pk), Ok(()));
        let mut with_newline = sig.clone();
        with_newline.push(b'\n');
        assert_eq!(verify_signature(&data, &with_newline, &pk), Ok(()), "a trailing newline in the .sig is fine");
        let mut changed = data.clone();
        changed[5] ^= 1;
        assert_eq!(verify_signature(&changed, &sig, &pk), Err(UpdateError::BadSignature), "one byte changed");
        assert_eq!(verify_signature(&data, &sig, &public_key()), Err(UpdateError::BadSignature), "another key");
        assert_eq!(verify_signature(&data, b"garbage", &pk), Err(UpdateError::BadSignature));
        assert_eq!(verify_signature(&data, b"", &pk), Err(UpdateError::BadSignature));
    }

    #[test]
    fn versions() {
        assert!(is_newer("0.2.0", "0.1.0"));
        assert!(is_newer("0.10.0", "0.9.9"), "numbers, not text");
        assert!(is_newer("1.0.0", "0.99.99"));
        assert!(is_newer("v0.1.1", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.1.0"), "same");
        assert!(!is_newer("0.0.9", "0.1.0"), "older");
        assert!(is_newer("1.0.0", "1.0.0-rc.1"), "a release beats its pre-release");
        assert!(!is_newer("1.0.0-rc.1", "1.0.0"));
        assert!(is_newer("1.0.0-rc.2", "1.0.0-rc.1"));
        assert!(is_newer("1.0.0-rc.10", "1.0.0-rc.9"));
        assert!(is_newer("1.0.0-beta", "1.0.0-alpha"));
        assert!(is_newer("1.0.0-alpha.1", "1.0.0-alpha"));
        assert!(!is_newer("garbage", "0.1.0"));
        assert!(!is_newer("1.2", "0.1.0"), "needs three numbers");
        assert!(!is_newer("1.2.3.4", "0.1.0"));
        assert_eq!(Version::parse("1.2.3+build.5").unwrap().to_string(), "1.2.3");
        assert_eq!(Version::parse("1.2.3-rc.1").unwrap().to_string(), "1.2.3-rc.1");
        assert!(Version::parse("1.2.x").is_none());
        assert!(Version::parse("1..3").is_none());
    }

    #[test]
    fn feed_parsing() {
        let sha = "A".repeat(64);
        let data = feed("0.2.0", Some(("https://x/a.AppImage", &sha, 10)));
        let r = parse_release(&data).unwrap();
        assert_eq!(r.version, "0.2.0");
        assert_eq!(r.notes, "Faster and smaller.");
        assert_eq!(r.linux.as_ref().unwrap().sha256, "a".repeat(64), "hex is lowercased");
        assert!(r.windows.is_none() && r.mac.is_none(), "missing platforms");
        // Broken entries count as missing.
        for bad in [
            r#"{"version":"0.2.0","linux":{"url":"https://x","sha256":"abc","size":10}}"#,
            r#"{"version":"0.2.0","linux":{"url":"https://x","sha256":"0000000000000000000000000000000000000000000000000000000000000000","size":0}}"#,
            r#"{"version":"0.2.0","linux":{"sha256":"0000000000000000000000000000000000000000000000000000000000000000","size":3}}"#,
            r#"{"version":"0.2.0","linux":"https://x"}"#,
        ] {
            assert!(parse_release(bad.as_bytes()).unwrap().linux.is_none(), "{bad}");
        }
        assert!(matches!(parse_release(b"[]"), Err(UpdateError::BadFeed(_))));
        assert!(matches!(parse_release(b"{\"version\":\"two\"}"), Err(UpdateError::BadFeed(_))));
        assert!(matches!(parse_release(b"not json"), Err(UpdateError::BadFeed(_))));
    }

    #[test]
    fn offer_rules() {
        let k = test_key();
        let pk = k.verifying_key().to_bytes();
        let sha = "0".repeat(64);
        let data = feed("0.2.0", Some(("https://x/a.AppImage", &sha, 10)));
        let sig = sign(&k, &data);
        let o = offer(&data, &sig, &pk, "0.1.0", Target::Linux).unwrap().unwrap();
        assert_eq!((o.version.as_str(), o.asset.size), ("0.2.0", 10));
        assert_eq!(offer(&data, &sig, &pk, "0.2.0", Target::Linux), Ok(None), "not newer");
        assert_eq!(offer(&data, &sig, &pk, "0.3.0", Target::Linux), Ok(None), "older");
        assert_eq!(offer(&data, &sig, &pk, "0.1.0", Target::Windows), Ok(None), "no Windows entry");
        // The signature is checked first, before anything else is believed.
        assert_eq!(offer(b"not json", b"x", &pk, "0.1.0", Target::Linux), Err(UpdateError::BadSignature));
        let http = feed("0.2.0", Some(("http://x/a.AppImage", &sha, 10)));
        assert_eq!(offer(&http, &sign(&k, &http), &pk, "0.1.0", Target::Linux), Err(UpdateError::NotHttps));
    }

    #[test]
    fn urls_and_targets() {
        assert_eq!(
            feed_urls(DEFAULT_FEED).0,
            "https://github.com/The-Karya/context-capsule/releases/latest/download/latest.json"
        );
        assert_eq!(feed_urls("https://h/x/").1, "https://h/x/latest.json.sig");
        assert!(is_https("HTTPS://h/a") && !is_https("http://h/a") && !is_https("https://"));
        assert_eq!((Target::Mac.key(), Target::Windows.key(), Target::Linux.key()), ("mac", "windows", "linux"));
        assert_eq!(ready_message("0.2.0"), "Context Capsule 0.2.0 is ready. Restart to update.");
        assert_eq!(part_path(Path::new("/a/b.exe")), Path::new("/a/b.exe.part"));
    }

    #[test]
    fn settings() {
        assert_eq!(Settings::parse(b"{}"), Settings::default());
        assert!(Settings::default().auto);
        let s = Settings::parse(br#"{"auto_update": false, "update_feed": "https://example.com/feed"}"#);
        assert_eq!((s.auto, s.feed.as_str()), (false, "https://example.com/feed"));
        assert_eq!(Settings::parse(br#"{"update_feed": "http://insecure"}"#).feed, DEFAULT_FEED, "https only");
        assert_eq!(Settings::parse(b"broken"), Settings::default());

        let dir = tmp("settings");
        let config = dir.join("config.json");
        set_auto_update(&config, false).unwrap();
        assert!(!Settings::load(&config).auto, "made the file");
        std::fs::write(&config, r#"{"sounds": false, "auto_update": false}"#).unwrap();
        set_auto_update(&config, true).unwrap();
        let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&config).unwrap()).unwrap();
        assert_eq!(v["sounds"], false, "other keys stay");
        assert_eq!(v["auto_update"], true);
        std::fs::write(&config, "{oops").unwrap();
        assert!(set_auto_update(&config, false).is_err(), "a broken file is left alone");
        assert_eq!(std::fs::read_to_string(&config).unwrap(), "{oops");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn schedule() {
        let t0 = Instant::now();
        let mut s = Schedule::new(t0);
        assert!(!s.due(t0, false), "not right at launch");
        assert!(!s.due(t0 + Duration::from_secs(59), false));
        assert!(s.due(t0 + FIRST_CHECK_AFTER, false), "a minute after launch");
        assert!(!s.due(t0 + FIRST_CHECK_AFTER, true), "never while busy");
        let t1 = t0 + FIRST_CHECK_AFTER;
        s.checked(t1, true);
        assert!(!s.due(t1 + Duration::from_secs(3600), false));
        assert!(s.due(t1 + CHECK_EVERY, false), "then daily");
        assert!(!s.due(t1 + CHECK_EVERY, true));
        s.checked(t1, false);
        assert!(s.due(t1 + RETRY_AFTER, false), "sooner after a failure");
    }

    /// Serves files from memory; counts requests.
    #[derive(Default)]
    struct Fake {
        files: HashMap<String, Vec<u8>>,
        hits: Arc<Mutex<Vec<String>>>,
    }

    impl Downloader for Fake {
        fn get(&self, url: &str, max_bytes: u64) -> Result<Vec<u8>, String> {
            self.hits.lock().unwrap().push(url.to_string());
            let body = self.files.get(url).cloned().ok_or_else(|| "404".to_string())?;
            if body.len() as u64 > max_bytes {
                return Err("too big".into());
            }
            Ok(body)
        }
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("cc-update-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn served(k: &SigningKey, version: &str, asset_url: &str, body: &[u8], claimed_sha: Option<&str>) -> Fake {
        let sha = claimed_sha.map(str::to_string).unwrap_or_else(|| hex_of(body));
        let data = feed(version, Some((asset_url, &sha, body.len() as u64)));
        let (f, s) = feed_urls(DEFAULT_FEED);
        let mut fake = Fake::default();
        fake.files.insert(s, sign(k, &data));
        fake.files.insert(f, data);
        fake.files.insert(asset_url.into(), body.to_vec());
        fake
    }

    fn updater(fake: Fake, dir: &Path, staging: bool, key: &SigningKey) -> Updater {
        Updater::new(
            "0.1.0",
            Target::Linux,
            dir.join("config.json"),
            staging.then(|| dir.join("new.AppImage")),
            Box::new(fake),
            AppLog::stderr_only(),
        )
        .with_key(key.verifying_key().to_bytes())
    }

    #[test]
    fn check_downloads_and_verifies() {
        let k = test_key();
        let dir = tmp("check");
        let body = b"the new app".to_vec();
        let u = updater(served(&k, "0.2.0", "https://dl/new", &body, None), &dir, true, &k);
        let Outcome::Ready(r) = u.check() else { panic!("expected ready") };
        assert_eq!(r.version, "0.2.0");
        assert_eq!(std::fs::read(&r.file).unwrap(), body);
        assert!(!part_path(&r.file).exists());
        assert_eq!(u.ready(), Some(r.clone()));
        assert!(u.should_tell(&Outcome::Ready(r.clone())));
        assert!(!u.should_tell(&Outcome::Ready(r)), "once per version");
        // A second check finds the file already there: no second download.
        let Outcome::Ready(_) = u.check() else { panic!() };
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_bad_download_is_never_kept() {
        let k = test_key();
        let dir = tmp("bad");
        let u = updater(served(&k, "0.2.0", "https://dl/new", b"tampered", Some(&"1".repeat(64))), &dir, true, &k);
        assert_eq!(u.check(), Outcome::Failed(UpdateError::WrongHash));
        assert!(!dir.join("new.AppImage").exists() && !part_path(&dir.join("new.AppImage")).exists());
        assert_eq!(u.ready(), None);
        assert_eq!(u.check().message(), CHECK_FAILED);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn signed_by_someone_else_downloads_nothing() {
        let k = test_key();
        let other = SigningKey::from_bytes(&[7u8; 32]);
        let dir = tmp("other");
        let fake = served(&other, "0.2.0", "https://dl/new", b"x", None);
        let hits = fake.hits.clone();
        let u = updater(fake, &dir, true, &k);
        assert_eq!(u.check(), Outcome::Failed(UpdateError::BadSignature));
        assert!(!hits.lock().unwrap().iter().any(|h| h == "https://dl/new"), "only the feed and its signature were fetched");
        assert!(!dir.join("new.AppImage").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn outcomes() {
        let k = test_key();
        let dir = tmp("outcomes");
        // Up to date.
        let u = updater(served(&k, "0.1.0", "https://dl/new", b"x", None), &dir, true, &k);
        assert_eq!(u.check(), Outcome::UpToDate);
        assert_eq!(u.check().message(), UP_TO_DATE);
        // Can't install here: only say it's out.
        let u = updater(served(&k, "0.3.0", "https://dl/new", b"x", None), &dir, false, &k);
        assert!(!u.can_install());
        let o = u.check();
        assert_eq!(o.message(), "A new version is out: Context Capsule 0.3.0.");
        // No network / no release yet (404).
        let u = updater(Fake::default(), &dir, true, &k);
        assert!(matches!(u.check(), Outcome::Failed(UpdateError::Network(_))));
        // The setting turns automatic checks off.
        set_auto_update(&dir.join("config.json"), false).unwrap();
        assert!(!u.due(Instant::now() + CHECK_EVERY, false));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
