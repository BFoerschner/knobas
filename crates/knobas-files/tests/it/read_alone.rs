//! D14 point 1: the read alone, over a corpus (`examples/files_corpus.rs`
//! builds one): the spike's thirteen texts typed a key at a time from the
//! third character. Ignored: run with `KNOBAS_FILES_CORPUS=<index dir>` and
//! `--ignored --release`.

use std::time::Instant;

use knobas_files::{FileIndex, Reads, Repo};

const TEXTS: &[&str] = &[
    "adproxyserver",
    "configuration",
    "inetsocketaddress",
    "kafka consumer group",
    "connection refused",
    "settlebridge",
    "authenticationprovider",
    "servletrequest",
    "error handler",
    "sql injection",
    "jwtfilter",
    "testcontainers",
    "test payment retry",
];

#[test]
#[ignore = "needs KNOBAS_FILES_CORPUS, a built corpus index"]
fn the_files_read_alone() {
    let dir =
        std::env::var("KNOBAS_FILES_CORPUS").expect("KNOBAS_FILES_CORPUS names an index folder");
    let index = FileIndex::open(std::path::Path::new(&dir)).expect("the corpus");
    let held = index.held().unwrap();
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
                    },
                )
            })
            .collect(),
    );
    let mut every = Vec::new();
    for text in TEXTS {
        let chars: Vec<char> = text.chars().collect();
        for end in 3..=chars.len() {
            let typed: String = chars[..end].iter().collect();
            if typed.ends_with(' ') {
                continue;
            }
            let best = (0..5)
                .map(|_| {
                    let t0 = Instant::now();
                    let _ = index.ask(&typed, Reads::BOTH, 30);
                    t0.elapsed().as_secs_f64() * 1e3
                })
                .fold(f64::MAX, f64::min);
            every.push((best, typed));
        }
    }
    every.sort_by(|a, b| a.0.total_cmp(&b.0));
    let median = every[every.len() / 2].0;
    let worst = every.last().unwrap();
    println!(
        "{} keys over {} repositories: median {median:.2} ms, worst {:.2} ms (`{}`)",
        every.len(),
        held.len(),
        worst.0,
        worst.1
    );
    assert!(median < 5.0, "median {median:.2} ms");
    assert!(worst.0 < 25.0, "worst {:.2} ms at `{}`", worst.0, worst.1);
}
