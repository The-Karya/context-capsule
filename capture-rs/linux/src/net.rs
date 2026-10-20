// context-capsule — linux: https downloads (ureq with rustls) — used by the speech model download and by
// updates. https only, also after redirects.

use std::io::{Read, Write};
use std::path::Path;
use std::time::Duration;

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .https_only(true)
        .timeout_connect(Some(Duration::from_secs(30)))
        .user_agent("Context Capsule")
        .build()
        .into()
}

/// GET `url` into `out`, at most `max_bytes`; `progress` hears the byte count. Returns the number of bytes.
pub fn fetch(url: &str, max_bytes: u64, out: &mut dyn Write, progress: &mut dyn FnMut(u64)) -> Result<u64, String> {
    let response = agent().get(url).call().map_err(|e| format!("download failed: {e}"))?;
    let mut body = response.into_body().into_reader();
    let mut buf = vec![0u8; 1 << 16];
    let mut total = 0u64;
    loop {
        let n = body.read(&mut buf).map_err(|e| format!("download failed: {e}"))?;
        if n == 0 {
            break;
        }
        total += n as u64;
        if total > max_bytes {
            return Err("the download is larger than expected".into());
        }
        out.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        progress(total);
    }
    Ok(total)
}

/// The core's `Downloader` on ureq.
pub struct Https;

impl context_capsule_core::update::Downloader for Https {
    fn get(&self, url: &str, max_bytes: u64) -> Result<Vec<u8>, String> {
        let mut body = Vec::new();
        fetch(url, max_bytes, &mut body, &mut |_| {})?;
        Ok(body)
    }

    fn save(&self, url: &str, dest: &Path, max_bytes: u64) -> Result<(), String> {
        let mut file = std::fs::File::create(dest).map_err(|e| e.to_string())?;
        fetch(url, max_bytes, &mut file, &mut |_| {})?;
        file.sync_all().map_err(|e| e.to_string())
    }
}
