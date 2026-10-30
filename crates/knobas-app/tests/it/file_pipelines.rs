//! **File pipelines** (#2264, spec #2234 step 5): the steps between a file's
//! bytes and its renderer, at the IPC seam (`entity_file_piped_inner`), over
//! a fake source that hands out the bytes a test put in it.
//!
//! * Every built-in step round trips through the command.
//! * Order is the pipeline's: `gunzip → jq` draws JSON, `jq → gunzip` fails at jq.
//! * The cap is checked while a step writes: a gzip bomb is refused at it,
//!   and a far bigger one is refused as fast.
//! * A run over a file at a sha is kept by (sha, path, pipeline): the same
//!   pipeline again is the same answer with no work; another filter is run.

use std::collections::HashMap;
use std::io::Write as _;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use knobas_app::file_pipeline::saved::{self, CommandStepDraft, NamedPipelineDraft};
use knobas_app::file_pipeline::{
    Limits, PipeFormat, PipeOutput, PipeRun, PipeStep, PipeWhy, Piped, SIZE_CAP, run_on_file,
};
use knobas_app::renderers::{self, RendererDefault};
use knobas_app::sources::live::LiveSources;
use knobas_app::sources::{self, NewSource, Registry, SecretInput};
use knobas_secrets::{MemoryStore, SecretStore};
use knobas_source::instance::SourceInstance;
use knobas_source::view::{FileBytes, Standing};
use knobas_source::{AuthMethod, Source, SourceDescriptor, SourceError};
use knobas_sync::scheduler::AdapterRegistry;
use sqlx::PgPool;

const SHA: &str = "4c1d0aa4c1d0aa4c1d0aa4c1d0aa4c1d0aa4c1d0";

/// The files the fake serves, and how many byte reads it was asked.
#[derive(Default)]
struct Shelf {
    files: Mutex<HashMap<String, Vec<u8>>>,
    reads: AtomicUsize,
}

struct Building(Arc<Shelf>);

impl AdapterRegistry for Building {
    fn descriptors(&self) -> Vec<SourceDescriptor> {
        Registry::for_process().descriptors()
    }
    fn build(&self, instance: SourceInstance) -> Result<Box<dyn Source>, SourceError> {
        Ok(Box::new(Fake {
            instance,
            shelf: self.0.clone(),
        }))
    }
}

struct Fake {
    instance: SourceInstance,
    shelf: Arc<Shelf>,
}

#[async_trait::async_trait]
impl Source for Fake {
    fn descriptor(&self) -> SourceDescriptor {
        let mut d = Registry::for_process()
            .descriptors()
            .into_iter()
            .find(|d| d.adapter_kind == "mock")
            .expect("the mock is compiled in");
        d.id = self.instance.id.clone();
        d
    }
    async fn test_connection(&self) -> Result<knobas_source::ConnectionInfo, SourceError> {
        Ok(knobas_source::ConnectionInfo::default())
    }
    async fn sync(
        &self,
        cursor: Option<knobas_source::Cursor>,
        _sink: &mut (dyn knobas_source::Sink + Send),
    ) -> Result<knobas_source::Cursor, SourceError> {
        Ok(cursor.unwrap_or_default())
    }
    async fn write(
        &self,
        _op: knobas_source::WriteOp,
    ) -> Result<knobas_source::WriteReceipt, SourceError> {
        Err(SourceError::protocol("read-only"))
    }
    async fn reachable_transitions(&self, _entity: &str) -> Result<Vec<String>, SourceError> {
        Ok(Vec::new())
    }
    async fn view(
        &self,
        _entity: &str,
        _section: Option<&str>,
    ) -> Result<Option<knobas_source::view::SourceView>, SourceError> {
        Ok(None)
    }
    async fn file_bytes(
        &self,
        _entity: &str,
        path: &str,
        _commit: Option<&str>,
        cap: u64,
    ) -> Result<Option<FileBytes>, SourceError> {
        self.shelf.reads.fetch_add(1, Ordering::SeqCst);
        let files = self.shelf.files.lock().unwrap();
        let bytes = files
            .get(path)
            .ok_or_else(|| SourceError::protocol(format!("no file {path:?}")))?;
        Ok(Some(if bytes.len() as u64 > cap {
            FileBytes::TooLarge {
                size: bytes.len() as u64,
            }
        } else {
            FileBytes::Read(bytes.clone())
        }))
    }
    fn bytes_standing(&self, _entity: &str, _path: &str, commit: Option<&str>) -> Standing {
        if commit.is_some() {
            Standing::Fixed
        } else {
            Standing::Live
        }
    }
}

