// context-capsule — linux: the whisper.cpp speech model — where it lives, checking it, and the one-time
// download (only after the user agrees), verified by SHA-256.

use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// The multilingual "base" model (ggml), pinned to one revision so the hash never changes under us.
pub const FILE: &str = "ggml-base.bin";
pub const URL: &str =
    "https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/ggml-base.bin";
pub const SHA256: &str = "60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe";
pub const BYTES: u64 = 147_951_465;

/// `<data dir>/models/ggml-base.bin`.
pub fn path(root: &Path) -> PathBuf {
    root.join("models").join(FILE)
}

/// Present with the right size (the hash is checked when it is loaded).
pub fn is_present(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|m| m.len() == BYTES)
}

pub fn sha256_hex(mut r: impl Read) -> std::io::Result<String> {
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = r.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex(&h.finalize()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The file's SHA-256 is `expected`.
pub fn verify(path: &Path, expected: &str) -> Result<(), String> {
    let f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let got = sha256_hex(std::io::BufReader::new(f)).map_err(|e| e.to_string())?;
    if got == expected {
        Ok(())
    } else {
        Err(format!(
            "the speech model in {} is damaged (checksum mismatch); delete it to download it again",
            path.display()
        ))
    }
}

/// Download to `<path>.part`, hashing as it comes, then move it into place only if the hash matches.
pub fn download(path: &Path, mut progress: impl FnMut(u64)) -> Result<(), String> {
    let dir = path.parent().ok_or("bad model path")?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let part = path.with_extension("bin.part");
    let response = ureq::get(URL).call().map_err(|e| format!("download failed: {e}"))?;
    let mut body = response.into_body().into_reader();
    let mut file = std::fs::File::create(&part).map_err(|e| e.to_string())?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    let mut total = 0u64;
    loop {
        let n = body.read(&mut buf).map_err(|e| format!("download failed: {e}"))?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
        file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        total += n as u64;
        progress(total);
    }
    file.sync_all().map_err(|e| e.to_string())?;
    drop(file);
    if hex(&h.finalize()) != SHA256 || total != BYTES {
        let _ = std::fs::remove_file(&part);
        return Err("the downloaded speech model did not match its checksum; nothing was kept".into());
    }
    std::fs::rename(&part, path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_and_hashes() {
        assert_eq!(path(Path::new("/d")), Path::new("/d/models/ggml-base.bin"));
        assert_eq!(
            sha256_hex(&b"abc"[..]).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let dir = std::env::temp_dir().join(format!("cc-model-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join(FILE);
        std::fs::write(&f, b"abc").unwrap();
        assert!(!is_present(&f), "wrong size");
        assert!(verify(&f, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad").is_ok());
        assert!(verify(&f, SHA256).is_err());
        assert!(!is_present(&dir.join("missing.bin")));
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(SHA256.len(), 64);
        assert!(URL.ends_with(FILE));
    }
}
