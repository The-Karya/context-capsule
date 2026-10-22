// context-capsule — windows: the any-AI program (SPEC "Any AI"). context-capsule-mcp.exe is inside the exe
// (zstd-compressed by build.rs) and written next to it — the installed folder, or wherever "Just run it" runs —
// when it's missing or different, the same way as the Claude extension. AI apps connected to Context Capsule
// start it from there, so its place stays the same across updates.
//! Pure file work, tested on any OS. Writing out takes a second or two (decompress and check 87 MB): it runs
//! on a thread at startup, and every call that needs the program waits for it.

use context_capsule_core::anyai::{self, Launch};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const FILE_NAME: &str = "context-capsule-mcp.exe";

/// The program, compressed (empty when the build had no mcp/dist-bin/context-capsule-mcp-windows-x64.exe).
static PACKED: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/context-capsule-mcp.exe.zst"));
/// Its SHA-256 (hex) and size, uncompressed.
const SHA256: &str = env!("CC_MCP_SHA256");
const SIZE: &str = env!("CC_MCP_SIZE");

/// What's inside: (compressed bytes, SHA-256 hex, size).
#[derive(Clone, Copy)]
pub struct Embedded<'a> {
    pub packed: &'a [u8],
    pub sha256: &'a str,
    pub size: u64,
}

pub fn embedded() -> Embedded<'static> {
    Embedded { packed: PACKED, sha256: SHA256, size: SIZE.parse().unwrap_or(0) }
}

fn sha256_of(path: &Path) -> Option<String> {
    let mut f = std::fs::File::open(path).ok()?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Some(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// The file at `path` is exactly the embedded program.
pub fn is_current(path: &Path, e: Embedded) -> bool {
    std::fs::metadata(path).is_ok_and(|m| m.len() == e.size) && sha256_of(path).as_deref() == Some(e.sha256)
}

/// Write the embedded program to `path` unless it's already there. The new file is checked before it moves in;
/// a running copy (an AI app has it open) is moved aside to `.old` first, as updates do with the main exe.
/// Ok(false): nothing inside this build.
pub fn write_out(path: &Path, e: Embedded) -> std::io::Result<bool> {
    if e.packed.is_empty() || e.sha256.is_empty() {
        return Ok(false);
    }
    if is_current(path, e) {
        return Ok(true);
    }
    let staged = crate::install::new_path(path);
    let result = (|| {
        let mut dec = ruzstd::decoding::StreamingDecoder::new(e.packed).map_err(std::io::Error::other)?;
        let mut file = std::fs::File::create(&staged)?;
        std::io::copy(&mut dec, &mut file)?;
        file.sync_all()?;
        drop(file);
        if !is_current(&staged, e) {
            return Err(std::io::Error::other("the unpacked program doesn't match"));
        }
        crate::install::replace_exe(&staged, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&staged);
    }
    crate::install::remove_leftovers(path);
    result.map(|()| true)
}

/// Done once per run (Ok or not): the path when the program is there.
static READY: Mutex<Option<Option<PathBuf>>> = Mutex::new(None);

/// The program next to `exe`, written out first if needed (once per run; other callers wait for it).
pub fn ensure(exe: &Path, log: &dyn Fn(&str)) -> Option<PathBuf> {
    let mut ready = READY.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(done) = ready.as_ref() {
        return done.clone();
    }
    let path = exe.with_file_name(FILE_NAME);
    let found = match write_out(&path, embedded()) {
        Ok(true) => Some(path),
        Ok(false) => path.is_file().then_some(path),
        Err(e) => {
            log(&format!("any AI: cannot write {FILE_NAME}: {e}"));
            path.is_file().then_some(path)
        }
    };
    *ready = Some(found.clone());
    found
}

/// How to run the program: next to the exe (written out from inside it), else, when developing, the repo's
/// mcp/dist-bin binary or mcp/dist/index.js with node.
pub fn launch(log: &dyn Fn(&str)) -> Option<Launch> {
    let exe = std::env::current_exe().ok()?;
    let next_to = ensure(&exe, log);
    let cwd = std::env::current_dir().unwrap_or_default();
    let found = anyai::find(
        &next_to.into_iter().collect::<Vec<_>>(),
        &anyai::repo_candidates(&exe, &cwd),
        anyai::node_on_path().as_deref(),
        &|p: &Path| p.is_file(),
    );
    log(&format!("any AI: program {}", found.as_ref().map_or_else(|| "not found".to_string(), |l| l.to_string())));
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_once_and_replaces_a_changed_copy() {
        let dir = std::env::temp_dir().join(format!("cc-program-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let raw: Vec<u8> = (0..300_000u32).flat_map(|i| (i % 251).to_le_bytes()).collect();
        let packed = zstd_like(&raw);
        let sha: String = Sha256::digest(&raw).iter().map(|b| format!("{b:02x}")).collect();
        let e = Embedded { packed: &packed, sha256: &sha, size: raw.len() as u64 };
        let path = dir.join(FILE_NAME);

        assert!(write_out(&path, e).unwrap());
        assert_eq!(std::fs::read(&path).unwrap(), raw);
        let before = std::fs::metadata(&path).unwrap().modified().unwrap();
        assert!(write_out(&path, e).unwrap(), "already there: kept");
        assert_eq!(std::fs::metadata(&path).unwrap().modified().unwrap(), before);

        std::fs::write(&path, b"an older version").unwrap();
        assert!(write_out(&path, e).unwrap());
        assert_eq!(std::fs::read(&path).unwrap(), raw, "a different copy is replaced");
        assert!(!crate::install::old_path(&path).exists() && !crate::install::new_path(&path).exists(), "no leftovers");

        let wrong = Embedded { sha256: "00", ..e };
        std::fs::write(&path, b"old").unwrap();
        assert!(write_out(&path, wrong).is_err(), "a bad unpack never moves in");
        assert_eq!(std::fs::read(&path).unwrap(), b"old");
        assert!(!crate::install::new_path(&path).exists());

        assert!(!write_out(&dir.join("none.exe"), Embedded { packed: &[], sha256: "", size: 0 }).unwrap(), "nothing inside");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A valid zstd frame of `raw` (stored as raw blocks), so the test needs no compressor.
    fn zstd_like(raw: &[u8]) -> Vec<u8> {
        let mut out = vec![0x28, 0xB5, 0x2F, 0xFD];
        // Frame header: single segment off, no checksum, content size absent, window descriptor present.
        out.push(0x00);
        out.push(0x58); // window 2^(10+11) = 2 MiB
        let chunks: Vec<&[u8]> = raw.chunks(128 * 1024 - 1).collect();
        for (i, c) in chunks.iter().enumerate() {
            let last = u32::from(i + 1 == chunks.len());
            let header = last | (c.len() as u32) << 3; // block type 0: raw
            out.extend_from_slice(&header.to_le_bytes()[..3]);
            out.extend_from_slice(c);
        }
        out
    }
}
