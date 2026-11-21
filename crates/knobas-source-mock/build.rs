//! Compiles the fixture repositories' files in (the everything-search
//! decisions, 2026-11-20, D16): `fixtures/tidewater/files/<owner>/<name>/…`
//! becomes one table of `(repository, path, bytes)` in `$OUT_DIR/files.rs`,
//! read by `include_bytes!`, so the mock answers its trees and blobs in tests
//! and in the packaged demo with no file beside the binary, as `work.json` is
//! compiled in.

use std::path::{Path, PathBuf};

fn walk(root: &Path, at: &Path, out: &mut Vec<(String, PathBuf)>) {
    let mut entries: Vec<_> = std::fs::read_dir(at)
        .unwrap_or_else(|e| panic!("{}: {e}", at.display()))
        .flatten()
        .collect();
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        let kind = entry.file_type().expect("a file type");
        if kind.is_dir() {
            walk(root, &path, out);
        } else if kind.is_file() {
            let relative = path
                .strip_prefix(root)
                .expect("under the root")
                .to_string_lossy()
                .replace('\\', "/");
            out.push((relative, path));
        }
    }
}

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("the manifest dir"));
    let root = manifest.join("../../fixtures/tidewater/files");
    println!("cargo:rerun-if-changed={}", root.display());
    let mut files = Vec::new();
    walk(&root, &root, &mut files);
    let mut table = String::from("&[\n");
    for (relative, path) in &files {
        // `<owner>/<name>/<path in the repository>`.
        let mut parts = relative.splitn(3, '/');
        let (Some(owner), Some(name), Some(inner)) = (parts.next(), parts.next(), parts.next())
        else {
            panic!("{relative}: a fixture file sits under <owner>/<name>/");
        };
        println!("cargo:rerun-if-changed={}", path.display());
        let absolute = path.canonicalize().expect("a fixture file");
        table.push_str(&format!(
            "    ({:?}, {:?}, {:?}, include_bytes!({:?})),\n",
            owner,
            name,
            inner,
            absolute.to_string_lossy()
        ));
    }
    table.push(']');
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("the out dir")).join("files.rs");
    std::fs::write(out, table).expect("files.rs written");
}
