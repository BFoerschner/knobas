//! Every live read crosses the boundary (#1205), at the SPI: the fixture's
//! run and pipeline answer each read, its view's deferred sections carry a
//! diff and a tree, and a download streams through a host-owned sink.

use knobas_source::run::{DownloadSink, DownloadTarget, TestStatus};
use knobas_source::view::Block;
use knobas_source::{Source, SourceError};
use knobas_wasm_host::limits::Limits;
use knobas_wasm_host::{INTERFACE_VERSION, Version, WasmAdapter, load_fixture};

const RUN: &str = "surveys:run-1";
const PIPELINE: &str = "surveys:pipeline-1";

fn instance(config: serde_json::Value) -> knobas_source::instance::SourceInstance {
    knobas_source::instance::SourceInstance {
        id: "surveys".to_owned(),
        kind: "wasm-fixture".to_owned(),
        display_name: "Harbour surveys".to_owned(),
        base_url: "https://surveys.example".to_owned(),
        auth: None,
        secret: None,
        account: None,
        config,
    }
}

fn build(adapter: &WasmAdapter, config: serde_json::Value) -> Box<dyn Source> {
    match adapter.build(instance(config)) {
        Ok(source) => source,
        Err(e) => panic!("the fixture must build: {e:?}"),
    }
}

#[tokio::test]
async fn every_run_and_pipeline_read_answers_across_the_boundary() {
    let source = build(&load_fixture(), serde_json::json!({}));

    let log = source.run_log(RUN, Some(3), 2).await.unwrap().unwrap();
    let numbers: Vec<u64> = log.lines.iter().map(|l| l.number).collect();
    assert_eq!((numbers, log.total, log.steps.len()), (vec![3, 4], 6, 2));
    let tail = source.run_log(RUN, None, 2).await.unwrap().unwrap();
    assert_eq!(tail.lines[1].text, "error: 1 test failed");

    let tests = source.run_tests(RUN).await.unwrap().unwrap();
    assert_eq!(tests.tests[0].status, TestStatus::Failed);
    let history = source
        .test_history(RUN, "crane::hoist::brake_limit")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(history.len(), 2);

    let root = source.run_artifacts(RUN, "").await.unwrap().unwrap();
    assert!(root.iter().any(|e| e.folder && e.path == "reports"));
    let folder = source.run_artifacts(RUN, "reports").await.unwrap().unwrap();
    assert_eq!(folder[0].path, "reports/summary.txt");
    let preview = source
        .artifact_preview(RUN, "crane.txt")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(preview.text.as_deref(), Some("brake wear: 0.81 mm\n"));

    let settings = source.pipeline_settings(PIPELINE).await.unwrap().unwrap();
    assert_eq!(settings.steps, ["Checkout", "Test"]);
    let older = source.older_runs(PIPELINE, RUN, 10).await.unwrap().unwrap();
    let older: Vec<String> = older.iter().map(|r| r.entity.to_string()).collect();
    assert_eq!(older, ["surveys:run-0"]);
    let chain = source.run_chain(RUN).await.unwrap();
    assert!(
        chain.is_some_and(|steps| steps.is_empty()),
        "a run of a pipeline in no chain"
    );

    // A read of something that is not a run is the adapter's refusal, not
    // a silent "keeps no runs".
    let refused = source.run_log(PIPELINE, None, 10).await;
    assert!(
        matches!(&refused, Err(SourceError::Protocol { message, .. }) if message.contains("is not a run")),
        "{refused:?}"
    );
}

#[tokio::test]
async fn the_run_s_deferred_sections_carry_the_diff_and_the_tree() {
    let source = build(&load_fixture(), serde_json::json!({}));
    let view = source.view(RUN, None).await.unwrap().unwrap();
    let deferred: Vec<&str> = view
        .sections
        .iter()
        .filter(|s| s.deferred)
        .map(|s| s.id.as_str())
        .collect();
    assert_eq!(deferred, ["changes", "files"]);

    let changes = source.view(RUN, Some("changes")).await.unwrap().unwrap();
    assert!(
        matches!(&changes.sections[0].blocks[0], Block::Diff { files, .. } if files[0].path == "src/hoist.rs"),
        "{changes:?}"
    );
    let files = source.view(RUN, Some("files")).await.unwrap().unwrap();
    let Block::Tree { entries, .. } = &files.sections[0].blocks[0] else {
        panic!("the files section is a tree: {files:?}");
    };
    let opens = entries[0].section.clone().unwrap();
    let file = source.view(RUN, Some(&opens)).await.unwrap().unwrap();
    assert!(
        matches!(&file.sections[0].blocks[0], Block::File { text, .. } if text == "# Crane telemetry"),
        "{file:?}"
    );
}

