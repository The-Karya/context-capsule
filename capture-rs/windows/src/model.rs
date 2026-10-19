// context-capsule — windows: the on-device speech model (whisper.cpp "base", multilingual) — where it lives,
// how it is checked, and the one-time download the user agrees to first.
//! The file is verified by size and SHA-256 before use; a partial or damaged download is never loaded.

use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};

pub const FILE_NAME: &str = "ggml-base.bin";
pub const URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin";
pub const SIZE: u64 = 147_951_465;
pub const SHA256: &str = "60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe";

/// `<data dir>/models/ggml-base.bin`
pub fn path(root: &Path) -> PathBuf {
    root.join("models").join(FILE_NAME)
}

/// Where a download is written until it checks out.
pub fn partial_path(root: &Path) -> PathBuf {
    root.join("models").join(format!("{FILE_NAME}.part"))
}

/// "148 MB", for the consent dialog.
pub fn size_label(bytes: u64) -> String {
    format!("{} MB", (bytes + 500_000) / 1_000_000)
}

/// Lowercase hex SHA-256 of everything `reader` gives.
pub fn sha256_hex(mut reader: impl Read) -> std::io::Result<String> {
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex(&hasher.finalize()))
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The file is exactly `size` bytes with this SHA-256 (size first: cheap, and catches partial files).
pub fn verify(path: &Path, size: u64, sha256: &str) -> bool {
    match std::fs::metadata(path) {
        Ok(m) if m.is_file() && m.len() == size => {}
        _ => return false,
    }
    std::fs::File::open(path)
        .and_then(|f| sha256_hex(std::io::BufReader::new(f)))
        .is_ok_and(|h| h.eq_ignore_ascii_case(sha256))
}

/// The consent dialog: what, how big, once, and that audio stays here.
pub fn consent_text(dir: &Path) -> String {
    format!(
        "Voice notes are transcribed on this PC by the Whisper speech model (whisper.cpp, \"base\", multilingual).\n\n\
         It is a one-time download of about {} from huggingface.co, saved in:\n{}\n\n\
         Your voice is never sent anywhere and no audio is saved. Download the model now?",
        size_label(SIZE),
        dir.display()
    )
}

/// What a voice note says until the model is there.
pub fn missing_text(voice_hotkey: &str, dir: &Path) -> String {
    format!(
        "voice notes need the speech model (one-time download, about {}). Press {voice_hotkey} while recording \
         and choose Yes, or put {FILE_NAME} in {}",
        size_label(SIZE),
        dir.display()
    )
}

#[cfg(windows)]
pub use download::download;

