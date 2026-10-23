// context-capsule — core: build script — embeds the shared window page (ui/ at the repo root, without dev/ and
// tests/) so the Windows and Linux apps serve it from inside the program, never from disk or the network.

use std::path::{Path, PathBuf};

fn walk(dir: &Path, root: &Path, out: &mut Vec<(String, PathBuf)>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let path = e.path();
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let rel = path.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
        if path.is_dir() {
            if rel == "dev" || rel == "tests" {
                continue;
            }
            walk(&path, root, out);
        } else if [".html", ".css", ".js", ".svg", ".png", ".woff2", ".json"].iter().any(|x| name.ends_with(x)) {
            out.push((rel, path));
        }
    }
}

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let ui = manifest.join("../../ui");
    println!("cargo:rerun-if-changed={}", ui.display());
    let mut files = vec![];
    walk(&ui, &ui, &mut files);
    for (_, p) in &files {
        println!("cargo:rerun-if-changed={}", p.display());
    }
    let mut code = String::from("/// The page's files: (path relative to ui/, bytes).\npub static FILES: &[(&str, &[u8])] = &[\n");
    for (rel, path) in &files {
        code.push_str(&format!("    ({rel:?}, include_bytes!({:?})),\n", path.display().to_string()));
    }
    code.push_str("];\n");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("ui_files.rs");
    std::fs::write(out, code).unwrap();
}