/// Counts what it is handed, and keeps at most `keep` bytes of it.
#[derive(Default)]
struct Counting {
    name: Option<String>,
    bytes: u64,
    writes: u64,
    largest: usize,
    kept: Vec<u8>,
    keep: usize,
    refuse_after: Option<u64>,
}

#[async_trait::async_trait]
impl DownloadSink for Counting {
    async fn start(&mut self, file_name: &str) -> Result<(), SourceError> {
        self.name = Some(file_name.to_owned());
        Ok(())
    }

    async fn write(&mut self, bytes: &[u8]) -> Result<(), SourceError> {
        if self.refuse_after == Some(self.writes) {
            return Err(SourceError::protocol("the disk is full"));
        }
        self.writes += 1;
        self.bytes += bytes.len() as u64;
        self.largest = self.largest.max(bytes.len());
        let room = self.keep.saturating_sub(self.kept.len());
        self.kept.extend_from_slice(&bytes[..room.min(bytes.len())]);
        Ok(())
    }
}

#[tokio::test]
async fn a_run_s_log_and_one_artifact_download_through_the_host_s_sink() {
    let source = build(&load_fixture(), serde_json::json!({}));
    let mut log = Counting {
        keep: 4096,
        ..Counting::default()
    };
    assert!(
        source
            .download(RUN, &DownloadTarget::Log, &mut log)
            .await
            .unwrap()
    );
    assert_eq!(log.name.as_deref(), Some("run-42.log"));
    assert!(String::from_utf8_lossy(&log.kept).ends_with("error: 1 test failed\n"));

    let mut file = Counting {
        keep: 4096,
        ..Counting::default()
    };
    let target = DownloadTarget::File {
        path: "reports/summary.txt".to_owned(),
    };
    assert!(source.download(RUN, &target, &mut file).await.unwrap());
    assert_eq!(file.name.as_deref(), Some("summary.txt"));
    assert_eq!(file.kept, b"1 failed, 1 passed");
}

#[tokio::test]
async fn a_large_download_streams_without_the_component_holding_it_whole() {
    // 64 MiB through a component capped at 16: it holds one chunk at a time
    // or it is stopped at its cap.
    let adapter = load_fixture().with_limits(Limits {
        memory_mib: 16,
        ..Limits::default()
    });
    let source = build(&adapter, serde_json::json!({ "download_mib": 64 }));
    let mut sink = Counting::default();
    let answer = source.download(RUN, &DownloadTarget::All, &mut sink).await;
    assert!(matches!(answer, Ok(true)), "{answer:?}");
    assert_eq!(sink.bytes, 64 << 20);
    assert!(sink.writes >= 1024, "in many writes: {}", sink.writes);
    assert!(sink.largest <= 64 * 1024, "{}", sink.largest);
}

#[tokio::test]
async fn a_download_the_sink_refuses_fails_with_the_sink_s_error() {
    let source = build(&load_fixture(), serde_json::json!({}));
    let mut sink = Counting {
        refuse_after: Some(2),
        ..Counting::default()
    };
    let answer = source.download(RUN, &DownloadTarget::All, &mut sink).await;
    assert!(
        matches!(&answer, Err(SourceError::Protocol { message, .. }) if message == "the disk is full"),
        "{answer:?}"
    );
    assert_eq!(sink.writes, 2, "nothing more was written after the refusal");
}

/// A component built against 1.0.0 still links and syncs, and keeps no runs
/// to download: what 1.1.0 added is optional to it.
#[tokio::test]
async fn a_component_built_against_1_0_0_is_still_hosted() {
    let path = knobas_wasm_host::fixture_component_v1_0();
    let adapter = WasmAdapter::load(&path).unwrap_or_else(|e| {
        panic!(
            "the 1.0.0 fixture at {} does not load ({e}); `just wasm-fixture` builds it",
            path.display()
        )
    });
    assert_eq!(
        adapter.version(),
        Version {
            major: 1,
            minor: 0,
            patch: 0
        }
    );
    assert_ne!(adapter.version(), INTERFACE_VERSION);
    assert!(!adapter.downloads());
    assert!(load_fixture().downloads());
    let source = build(&adapter, serde_json::json!({}));
    let mut items = knobas_source::contract::VecSink(Vec::new());
    assert_eq!(source.sync(None, &mut items).await.unwrap(), "5");
    let mut sink = Counting::default();
    let answer = source.download(RUN, &DownloadTarget::Log, &mut sink).await;
    assert!(matches!(answer, Ok(false)), "{answer:?}");
    assert!(sink.name.is_none());
}
