//! The fixture's Builds frame (#1205): one pipeline and one run of it, so
//! that every live read crosses the boundary with something in it.
//!
//! * **The pipeline** `pipeline-1`, *Crane telemetry*, and its **run**
//!   `run-1`, number 42 on `main`, failed. `older_runs` answers one older
//!   run, `run-0`, past the window the sync keeps.
//! * **The run's log** is [`LOG`], two steps; a range read answers its slice.
//! * **Its tests** are two, one failed; **its artifacts** a file and a folder.
//! * **Its view** has a section drawn at once and two deferred ones: the
//!   change it built, as a diff, and the tree it built, whose file opens in
//!   the same tab (the code-host diff and tree reads).
//! * **A download** streams 64 KiB chunks into `download-sink`; the whole
//!   artifact set is `download_mib` MiB of them (config, default 1), which
//!   the component never holds more than one chunk of.

use knobas_adapter_sdk::spi::KindInfo;
use knobas_adapter_sdk::spi::role::{CI, RoleClaim};
use knobas_adapter_sdk::spi::{Frame, KindPaths, PayloadPath, StatePath, StateValue, Tone};
use serde_json::{Value, json};

pub const PIPELINE: &str = "pipeline-1";
pub const RUN: &str = "run-1";
pub const OLDER_RUN: &str = "run-0";

/// The run's log: `(step, text, tone)` per line.
pub const LOG: [(u32, &str, Option<&str>); 6] = [
    (0, "checkout quay-telemetry at 4be1c09", None),
    (0, "checked out", None),
    (1, "cargo test --workspace", None),
    (1, "test crane::hoist::brake_wear ... ok", None),
    (1, "test crane::hoist::brake_limit ... FAILED", Some("err")),
    (1, "error: 1 test failed", Some("err")),
];

/// The bytes one download write carries.
#[cfg_attr(feature = "v1_0", allow(dead_code))]
pub const CHUNK: usize = 64 * 1024;

/// The pipeline's and the run's payloads, as the sync emits them.
pub fn pipeline_payload() -> Value {
    json!({ "key": PIPELINE, "id": PIPELINE, "name": "Crane telemetry" })
}

pub fn run_payload(key: &str, number: &str) -> Value {
    json!({
        "key": key,
        "pipeline": PIPELINE,
        "state": "failed",
        "number": number,
        "branch": "main",
        "started": "2026-10-18T09:00:00Z",
        "finished": "2026-10-18T09:04:00Z",
    })
}

/// The kinds the descriptor declares for the frame.
pub fn kinds() -> [KindInfo; 2] {
    let kind = |id: &str, label: &str, plural: &str, monogram: &str| KindInfo {
        id: id.into(),
        label: label.into(),
        plural: plural.into(),
        monogram: monogram.into(),
        full_sync_exhaustive: true,
    };
    [
        kind("pipeline", "Pipeline", "Pipelines", "PL"),
        kind("run", "Run", "Runs", "RN"),
    ]
}

/// Where the frame's kinds keep what the Builds frame reads.
pub fn payload_paths() -> [KindPaths; 2] {
    let at = |key: &str| vec![PayloadPath::of([key])];
    [
        KindPaths {
            kind: "pipeline".into(),
            frame: Some(Frame::Pipeline),
            name: at("name"),
            pipeline: at("id"),
            ..KindPaths::default()
        },
        KindPaths {
            kind: "run".into(),
            frame: Some(Frame::Run),
            pipeline: at("pipeline"),
            number: at("number"),
            branch: at("branch"),
            started: at("started"),
            finished: at("finished"),
            state: vec![StatePath {
                at: PayloadPath::of(["state"]),
                values: vec![
                    StateValue::of("failed", Tone::Err),
                    StateValue::of("passed", Tone::Ok),
                ],
            }],
            ..KindPaths::default()
        },
    ]
}

/// The CI claim: `run_log` and every optional read.
pub fn role() -> RoleClaim {
    RoleClaim::new(
        CI,
        [
            "run_log",
            "pipeline_settings",
            "older_runs",
            "run_tests",
            "test_history",
            "run_artifacts",
            "artifact_preview",
            "download",
            "run_chain",
        ],
    )
}

pub fn pipeline_settings() -> Value {
    json!({
        "repositories": [], "triggers": [], "parameters": [], "parameter_write_op": null,
        "steps": ["Checkout", "Test"],
    })
}

