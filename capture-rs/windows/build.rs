// context-capsule — windows: build script — asks the linker to embed an app manifest that turns on the
// modern look for standard controls (Common Controls 6: Windows 10/11 buttons in the setup window and
// dialogs), and puts two files inside the exe, so one download is the whole app:
// - the Claude extension (mcp/dist-ext/context-capsule.mcpb, built by mcp/extension/build.sh);
// - the any-AI program (mcp/dist-bin/context-capsule-mcp-windows-x64.exe, built by mcp/build-binary.sh),
//   zstd-compressed (about 87 MB → 33 MB), with its SHA-256 and size so the app can check the copy it writes.
// Without a file the exe embeds nothing for it and expects it next to it, as before.

use sha2::{Digest, Sha256};
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let windows_msvc = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    let release = std::env::var("PROFILE").as_deref() == Ok("release");
    if windows_msvc {
        println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg-bins=/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' \
             version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'"
        );
    }

    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));

    let mcpb = manifest.join("../../mcp/dist-ext/context-capsule.mcpb");
    println!("cargo:rerun-if-changed={}", mcpb.display());
    let data = std::fs::read(&mcpb).unwrap_or_default();
    if data.is_empty() && windows_msvc && release {
        println!("cargo:warning=mcp/dist-ext/context-capsule.mcpb is missing: run mcp/extension/build.sh first, or the exe has no Claude extension inside");
    }
    std::fs::write(out_dir.join("context-capsule.mcpb"), data).expect("write OUT_DIR/context-capsule.mcpb");

    // The any-AI program. Only compressed for Windows builds (other hosts run the tests: nothing to embed).
    let program = manifest.join("../../mcp/dist-bin/context-capsule-mcp-windows-x64.exe");
    println!("cargo:rerun-if-changed={}", program.display());
    let raw = if windows_msvc { std::fs::read(&program).unwrap_or_default() } else { Vec::new() };
    if raw.is_empty() && windows_msvc && release {
        println!("cargo:warning=mcp/dist-bin/context-capsule-mcp-windows-x64.exe is missing: run mcp/build-binary.sh windows-x64 first, or Connect your AI and Copy my day won't work");
    }
    let (packed, sha) = if raw.is_empty() {
        (Vec::new(), String::new())
    } else {
        let sha: String = Sha256::digest(&raw).iter().map(|b| format!("{b:02x}")).collect();
        // Level 19 in release (slow, about 30 s, only when the file changed); quicker for debug builds.
        let level = if release { 19 } else { 3 };
        (zstd::bulk::compress(&raw, level).expect("zstd"), sha)
    };
    std::fs::write(out_dir.join("context-capsule-mcp.exe.zst"), packed).expect("write OUT_DIR/context-capsule-mcp.exe.zst");
    println!("cargo:rustc-env=CC_MCP_SHA256={sha}");
    println!("cargo:rustc-env=CC_MCP_SIZE={}", raw.len());
}