struct Fixture {
    pool: PgPool,
    secrets: Arc<dyn SecretStore>,
    registry: Arc<dyn AdapterRegistry>,
    shelf: Arc<Shelf>,
    live: LiveSources,
    entity: String,
}

/// The jq step runs in a child: the app's own binary (#2314). A test
/// binary is not it, so it is pointed at the one Cargo built beside it.
fn helper() {
    knobas_app::file_pipeline::use_helper(env!("CARGO_BIN_EXE_knobas-app"));
}

/// `file_pipeline::run_bytes`, with the jq helper in place.
async fn run_bytes(steps: &[PipeStep], bytes: Vec<u8>, name: &str, limits: Limits) -> PipeRun {
    helper();
    knobas_app::file_pipeline::run_bytes(steps, bytes, name, limits).await
}

async fn fixture() -> Fixture {
    helper();
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    let shelf = Arc::new(Shelf::default());
    let id = format!("pipe-{}", &uuid::Uuid::new_v4().simple().to_string()[..12]);
    let f = Fixture {
        pool,
        secrets: Arc::new(MemoryStore::default()),
        registry: Arc::new(Building(shelf.clone())),
        shelf,
        live: LiveSources::default(),
        entity: format!("{id}:owner/repo"),
    };
    sources::crud::add(
        &f.pool,
        &f.secrets,
        f.registry.as_ref(),
        NewSource {
            id,
            adapter_kind: "mock".to_owned(),
            display_name: "Fake".to_owned(),
            base_url: "https://pipes.example.invalid".to_owned(),
            auth_kind: AuthMethod::Pat,
            config: serde_json::json!({}),
            secret: SecretInput::of("pat"),
            sync_interval_secs: 300,
            enabled: true,
        },
    )
    .await
    .unwrap();
    f
}

impl Fixture {
    fn put(&self, path: &str, bytes: impl Into<Vec<u8>>) {
        self.shelf
            .files
            .lock()
            .unwrap()
            .insert(path.to_owned(), bytes.into());
    }

    /// `steps` over `path` at [`SHA`], through the command's body.
    async fn pipe(&self, path: &str, steps: &[PipeStep]) -> PipeRun {
        knobas_app::commands::entity::entity_file_piped_inner(
            &self.pool,
            &self.secrets,
            &self.registry,
            &self.live,
            &self.entity,
            path,
            Some(SHA),
            steps,
        )
        .await
        .expect("the read")
    }

    /// The same, in process: the kept run itself.
    async fn piped(&self, path: &str, steps: &[PipeStep]) -> Arc<PipeRun> {
        match run_on_file(
            &self.pool,
            &self.secrets,
            &self.registry,
            &self.live,
            &self.entity,
            path,
            Some(SHA),
            steps,
            Limits::default(),
        )
        .await
        .unwrap()
        {
            Piped::Ran(run) => run,
            other => panic!("{other:?}"),
        }
    }
}

/// The output's name and text, or the run's failure.
fn done(run: &PipeRun) -> (&str, &str) {
    match run {
        PipeRun::Done {
            name,
            output: PipeOutput::Text { text },
        } => (name, text),
        other => panic!("not text: {other:?}"),
    }
}

fn failure(run: &PipeRun) -> &knobas_app::file_pipeline::PipeFailure {
    match run {
        PipeRun::Failed { failure } => failure,
        other => panic!("did not fail: {other:?}"),
    }
}

fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    encoder.write_all(bytes).unwrap();
    encoder.finish().unwrap()
}

fn jq(filter: &str) -> PipeStep {
    PipeStep::Jq {
        filter: filter.to_owned(),
    }
}

const EVENTS: &str = r#"{"events":[{"at":"08:14","level":"info","code":200},{"at":"08:15","level":"error","code":503,"msg":"retry"}]}"#;

