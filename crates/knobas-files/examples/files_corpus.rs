//! A corpus for the files index's own measures (the everything-search
//! decisions, D14 point 1), written through [`knobas_files::FileWriter`] with
//! `knobas_core::code_text`'s rules, as the files pass writes.
//!
//! ```sh
//! cargo run --release -p knobas-files --example files_corpus -- build <index dir> <folder>...
//! cargo run --release -p knobas-files --example files_corpus -- keys <index dir> <rounds> <text>...
//! ```
//!
//! `build`: each immediate subfolder of each `<folder>` is one repository,
//! numbered from 1 in the order read; a commit every 2,000 files. `keys`: each
//! text typed a key at a time from its third character, the best of `rounds`
//! a key, as the launcher asks. `KNOBAS_FILES_CORPUS=<index dir>` then runs the
//! ignored `read_alone` test over it.

use std::path::Path;
use std::time::Instant;

use knobas_core::code_text;
use knobas_files::{FileIndex, Reads, Repo};

fn walk(root: &Path, at: &Path, out: &mut Vec<(String, u64)>) {
    let Ok(entries) = std::fs::read_dir(at) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let name = entry.file_name().to_string_lossy().into_owned();
        if kind.is_dir() {
            if !code_text::SKIP_FOLDERS.contains(&name.as_str()) {
                walk(root, &path, out);
            }
        } else if kind.is_file() {
            let relative = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            out.push((relative, entry.metadata().map_or(0, |m| m.len())));
        }
    }
}

fn build(index_dir: &Path, folders: &[String]) {
    let index = FileIndex::open(index_dir).expect("an index");
    let mut writer = index.writer().expect("the writer");
    let started = Instant::now();
    let (mut no, mut files, mut text_files, mut bytes) = (0_u64, 0_u64, 0_u64, 0_u64);
    for folder in folders {
        let mut repos: Vec<_> = std::fs::read_dir(folder)
            .expect("a folder")
            .flatten()
            .filter(|e| e.path().is_dir())
            .map(|e| e.path())
            .collect();
        repos.sort();
        for repo in repos {
            no += 1;
            let mut paths = Vec::new();
            walk(&repo, &repo, &mut paths);
            for (path, size) in paths {
                let read = code_text::refuses_path(&path)
                    .or_else(|| code_text::refuses_size(size))
                    .is_none();
                let text = read
                    .then(|| std::fs::read(repo.join(&path)).ok())
                    .flatten()
                    .filter(|bytes| code_text::refuses_bytes(bytes).is_none())
                    .and_then(|bytes| code_text::decode(&bytes).map(std::borrow::Cow::into_owned));
                if let Some(text) = &text {
                    text_files += 1;
                    bytes += text.len() as u64;
                }
                writer.put(no, &path, text.as_deref()).expect("put");
                files += 1;
                if files % 2_000 == 0 {
                    writer.commit().expect("commit");
                }
            }
        }
    }
    writer.commit().expect("commit");
    println!(
        "build: {no} repositories, {files} files, {text_files} with text, {} MB of text, {:.1} s",
        bytes >> 20,
        started.elapsed().as_secs_f64()
    );
}

/// Every repository the index holds, known by its number.
pub fn known(index: &FileIndex) {
    let held = index.held().expect("held");
    index.set_repos(
        held.keys()
            .map(|no| {
                (
                    *no,
                    Repo {
                        entity_id: format!("corpus:{no}"),
                        source_id: "corpus".to_owned(),
                        favourite: false,
                        updated: 0,
                        branch: "main".to_owned(),
                    },
                )
            })
            .collect(),
    );
}

fn keys(index_dir: &Path, rounds: usize, texts: &[String]) {
    let index = FileIndex::open(index_dir).expect("an index");
    known(&index);
    let mut every = Vec::new();
    for text in texts {
        let chars: Vec<char> = text.chars().collect();
        let mut ms = Vec::new();
        for end in 3..=chars.len() {
            let typed: String = chars[..end].iter().collect();
            if typed.ends_with(' ') {
                continue;
            }
            let best = (0..rounds)
                .map(|_| {
                    let t0 = Instant::now();
                    let _ = index.ask(&typed, Reads::BOTH, 30);
                    t0.elapsed().as_secs_f64() * 1e3
                })
                .fold(f64::MAX, f64::min);
            ms.push(best);
            every.push(best);
        }
        ms.sort_by(f64::total_cmp);
        println!(
            "{text}: {} keys, median {:.2} ms, worst {:.2} ms",
            ms.len(),
            ms[ms.len() / 2],
            ms[ms.len() - 1]
        );
    }
    every.sort_by(f64::total_cmp);
    println!(
        "every key: {}, median {:.2} ms, 90th {:.2} ms, worst {:.2} ms",
        every.len(),
        every[every.len() / 2],
        every[every.len() * 9 / 10],
        every[every.len() - 1]
    );
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("build") if args.len() >= 3 => build(Path::new(&args[1]), &args[2..]),
        Some("keys") if args.len() >= 4 => keys(
            Path::new(&args[1]),
            args[2].parse().expect("rounds"),
            &args[3..],
        ),
        _ => eprintln!(
            "files_corpus build <index dir> <folder>... | keys <index dir> <rounds> <text>..."
        ),
    }
}