/// `count` lines from line `from` (from 1), or the last `count`.
pub fn run_log(from: Option<u64>, count: u32) -> Value {
    let total = LOG.len() as u64;
    let count = u64::from(count).min(total);
    let first = from.unwrap_or(total - count + 1).max(1);
    let lines: Vec<Value> = (first..=total.min(first + count - 1))
        .map(|number| {
            let (step, text, tone) = LOG[usize::try_from(number - 1).unwrap_or(0)];
            json!({ "number": number, "time": null, "text": text, "tone": tone, "step": step })
        })
        .collect();
    json!({
        "lines": lines,
        "steps": [
            { "name": "Checkout", "first": 1, "last": 2, "seconds": 3, "tone": null },
            { "name": "Test", "first": 3, "last": 6, "seconds": 237, "tone": "err" },
        ],
        "total": total,
    })
}

pub fn run_tests() -> Value {
    let test = |id: &str, status: &str, details: Option<&str>| {
        json!({ "id": id, "name": id, "module": "crane::hoist", "status": status,
                "muted": false, "new_failure": status == "failed", "duration_ms": 12,
                "details": details })
    };
    json!({ "tests": [
        test("crane::hoist::brake_limit", "failed", Some("assertion failed: wear < limit")),
        test("crane::hoist::brake_wear", "passed", None),
    ], "truncated": false })
}

pub fn test_history() -> Value {
    json!([
        { "run": RUN, "number": "42", "branch": "main", "status": "failed" },
        { "run": OLDER_RUN, "number": "41", "branch": "main", "status": "passed" },
    ])
}

/// One folder of the artifacts, `""` for the root; `None` for no folder.
pub fn run_artifacts(path: &str) -> Option<Value> {
    match path {
        "" => Some(json!([
            { "path": "reports", "name": "reports", "size": null, "folder": true, "children": 1 },
            { "path": "crane.txt", "name": "crane.txt", "size": 21, "folder": false,
              "children": null },
        ])),
        "reports" => Some(json!([
            { "path": "reports/summary.txt", "name": "summary.txt", "size": 18, "folder": false,
              "children": null },
        ])),
        _ => None,
    }
}

/// An artifact file's text.
pub fn artifact_text(path: &str) -> Option<&'static str> {
    match path {
        "crane.txt" => Some("brake wear: 0.81 mm\n"),
        "reports/summary.txt" => Some("1 failed, 1 passed"),
        _ => None,
    }
}

/// The run's view: a summary drawn at once, and the change and the tree,
/// deferred. `section` names one; `None` for a section it does not have.
pub fn view(section: Option<&str>) -> Option<Value> {
    let summary = json!({ "id": "summary", "title": "Run 42", "count": null, "deferred": false,
        "blocks": [{ "type": "facts", "facts": [
            { "label": "Branch", "value": "main", "tone": null },
        ] }] });
    let deferred = |id: &str, title: &str| json!({ "id": id, "title": title, "count": 1, "deferred": true, "blocks": [] });
    let one = |id: &str, title: &str, block: Value| {
        json!({ "sections": [{ "id": id, "title": title, "count": 1, "deferred": false,
            "blocks": [block] }], "actions": [] })
    };
    match section {
        None => Some(json!({
            "sections": [summary, deferred("changes", "Changes"), deferred("files", "Files")],
            "actions": [],
        })),
        Some("summary") => Some(json!({ "sections": [summary], "actions": [] })),
        Some("changes") => Some(one(
            "changes",
            "Changes",
            json!({
                "type": "diff", "truncated": false, "files": [{
                    "path": "src/hoist.rs", "old_path": null, "change": "modified",
                    "additions": 1, "deletions": 1, "truncated": false,
                    "hunks": [{ "header": "@@ -7,1 +7,1 @@", "lines": [
                        { "kind": "del", "old": 7, "new": null, "text": "const LIMIT: f32 = 0.9;" },
                        { "kind": "add", "old": null, "new": 7, "text": "const LIMIT: f32 = 0.8;" },
                    ] }],
                }],
            }),
        )),
        Some("files") => Some(one(
            "files",
            "Files",
            json!({
                "type": "tree", "truncated": false, "url": null,
                "trail": [{ "name": "quay-telemetry", "section": "files" }],
                "entries": [{ "name": "README.md", "kind": "file", "size": 15,
                              "section": "files:README.md" }],
            }),
        )),
        Some("files:README.md") => Some(one(
            "files:README.md",
            "README.md",
            json!({
                "type": "file", "path": "README.md", "text": "# Crane telemetry", "language":
                "markdown", "size": 15, "truncated": false, "binary": false, "url": null,
                "sha": null,
                "trail": [{ "name": "quay-telemetry", "section": "files" },
                          { "name": "README.md", "section": "files:README.md" }],
            }),
        )),
        Some(_) => None,
    }
}