/// The ticket's first criterion: a `.json.gz` through `gunzip → jq` is JSON,
/// laid out as jq lays it out, under a name the JSON renderer takes.
#[tokio::test]
async fn gunzip_then_jq_draws_json() {
    let f = fixture().await;
    f.put("fixtures/events.json.gz", gzip(EVENTS.as_bytes()));
    let run = f
        .pipe(
            "fixtures/events.json.gz",
            &[
                PipeStep::Gunzip,
                jq(r#".events[] | select(.level == "error") | {at, code}"#),
            ],
        )
        .await;
    assert_eq!(
        done(&run),
        (
            "fixtures/events.json",
            "{\n  \"at\": \"08:15\",\n  \"code\": 503\n}\n"
        )
    );
}

/// Order is the pipeline's: jq first reads gzip, which is not JSON.
#[tokio::test]
async fn steps_run_in_their_order() {
    let f = fixture().await;
    f.put("events.json.gz", gzip(EVENTS.as_bytes()));
    let run = f.pipe("events.json.gz", &[jq("."), PipeStep::Gunzip]).await;
    let failed = failure(&run);
    assert_eq!((failed.index, failed.name.as_str()), (0, "jq"));
    assert!(failed.error.contains("not JSON"), "{}", failed.error);
}

/// A changed filter is run again; the same pipeline again is the kept run,
/// with no second byte read and no second run.
#[tokio::test]
async fn a_run_is_kept_by_sha_and_pipeline() {
    let f = fixture().await;
    f.put("events.json.gz", gzip(EVENTS.as_bytes()));
    let codes = [PipeStep::Gunzip, jq("[.events[].code]")];
    let first = f.piped("events.json.gz", &codes).await;
    assert_eq!(done(&first).1, "[\n  200,\n  503\n]\n");
    let levels = f
        .piped(
            "events.json.gz",
            &[PipeStep::Gunzip, jq("[.events[].level]")],
        )
        .await;
    assert_eq!(done(&levels).1, "[\n  \"info\",\n  \"error\"\n]\n");
    let again = f.piped("events.json.gz", &codes).await;
    assert!(Arc::ptr_eq(&first, &again), "the kept run, not a new one");
    assert_eq!(f.shelf.reads.load(Ordering::SeqCst), 1, "one byte read");
}

/// The reversed pipeline is another key: it is run, not answered from the
/// forward one's.
#[tokio::test]
async fn another_order_is_another_run() {
    let f = fixture().await;
    f.put("a.json.gz", gzip(br#"{"a":1}"#));
    let forward = f.piped("a.json.gz", &[PipeStep::Gunzip, jq(".a")]).await;
    assert_eq!(done(&forward).1, "1\n");
    let reversed = f.piped("a.json.gz", &[jq(".a"), PipeStep::Gunzip]).await;
    assert_eq!(failure(&reversed).name, "jq");
}

/// A gzip bomb: 201 MiB of zeros in 201 gzip members of 1 MiB, ~200 KiB on
/// the wire. Refused at the cap, naming the cap and the input's size.
#[tokio::test]
async fn a_gzip_bomb_is_refused_at_the_cap() {
    let f = fixture().await;
    let member = gzip(&vec![0u8; 1024 * 1024]);
    let bomb = member.repeat(201);
    f.put("bomb.json.gz", bomb.clone());
    let run = f.pipe("bomb.json.gz", &[PipeStep::Gunzip, jq(".")]).await;
    let failed = failure(&run);
    assert_eq!((failed.index, failed.why), (0, PipeWhy::TooLarge));
    assert_eq!(failed.input.size, bomb.len() as u64);
    assert!(
        failed.error.ends_with(" of input expands past 200 MiB"),
        "{}",
        failed.error
    );
    assert!(
        failed
            .error
            .starts_with("gunzip was stopped at the size cap")
    );
    assert_eq!(SIZE_CAP, 200 * 1024 * 1024);
}

/// The cap is checked while each decompressor writes: 1 GiB behind 1 MiB of
/// input stops at a 1 MiB cap long before the whole would have expanded.
#[tokio::test]
async fn every_decompressor_stops_at_the_cap_as_it_streams() {
    let mib = vec![0u8; 1024 * 1024];
    let limits = Limits {
        cap: 1024 * 1024,
        timeout: Duration::from_secs(8),
    };
    let zstd =
        ruzstd::encoding::compress_to_vec(&mib[..], ruzstd::encoding::CompressionLevel::Fastest);
    let mut bz = bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::fast());
    bz.write_all(&mib).unwrap();
    let bz = bz.finish().unwrap();
    let mut xz = Vec::new();
    lzma_rs::xz_compress(&mut &mib[..], &mut xz).unwrap();
    for (step, member, copies) in [
        (PipeStep::Gunzip, gzip(&mib), 1024),
        (PipeStep::Unzstd, zstd, 1024),
        (PipeStep::Bunzip2, bz, 1024),
        // One xz stream: 2 MiB behind it is past the cap all the same.
        (PipeStep::Unxz, xz, 1),
    ] {
        let input = if copies == 1 {
            let mut two = Vec::new();
            lzma_rs::xz_compress(&mut &[mib.clone(), mib.clone()].concat()[..], &mut two).unwrap();
            two
        } else {
            member.repeat(copies)
        };
        let started = Instant::now();
        let run = run_bytes(std::slice::from_ref(&step), input, "x", limits).await;
        let failed = failure(&run);
        assert_eq!(failed.why, PipeWhy::TooLarge, "{step:?}: {}", failed.error);
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "{step:?} took {:?}",
            started.elapsed()
        );
    }
}

/// A jq syntax error names the step and says what the filter got wrong.
#[tokio::test]
async fn a_jq_syntax_error_names_the_step() {
    let f = fixture().await;
    f.put("a.json.gz", gzip(br#"{"a":1}"#));
    let run = f
        .pipe("a.json.gz", &[PipeStep::Gunzip, jq(".a | | .b")])
        .await;
    let failed = failure(&run);
    assert_eq!((failed.index, failed.name.as_str()), (1, "jq"));
    assert_eq!(failed.why, PipeWhy::Error);
    assert!(
        failed
            .error
            .starts_with("the filter does not parse: expected "),
        "{}",
        failed.error
    );
    assert!(
        failed.error.contains("expected term at column 6"),
        "{}",
        failed.error
    );
    // Its input is gunzip's output, shown as text.
    assert_eq!(failed.input.head, r#"{"a":1}"#);
    assert!(!failed.input.binary);
}

/// A runtime error is jq's own words; an unknown function names itself.
#[tokio::test]
async fn jq_errors_are_its_own() {
    let run = run_bytes(
        &[jq(".a | ascii_downcase")],
        br#"{"a":null}"#.to_vec(),
        "x.json",
        Limits::default(),
    )
    .await;
    assert!(
        failure(&run).error.starts_with("jq: error: "),
        "{:?}",
        failure(&run)
    );
    let run = run_bytes(
        &[jq("nosuch(1)")],
        b"{}".to_vec(),
        "x.json",
        Limits::default(),
    )
    .await;
    assert!(
        failure(&run).error.contains("`nosuch`"),
        "{:?}",
        failure(&run)
    );
}

/// A step past its timeout is answered as timed out, and the jq step is
/// killed there (#2314): it runs in a child process, so a filter that loops
/// without writing (or composes a closure with itself without end) holds
/// nothing once its time is up. More of them than there are workers, and a
/// built-in step after them still gets a worker at once.
#[tokio::test]
async fn a_step_past_its_timeout_is_stopped() {
    let limits = Limits {
        cap: SIZE_CAP,
        timeout: Duration::from_millis(300),
    };
    let started = Instant::now();
    let filters: [&'static str; 4] = [
        "repeat(1)",
        "last(range(1e18))",
        "[repeat(1)] | length",
        "def f(g): f(g | g); f(.)",
    ];
    let endless: Vec<_> = filters
        .into_iter()
        .cycle()
        .take(knobas_app::file_pipeline::WORKERS + 2)
        .map(|filter| {
            tokio::spawn(
                async move { run_bytes(&[jq(filter)], b"null".to_vec(), "x", limits).await },
            )
        })
        .collect();
    for run in endless {
        let run = run.await.unwrap();
        let failed = failure(&run);
        assert_eq!(failed.why, PipeWhy::TimedOut, "{failed:?}");
        assert_eq!(failed.error, "jq did not finish in 300 ms");
    }
    assert!(started.elapsed() < Duration::from_secs(3));
    let next = Limits {
        cap: SIZE_CAP,
        timeout: Duration::from_secs(2),
    };
    let run = run_bytes(&[PipeStep::Gunzip], gzip(b"1"), "x.gz", next).await;
    assert_eq!(done(&run).1, "1");
    let run = run_bytes(&[jq(".")], b"1".to_vec(), "x", next).await;
    assert_eq!(done(&run).1, "1\n");
}

/// **A value nested deep at run time fails the step, not the app (#2314).**
/// `reduce` builds it by iteration, past the evaluation's stack guard; then
/// writing it (`tojson`) or comparing it (`==`) recursed off the end of the
/// stack and aborted the process. (Written as the output, it passes the size
/// cap first: two spaces of indent a level.) The jq step now runs in a child, so the
/// child dies and the step says why. That this test process is still here to
/// assert is half of the claim.
#[tokio::test]
async fn a_value_nested_deep_at_run_time_fails_the_step_not_the_app() {
    for filter in [
        "reduce range(1000000) as $_ (0; [.]) | tojson",
        "def deep: reduce range(1000000) as $_ (0; [.]); deep == deep",
    ] {
        let run = run_bytes(&[jq(filter)], b"null".to_vec(), "x.json", Limits::default()).await;
        let failed = failure(&run);
        assert_eq!((failed.index, failed.name.as_str()), (0, "jq"), "{filter}");
        assert_eq!(failed.why, PipeWhy::Error, "{filter}: {failed:?}");
        assert_eq!(
            failed.error, "jq ran out of stack: the filter built a value nested too deep to write",
            "{filter}"
        );
    }
}

/// **Every built-in step round trips at the IPC seam.**
#[tokio::test]
async fn every_built_in_step_round_trips() {
    let f = fixture().await;
    let json = r#"{"name":"knobas","tags":["a","b"],"n":1}"#;
    f.put("doc.json", json);
    f.put(
        "doc.json.b64",
        "eyJuYW1lIjoia25vYmFzIiwidGFncyI6WyJhIiwiYiJdLCJuIjoxfQ==\n",
    );
    f.put("doc.json.gz", gzip(json.as_bytes()));
    f.put(
        "doc.json.zst",
        ruzstd::encoding::compress_to_vec(
            json.as_bytes(),
            ruzstd::encoding::CompressionLevel::Fastest,
        ),
    );
    let mut bz = bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::fast());
    bz.write_all(json.as_bytes()).unwrap();
    f.put("doc.json.bz2", bz.finish().unwrap());
    let mut xz = Vec::new();
    lzma_rs::xz_compress(&mut json.as_bytes(), &mut xz).unwrap();
    f.put("doc.json.xz", xz);
    f.put("doc.yaml", "name: knobas\ntags:\n  - a\n  - b\nn: 1\n");
    f.put(
        "doc.toml",
        "name = \"knobas\"\nn = 1\n[deps]\nserde = \"1\"\n",
    );
    f.put("blob.bin", vec![0x1f, 0x8b, b'h', b'i', 0, 255]);

    for path in [
        "doc.json.gz",
        "doc.json.zst",
        "doc.json.bz2",
        "doc.json.xz",
        "doc.json.b64",
    ] {
        let step = match path.rsplit('.').next().unwrap() {
            "gz" => PipeStep::Gunzip,
            "zst" => PipeStep::Unzstd,
            "bz2" => PipeStep::Bunzip2,
            "xz" => PipeStep::Unxz,
            _ => PipeStep::Base64Decode,
        };
        assert_eq!(
            done(&f.pipe(path, &[step]).await),
            ("doc.json", json),
            "{path}"
        );
    }
    assert_eq!(
        done(&f.pipe("doc.json", &[PipeStep::Base64Encode]).await),
        (
            "doc.json.b64",
            "eyJuYW1lIjoia25vYmFzIiwidGFncyI6WyJhIiwiYiJdLCJuIjoxfQ=="
        )
    );
    assert_eq!(
        done(&f.pipe("doc.json", &[jq(".tags[0]")]).await),
        ("doc.json", "\"a\"\n")
    );
    let pretty_json =
        "{\n  \"name\": \"knobas\",\n  \"tags\": [\n    \"a\",\n    \"b\"\n  ],\n  \"n\": 1\n}\n";
    assert_eq!(
        done(
            &f.pipe(
                "doc.json",
                &[PipeStep::Pretty {
                    format: PipeFormat::Json
                }]
            )
            .await
        ),
        ("doc.json", pretty_json)
    );
    assert_eq!(
        done(
            &f.pipe(
                "doc.yaml",
                &[PipeStep::Pretty {
                    format: PipeFormat::Yaml
                }]
            )
            .await
        ),
        ("doc.yaml", "name: knobas\ntags:\n- a\n- b\nn: 1\n")
    );
    assert_eq!(
        done(
            &f.pipe(
                "doc.toml",
                &[PipeStep::Pretty {
                    format: PipeFormat::Toml
                }]
            )
            .await
        ),
        (
            "doc.toml",
            "name = \"knobas\"\nn = 1\n\n[deps]\nserde = \"1\"\n"
        )
    );
    let convert = |from, to| PipeStep::Convert { from, to };
    assert_eq!(
        done(
            &f.pipe("doc.json", &[convert(PipeFormat::Json, PipeFormat::Yaml)])
                .await
        ),
        ("doc.yaml", "name: knobas\ntags:\n- a\n- b\nn: 1\n")
    );
    assert_eq!(
        done(
            &f.pipe("doc.yaml", &[convert(PipeFormat::Yaml, PipeFormat::Json)])
                .await
        ),
        ("doc.json", pretty_json)
    );
    assert_eq!(
        done(
            &f.pipe("doc.toml", &[convert(PipeFormat::Toml, PipeFormat::Json)])
                .await
        ),
        (
            "doc.json",
            "{\n  \"name\": \"knobas\",\n  \"n\": 1,\n  \"deps\": {\n    \"serde\": \"1\"\n  }\n}\n"
        )
    );
    assert_eq!(
        done(
            &f.pipe("doc.yaml", &[convert(PipeFormat::Yaml, PipeFormat::Toml)])
                .await
        ),
        (
            "doc.toml",
            "name = \"knobas\"\ntags = [\n    \"a\",\n    \"b\",\n]\nn = 1\n"
        )
    );
    assert_eq!(
        done(&f.pipe("blob.bin", &[PipeStep::HexDump]).await),
        (
            "blob.bin.hexdump",
            "00000000  1f 8b 68 69 00 ff                                 |..hi..|\n"
        )
    );
    // Bytes that are not text cross as base64, with their size.
    match f.pipe("doc.json", &[PipeStep::Gunzip]).await {
        PipeRun::Failed { failure } => assert_eq!(failure.name, "gunzip"),
        other => panic!("{other:?}"),
    }
    match f.pipe("blob.bin", &[]).await {
        PipeRun::Done {
            output: PipeOutput::Bytes { base64, size },
            ..
        } => assert_eq!((base64.as_str(), size), ("H4toaQD/", 6)),
        other => panic!("{other:?}"),
    }
}

/// A failed step's input that is not text is shown as a hex dump.
#[tokio::test]
async fn a_binary_input_is_shown_as_hex() {
    let run = run_bytes(
        &[jq(".")],
        vec![0x1f, 0x8b, 0, 1],
        "x.gz",
        Limits::default(),
    )
    .await;
    let input = &failure(&run).input;
    assert!(input.binary);
    assert!(
        input.head.starts_with("00000000  1f 8b 00 01"),
        "{}",
        input.head
    );
}

/// **A filter that recurses without end is refused, by name (#2314).** Each
/// of these overflowed the worker's stack and aborted the process; now the
/// evaluation is unwound at its stack budget and the step says why. That
/// this test process is still here to assert is half of the claim.
#[tokio::test]
async fn a_recursive_filter_is_refused_with_the_steps_name() {
    for filter in ["def f: 1 + f; f", "def f: [f]; f", "def f: {a: f}; f"] {
        let run = run_bytes(
            &[PipeStep::Gunzip, jq(filter)],
            gzip(b"0"),
            "x.json.gz",
            Limits::default(),
        )
        .await;
        let failed = failure(&run);
        assert_eq!((failed.index, failed.name.as_str()), (1, "jq"), "{filter}");
        assert_eq!(failed.why, PipeWhy::Error, "{filter}: {failed:?}");
        assert!(
            failed.error.starts_with("the filter recursed too deep"),
            "{filter}: {failed:?}"
        );
    }
    // Honest recursion still runs.
    let run = run_bytes(
        &[jq("def f: if . < 10000 then . + 1 | f else . end; f")],
        b"0".to_vec(),
        "x.json",
        Limits::default(),
    )
    .await;
    assert_eq!(done(&run).1, "10000\n");
}

/// JSON nested past jq's own parse limit is refused, as a step's input and
/// as a `fromjson` string, rather than parsed by recursion off the stack.
#[tokio::test]
async fn json_nested_too_deep_is_refused() {
    let deep = format!("{}{}", "[".repeat(5_000_000), "]".repeat(5_000_000));
    for step in [
        jq("."),
        PipeStep::Pretty {
            format: PipeFormat::Json,
        },
    ] {
        let run = run_bytes(
            std::slice::from_ref(&step),
            deep.clone().into_bytes(),
            "x.json",
            Limits::default(),
        )
        .await;
        let failed = failure(&run);
        assert_eq!(
            failed.error, "the input nests deeper than 10000 levels",
            "{step:?}"
        );
    }
    let quoted = serde_json::to_vec(&deep).unwrap();
    let run = run_bytes(&[jq("fromjson")], quoted, "x.json", Limits::default()).await;
    let failed = failure(&run);
    assert!(
        failed
            .error
            .starts_with("fromjson was given JSON that nests deeper"),
        "{failed:?}"
    );
    // Within the limit it parses, brackets in strings not counted.
    let run = run_bytes(
        &[jq("fromjson | .[0][0]")],
        br#""[[\"[[[[\"]]""#.to_vec(),
        "x.json",
        Limits::default(),
    )
    .await;
    assert_eq!(done(&run).1, "\"[[[[\"\n");
}

/// Pretty-print and convert write through the cap too: indenting a nested
/// value multiplies it, so 10 KB of brackets would lay out as 50 MB. (The
/// answer is the same when the cap is checked after the whole layout; what
/// changed is that the 50 MB is never built.)
#[tokio::test]
async fn laying_out_is_stopped_at_the_cap() {
    let limits = Limits {
        cap: 1024 * 1024,
        timeout: Duration::from_secs(10),
    };
    let nested = format!("{}{}", "[".repeat(5_000), "]".repeat(5_000));
    let yaml = format!("- {}\n", "x".repeat(2 * 1024 * 1024));
    for (step, input) in [
        (
            PipeStep::Pretty {
                format: PipeFormat::Json,
            },
            nested.clone(),
        ),
        (
            PipeStep::Convert {
                from: PipeFormat::Yaml,
                to: PipeFormat::Json,
            },
            yaml,
        ),
    ] {
        let run = run_bytes(std::slice::from_ref(&step), input.into_bytes(), "x", limits).await;
        assert_eq!(failure(&run).why, PipeWhy::TooLarge, "{step:?}: {run:?}");
    }
}

fn command(name: &str, command: &str) -> CommandStepDraft {
    CommandStepDraft {
        id: None,
        name: name.to_owned(),
        command: command.to_owned(),
        timeout_secs: None,
    }
}

fn step(id: &str) -> PipeStep {
    PipeStep::Command { id: id.to_owned() }
}

/// **A command step runs the person's saved command (#2265)**: `tr a-z A-Z`
/// uppercases the content, which reaches it on stdin; the output keeps the
/// file's name. A failing one is named by its saved name and says its
/// stderr; an id that is not saved fails naming it.
#[tokio::test]
async fn a_command_step_uppercases_the_content() {
    let f = fixture().await;
    let saved = saved::save_command_steps(
        &f.pool,
        &[
            command(" Upper ", "tr a-z A-Z"),
            command("Breaks", r#"sh -c "echo it broke >&2; exit 3""#),
        ],
    )
    .await
    .unwrap();
    assert_eq!(
        saved.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
        ["upper", "breaks"]
    );
    f.put("notes.txt", "hello, pipeline");
    let run = f.pipe("notes.txt", &[step("upper")]).await;
    assert_eq!(done(&run), ("notes.txt", "HELLO, PIPELINE"));

    let run = f.pipe("notes.txt", &[step("upper"), step("breaks")]).await;
    let failed = failure(&run);
    assert_eq!((failed.index, failed.name.as_str()), (1, "Breaks"));
    assert_eq!(failed.why, PipeWhy::Error);
    assert_eq!(failed.error, "it broke");
    assert_eq!(failed.input.head, "HELLO, PIPELINE");

    let run = f.pipe("notes.txt", &[step("gone")]).await;
    assert!(
        failure(&run)
            .error
            .contains("'gone' is not one of your command steps"),
        "{run:?}"
    );
}

/// A kept run stands for the command its step ran: the same id with an
/// edited command is run again, not answered from the kept run.
#[tokio::test]
async fn an_edited_command_step_is_run_again() {
    let f = fixture().await;
    saved::save_command_steps(&f.pool, &[command("Upper", "tr a-z A-Z")])
        .await
        .unwrap();
    f.put("a.txt", "abc");
    assert_eq!(done(&f.pipe("a.txt", &[step("upper")]).await).1, "ABC");
    let mut edited = command("Upper", "tr a-c x-z");
    edited.id = Some("upper".to_owned());
    saved::save_command_steps(&f.pool, &[edited]).await.unwrap();
    assert_eq!(done(&f.pipe("a.txt", &[step("upper")]).await).1, "xyz");
}

/// A command step is killed at its own timeout, with everything it started.
#[tokio::test]
async fn a_command_step_past_its_timeout_is_stopped() {
    let f = fixture().await;
    let mut slow = command("Slow", "sleep 30");
    slow.timeout_secs = Some(1);
    saved::save_command_steps(&f.pool, &[slow]).await.unwrap();
    f.put("a.txt", "abc");
    let started = Instant::now();
    let run = f.pipe("a.txt", &[step("slow")]).await;
    let failed = failure(&run);
    assert_eq!(failed.why, PipeWhy::TimedOut);
    assert_eq!(failed.error, "Slow did not finish in 1 s");
    assert!(started.elapsed() < Duration::from_secs(5));
}

/// Named pipelines are one settings row: saved with ids made from their
/// names, a rename keeping the id; one with no steps, or a name used twice,
/// is refused by name.
#[tokio::test]
async fn named_pipelines_round_trip_and_refuse_by_name() {
    let f = fixture().await;
    let draft = |id: Option<&str>, name: &str, steps: Vec<PipeStep>| NamedPipelineDraft {
        id: id.map(str::to_owned),
        name: name.to_owned(),
        steps,
    };
    let saved = saved::save_named_pipelines(
        &f.pool,
        &[draft(
            None,
            " Gzipped JSON ",
            vec![PipeStep::Gunzip, jq(".")],
        )],
    )
    .await
    .unwrap();
    assert_eq!(saved[0].id, "gzipped-json");
    assert_eq!(saved[0].name, "Gzipped JSON");
    let renamed = saved::save_named_pipelines(
        &f.pool,
        &[draft(
            Some("gzipped-json"),
            "Events",
            vec![PipeStep::Gunzip],
        )],
    )
    .await
    .unwrap();
    assert_eq!(saved::named_pipelines(&f.pool).await.unwrap(), renamed);
    assert_eq!(renamed[0].id, "gzipped-json");

    let error = saved::save_named_pipelines(&f.pool, &[draft(None, "Empty", vec![])])
        .await
        .unwrap_err();
    assert!(
        error.message.contains("Empty has no steps"),
        "{}",
        error.message
    );
    let error = saved::save_named_pipelines(
        &f.pool,
        &[
            draft(None, "Twice", vec![PipeStep::Gunzip]),
            draft(None, "twice", vec![PipeStep::Gunzip]),
        ],
    )
    .await
    .unwrap_err();
    assert!(error.message.contains("twice"), "{}", error.message);
    assert_eq!(saved::named_pipelines(&f.pool).await.unwrap(), renamed);
}

/// A default per file pattern may name a pipeline and no renderer (the
/// output picks its own); one naming neither is refused. A row stored before
/// the field existed reads as naming no pipeline.
#[tokio::test]
async fn a_default_may_name_only_a_pipeline() {
    let f = fixture().await;
    let row = |renderer: &str, pipeline: Option<&str>| RendererDefault {
        pattern: "*.json.gz".to_owned(),
        renderer: renderer.to_owned(),
        pipeline: pipeline.map(str::to_owned),
    };
    let saved = renderers::save_defaults(&f.pool, &[row("", Some(" events "))])
        .await
        .unwrap();
    assert_eq!(saved, [row("", Some("events"))]);
    let error = renderers::save_defaults(&f.pool, &[row(" ", Some(" "))])
        .await
        .unwrap_err();
    assert!(error.message.contains("*.json.gz"), "{}", error.message);
    let old: RendererDefault =
        serde_json::from_str(r#"{"pattern":"*.md","renderer":"source"}"#).unwrap();
    assert_eq!(old.pipeline, None);
}
