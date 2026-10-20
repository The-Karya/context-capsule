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
        "Voice notes need the speech model (one-time download, about {}). Press {voice_hotkey} while recording \
         and choose Yes, or put {FILE_NAME} in {}.",
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
    use std::sync::atomic::AtomicU64;

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

    /// Writes to the file and hashes on the way.
    struct Hashing {
        file: std::fs::File,
        hasher: Sha256,
    }

    impl Write for Hashing {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            let n = self.file.write(buf)?;
            self.hasher.update(&buf[..n]);
            Ok(n)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            self.file.flush()
        }
    }

    fn fetch(part: &Path, done: &AtomicU64) -> Result<(), String> {
        let file = std::fs::File::create(part).map_err(|e| format!("cannot write the model: {e}"))?;
        let mut out = Hashing { file, hasher: Sha256::new() };
        let total = crate::net::fetch(URL, SIZE, &mut out, Some(done)).map_err(|e| {
            if e.starts_with("cannot reach") { "cannot reach huggingface.co".to_string() } else { e }
        })?;
        out.file.sync_all().map_err(|e| format!("cannot write the model: {e}"))?;
        if total != SIZE || !hex(&out.hasher.finalize()).eq_ignore_ascii_case(SHA256) {
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
        let m = missing_text("Alt+V", dir);
        assert!(m.contains("Alt+V") && m.contains(FILE_NAME));
    }
}
