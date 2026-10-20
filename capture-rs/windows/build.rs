// context-capsule — windows: build script — asks the linker to embed an app manifest that turns on the
// modern look for standard controls (Common Controls 6: Windows 10/11 buttons in the setup window and
// dialogs), and puts the Claude extension (mcp/dist-ext/context-capsule.mcpb, built by
// mcp/extension/build.sh) inside the exe, so one download is the whole app. Without the file the exe embeds
// nothing and expects the extension next to it, as before.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let windows_msvc = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    if windows_msvc {
        println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg-bins=/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' \
             version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'"
        );
    }

    let manifest = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let mcpb = manifest.join("../../mcp/dist-ext/context-capsule.mcpb");
    println!("cargo:rerun-if-changed={}", mcpb.display());
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR")).join("context-capsule.mcpb");
    let data = std::fs::read(&mcpb).unwrap_or_default();
    if data.is_empty() && windows_msvc && std::env::var("PROFILE").as_deref() == Ok("release") {
        println!("cargo:warning=mcp/dist-ext/context-capsule.mcpb is missing: run mcp/extension/build.sh first, or the exe has no Claude extension inside");
    }
    std::fs::write(&out, data).expect("write OUT_DIR/context-capsule.mcpb");
}
