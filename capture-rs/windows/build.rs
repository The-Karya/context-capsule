// context-capsule — windows: build script — asks the linker to embed an app manifest that turns on the
// modern look for standard controls (Common Controls 6: Windows 10/11 buttons in the setup window and
// dialogs), and puts two files inside the exe, so one download is the whole app:
// - the Claude extension (mcp/dist-ext/context-capsule.mcpb, built by mcp/extension/build.sh);
// - the any-AI program (mcp/dist-bin/context-capsule-mcp-windows-x64.exe, built by mcp/build-binary.sh),
//   zstd-compressed (about 87 MB → 33 MB), with its SHA-256 and size so the app can check the copy it writes.
// A release build for Windows stops with an error when either file is missing or empty: the exe is what people
// download, and without them Connect your AI and Copy my day can't work. Debug builds and builds on other hosts
// (tests, clippy) embed nothing; developing, the app finds the repo's mcp/dist-bin and mcp/dist instead.
// Also the app icon (icon.ico, the Karya panther) and the version info ("Karya Context Capsule"), as a .res the
// linker takes in (src/res.rs writes it: no resource compiler needed).

#[allow(dead_code)]
#[path = "src/res.rs"]
mod res;

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

    // The icon and version info.
    println!("cargo:rerun-if-changed=icon.ico");
    println!("cargo:rerun-if-changed=src/res.rs");
    if windows_msvc {
        let ico = std::fs::read(manifest.join("icon.ico")).expect("windows/icon.ico");
        let images = res::parse_ico(&ico).expect("windows/icon.ico is not an icon");
        let version = std::env::var("CARGO_PKG_VERSION").expect("CARGO_PKG_VERSION");
        let info = res::Version { version: &version, name: "Karya Context Capsule", company: "Karya", file_name: "context-capsule.exe" };
        let path = out_dir.join("context-capsule.res");
        std::fs::write(&path, res::res(&images, &info)).expect("write OUT_DIR/context-capsule.res");
        println!("cargo:rustc-link-arg-bins={}", path.display());
    }

    // Both files must be there for a Windows release build.
    let required = windows_msvc && release;
    let mcpb = manifest.join("../../mcp/dist-ext/context-capsule.mcpb");
    println!("cargo:rerun-if-changed={}", mcpb.display());
    let data = read_input(&mcpb, required, "run mcp/extension/build.sh first");
    std::fs::write(out_dir.join("context-capsule.mcpb"), data).expect("write OUT_DIR/context-capsule.mcpb");

    // The any-AI program. Only compressed for Windows builds (other hosts run the tests: nothing to embed).
    let program = manifest.join("../../mcp/dist-bin/context-capsule-mcp-windows-x64.exe");
    println!("cargo:rerun-if-changed={}", program.display());
    let raw = if windows_msvc { read_input(&program, required, "run mcp/build-binary.sh windows-x64 first") } else { Vec::new() };
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

/// The file's bytes (empty when it's missing). `required`: a missing or empty file stops the build instead.
fn read_input(path: &std::path::Path, required: bool, how: &str) -> Vec<u8> {
    let data = std::fs::read(path).unwrap_or_default();
    if data.is_empty() && required {
        panic!(
            "{} is missing or empty: {how}. A Windows release build carries it inside the exe (Connect your AI, Copy my day).",
            path.display()
        );
    }
    data
}