#[cfg(windows)]
mod download {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};
    use windows::core::{w, HSTRING};
    use windows::Win32::Networking::WinInet::{
        HttpQueryInfoW, InternetCloseHandle, InternetOpenUrlW, InternetOpenW, InternetReadFile, HTTP_QUERY_FLAG_NUMBER,
        HTTP_QUERY_STATUS_CODE, INTERNET_FLAG_NO_CACHE_WRITE, INTERNET_FLAG_NO_COOKIES, INTERNET_FLAG_RELOAD,
        INTERNET_FLAG_SECURE, INTERNET_OPEN_TYPE_PRECONFIG,
    };

    /// Closes a WinINet handle when dropped.
    struct Handle(*mut core::ffi::c_void);
    impl Drop for Handle {
        fn drop(&mut self) {
            // SAFETY: a handle WinINet gave us, closed once.
            unsafe {
                let _ = InternetCloseHandle(self.0);
            }
        }
    }

    /// Download the model to `<models>/…part`, check size and SHA-256 while it streams, then move it in
    /// place. `done` counts bytes for progress. Blocking: call from a background thread.
    pub fn download(root: &Path, done: &AtomicU64) -> Result<PathBuf, String> {
        let final_path = path(root);
        let part = partial_path(root);
        std::fs::create_dir_all(part.parent().unwrap_or(root)).map_err(|e| format!("cannot create the models folder: {e}"))?;
        let result = fetch(&part, done);
        if let Err(e) = result {
            let _ = std::fs::remove_file(&part);
            return Err(e);
        }
        std::fs::rename(&part, &final_path).map_err(|e| format!("cannot save the model: {e}"))?;
        Ok(final_path)
    }

    fn fetch(part: &Path, done: &AtomicU64) -> Result<(), String> {
        // SAFETY: plain WinINet calls; every handle is closed by `Handle`.
        let session = unsafe { InternetOpenW(w!("Context Capsule"), INTERNET_OPEN_TYPE_PRECONFIG.0, None, None, 0) };
        if session.is_null() {
            return Err("no internet connection".into());
        }
        let session = Handle(session);
        let flags = INTERNET_FLAG_RELOAD | INTERNET_FLAG_NO_CACHE_WRITE | INTERNET_FLAG_NO_COOKIES | INTERNET_FLAG_SECURE;
        // SAFETY: as above.
        let url = unsafe { InternetOpenUrlW(session.0, &HSTRING::from(URL), None, flags, None) };
        if url.is_null() {
            return Err("cannot reach huggingface.co".into());
        }
        let url = Handle(url);

        let mut status = 0u32;
        let mut len = std::mem::size_of::<u32>() as u32;
        // SAFETY: `status` is a u32 buffer of `len` bytes.
        let ok = unsafe {
            HttpQueryInfoW(
                url.0,
                HTTP_QUERY_STATUS_CODE | HTTP_QUERY_FLAG_NUMBER,
                Some(&mut status as *mut u32 as *mut core::ffi::c_void),
                &mut len,
                None,
            )
        };
        if ok.is_err() || status != 200 {
            return Err(format!("the download server answered {status}"));
        }

        let mut file = std::fs::File::create(part).map_err(|e| format!("cannot write the model: {e}"))?;
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; 1 << 16];
        let mut total = 0u64;
        loop {
            let mut read = 0u32;
            // SAFETY: `buf` has `buf.len()` writable bytes.
            let ok = unsafe { InternetReadFile(url.0, buf.as_mut_ptr() as *mut core::ffi::c_void, buf.len() as u32, &mut read) };
            if ok.is_err() {
                return Err("the download was interrupted".into());
            }
            if read == 0 {
                break;
            }
            let chunk = &buf[..read as usize];
            hasher.update(chunk);
            file.write_all(chunk).map_err(|e| format!("cannot write the model: {e}"))?;
            total += read as u64;
            done.store(total, Ordering::Relaxed);
            if total > SIZE {
                return Err("the download is larger than expected".into());
            }
        }
        file.sync_all().map_err(|e| format!("cannot write the model: {e}"))?;
        if total != SIZE || !hex(&hasher.finalize()).eq_ignore_ascii_case(SHA256) {
            return Err("the downloaded file did not check out (size or SHA-256)".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_live_under_models() {
        let root = Path::new("/data/cc");
        assert_eq!(path(root), Path::new("/data/cc/models/ggml-base.bin"));
        assert_eq!(partial_path(root), Path::new("/data/cc/models/ggml-base.bin.part"));
    }

    #[test]
    fn sha256_known_vectors() {
        assert_eq!(sha256_hex(&b""[..]).unwrap(), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(sha256_hex(&b"abc"[..]).unwrap(), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert_eq!(SHA256.len(), 64);
        assert!(SHA256.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn verify_checks_size_then_hash() {
        let dir = std::env::temp_dir().join(format!("cc-model-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("m.bin");
        std::fs::write(&f, b"abc").unwrap();
        let good = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert!(verify(&f, 3, good));
        assert!(verify(&f, 3, &good.to_uppercase()), "hex case does not matter");
        assert!(!verify(&f, 4, good), "wrong size");
        assert!(!verify(&f, 3, &"0".repeat(64)), "wrong hash");
        assert!(!verify(&dir.join("missing.bin"), 3, good), "missing file");
        assert!(!verify(&dir, 0, good), "a folder is not a model");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn user_texts() {
        assert_eq!(size_label(SIZE), "148 MB");
        let dir = Path::new(r"C:\Users\s\.context-capsule\models");
        let c = consent_text(dir);
        assert!(c.contains("148 MB") && c.contains("one-time") && c.contains("models"));
        let m = missing_text("Ctrl+Alt+Shift+V", dir);
        assert!(m.contains("Ctrl+Alt+Shift+V") && m.contains(FILE_NAME));
    }
}
