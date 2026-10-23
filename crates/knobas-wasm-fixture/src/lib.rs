//! The fixture external adapter (issue #1198, ADR-0028).
//!
//! A test adapter built to a WASM component against
//! `knobas:adapter@1.3.0`. It serves three surveys, and a pipeline with one
//! run ([`builds`], #1205), so that the host can be tested with the contract
//! battery, every live read, and through the sync engine into the mirror,
//! without a real product behind it. No external
//! adapter ships with knobas; this is what the host is tested with.
//!
//! * **The corpus** is [`SURVEYS`]. Every survey carries the word `quaywright`
//!   in its body, a token nothing else in knobas' fixtures spells, so a
//!   search for it finds these and only these.
//! * **The cursor** is the number of items already emitted, as a decimal
//!   string: the surveys, then the pipeline, then the run. A full sync emits
//!   all five and returns `"5"`; a sync from `"5"` emits nothing and returns
//!   `"5"`, as the battery's clause 2 asks.
//! * **The config** is `{"fault": "none" | "unauthorized" | "unreachable"}`,
//!   the battery's `Fault`. A faulted fixture fails `test_connection` and
//!   `sync` with the mapped error. A config it cannot read fails every call
//!   with a protocol error naming it.
//! * **The roles** are the custom role [`ROLE`], whose survey's view is two
//!   blocks: who surveyed it, and its body (#1202); and CI, for the Builds
//!   frame, with every optional read (#1205).
//! * **The writes** are one source action, [`BOOK_REPAIR`] (#1407): a
//!   custom role's writes are all source actions (#1194), and a survey's
//!   view offers this one, with one field, the day. The fixture performs it
//!   on a survey by answering a receipt and changing nothing, and refuses
//!   every other write with a protocol error, as battery clause 11 asks.
//! * **Built with `--features v1_0`** it is the same adapter against the
//!   frozen 1.0.0 world (`wit-1.0.0/`), which has no `downloads`, `query` or
//!   `sink.walked`: what the host's hold on 1.0.0 components is tested with.
//! * **The walk** (1.3.0, #1380): the outer unit is the item itself, as the
//!   mock's is. A sync reports `walked` before each item it hands and once
//!   after the last, `of` the items this run hands (`"items"`), so a sync
//!   from a cursor that hands nothing still reports `0 of 0`. The 1.0.0
//!   build reports nothing.
//! * **`"fault": "swallow_sink"`** is a misbehaving adapter: it ignores what
//!   `sink.item` answers and returns its cursor anyway, so the host's own
//!   hold on a refused item is witnessed.
//! * **`"fault": "spin"` and `"fault": "hog"`** are adapters that pass a
//!   limit (#1204): `sync` spins forever, or allocates a MiB at a time and
//!   never lets go, so the host's deadline and memory cap are witnessed.
//! * **`"fetch": "<url>"`** makes `test_connection` `GET` that URL through
//!   `wasi:http`, following redirects itself, and answer everything it was
//!   handed (its instance record, every hop's status, headers and body, the
//!   error that ended the walk) as JSON: in `detail` when the walk ended on
//!   a response, in an `Unreachable` error when it did not (#1203).
//!   `"first_byte_ms"` and `"post_mib"` bend that request
//!   ([`fetch::Options`]); `"fetch_times"` runs the fetch that many times
//!   in a row, as fast as the host lets it, and answers the last one: a
//!   naive adapter, for the host's pacing (#1631). `mirror` and `username`
//!   are config keys it declares and never reads, for a manifest's host
//!   template and the host's basic auth.
//! * **`"log_flood": <n>`** makes `test_connection` log one 256 KiB
//!   message `n` times before it answers: an adapter that floods the
//!   reader's log, for the host's bound on its lines (#1641).
//! * **`"download_mib"`** is how many MiB a download of every artifact
//!   streams (default 1).
//! * **`"descriptor_dwell_ms"`** makes `descriptor` wait that long on the
//!   host's clock before it answers: a component slow to describe itself,
//!   for the callers that build one off the runtime (#1705). The descriptor
//!   read at load is of an empty config, so it never waits.
//! * **`translate-query`** (#1208) reads a payload's `"survey_query"`
//!   string as one `label` condition and leaves nothing out; a payload
//!   without one it does not translate (`none`).

mod builds;
mod fetch;

use chrono::{TimeZone, Utc};
use exports::knobas::adapter::source::{Guest, GuestAdapter};
use knobas::adapter::sink;
use knobas::adapter::types::{
    ConnectionInfo, Fault as WitFault, Instance, RunChainStep, SourceError, SyncItem, WriteReceipt,
};
use knobas_source::role::RoleClaim;
use knobas_source::{
    AuthMethod, Capability, EntityRef, KindInfo, SOURCE_ACTION, SourceDescriptor, WriteOp,
};

#[cfg(not(feature = "v1_0"))]
wit_bindgen::generate!({
    world: "adapter",
    path: "../knobas-wasm-host/wit",
});

#[cfg(feature = "v1_0")]
wit_bindgen::generate!({
    world: "adapter",
    path: "wit-1.0.0",
});

/// The adapter kind this fixture is. A source's id is its own.
pub const KIND: &str = "wasm-fixture";

/// The custom role the fixture claims (ADR-0029): its surveys fit no built-in
/// role, so a survey opens the view `view` answers, and nothing else draws
/// them. Written out whole, not formatted from [`KIND`], so a test can find
/// the claim in the component's bytes (#1202's refusal of a role that is not
/// namespaced to the adapter).
pub const ROLE: &str = "wasm-fixture:survey-desk";

/// The fixture's one source action (#1407), declared in the descriptor's
/// `source_actions` and offered on every survey's view.
pub const BOOK_REPAIR: &str = "book_repair";

/// `(key, title, body)` per survey. The kind is `survey`, not `note`: a note
/// is knobas' own (quick capture), and the launcher hides those on a setting.
pub const SURVEYS: [(&str, &str, &str); 3] = [
    (
        "survey-1",
        "Quay crane 4 inspection",
        "quaywright inspection of quay crane 4: hoist brake worn, booked for Tuesday",
    ),
    (
        "survey-2",
        "Berth 7 fender survey",
        "quaywright survey of the berth 7 fenders: two panels cracked",
    ),
    (
        "survey-3",
        "Reefer plug audit",
        "quaywright audit of the reefer plugs on stack C: all forty answer",
    ),
];

#[derive(Clone, Copy)]
enum Fault {
    None,
    Unauthorized,
    Unreachable,
    SwallowSink,
    Spin,
    Hog,
}

struct Fixture;

impl Guest for Fixture {
    type Adapter = Adapter;
}

#[cfg(not(feature = "v1_0"))]
impl exports::knobas::adapter::query::Guest for Fixture {
    fn translate_query(
        _instance: Instance,
        saved_view: String,
    ) -> Result<Option<String>, SourceError> {
        let view: serde_json::Value = serde_json::from_str(&saved_view)
            .map_err(|e| protocol(format!("the saved view is not JSON: {e}")))?;
        Ok(view["survey_query"].as_str().map(|query| {
            serde_json::json!({
                "conditions": [{ "key": "label", "negated": false, "values": [query] }],
                "kept": 1,
                "left_out": [],
            })
            .to_string()
        }))
    }
}

/// One instance, for one call.
pub struct Adapter {
    id: String,
    name: String,
    fault: Result<Fault, String>,
    /// The URL `test_connection` reads, when the config names one.
    fetch: Option<String>,
    fetch_options: fetch::Options,
    /// How many times in a row `test_connection` runs the fetch.
    fetch_times: u64,
    /// The instance record as the host handed it over, for the fetch answer.
    received: serde_json::Value,
    /// The MiB a download of every artifact streams.
    #[cfg_attr(feature = "v1_0", allow(dead_code))]
    download_mib: u64,
    /// How many times `test_connection` logs [`FLOOD_BYTES`] bytes.
    log_flood: u64,
    /// How long `descriptor` waits before it answers, in milliseconds.
    descriptor_dwell_ms: u64,
}

/// The size of one message of a log flood.
const FLOOD_BYTES: usize = 256 << 10;

/// Log a 256 KiB message `times` times. Two-byte characters after a
/// seven-byte head, so a cut at an even size falls inside a character.
fn flood(times: u64) {
    let mut message = String::from("flood: ");
    while message.len() + 'ä'.len_utf8() <= FLOOD_BYTES {
        message.push('ä');
    }
    for _ in 0..times {
        knobas::adapter::log::log(knobas::adapter::log::Level::Info, &message);
    }
}

fn protocol(message: String) -> SourceError {
    SourceError::Protocol(WitFault {
        status: None,
        message,
    })
}

fn read_fault(config: &str) -> Result<Fault, String> {
    let value: serde_json::Value =
        serde_json::from_str(config).map_err(|e| format!("config is not JSON: {e}"))?;
    let object = value
        .as_object()
        .ok_or_else(|| format!("config must be an object, got {value}"))?;
    if let Some(key) = object.keys().find(|k| {
        ![
            "fault",
            "fetch",
            "first_byte_ms",
            "post_mib",
            "fetch_times",
            "download_mib",
            "log_flood",
            "descriptor_dwell_ms",
            "mirror",
            "username",
        ]
        .contains(&k.as_str())
    }) {
        return Err(format!(
            "config has a key the fixture does not know: {key:?}"
        ));
    }
    match object.get("fault").and_then(serde_json::Value::as_str) {
        None | Some("none") => Ok(Fault::None),
        Some("unauthorized") => Ok(Fault::Unauthorized),
        Some("unreachable") => Ok(Fault::Unreachable),
        Some("swallow_sink") => Ok(Fault::SwallowSink),
        Some("spin") => Ok(Fault::Spin),
        Some("hog") => Ok(Fault::Hog),
        Some(other) => Err(format!(
            "config fault {other:?} is not one the fixture knows"
        )),
    }
}

impl Adapter {
    /// The fault to answer with, or `None` for a healthy fixture.
    fn faulted(&self) -> Result<Option<SourceError>, SourceError> {
        match &self.fault {
            Err(message) => Err(protocol(message.clone())),
            Ok(Fault::None | Fault::SwallowSink | Fault::Spin | Fault::Hog) => Ok(None),
            Ok(Fault::Unauthorized) => Ok(Some(SourceError::Unauthorized(Some(401)))),
            Ok(Fault::Unreachable) => Ok(Some(SourceError::Unreachable(
                "the fixture was told to be unreachable".into(),
            ))),
        }
    }

    fn healthy(&self) -> Result<(), SourceError> {
        match self.faulted()? {
            None => Ok(()),
            Some(error) => Err(error),
        }
    }

    fn survey(&self, (key, title, body): (&str, &str, &str)) -> SyncItem {
        wit_item(knobas_source::SyncItem {
            entity: EntityRef::new(&self.id, key),
            kind: "survey".into(),
            title: title.into(),
            body_text: body.into(),
            author: Some("harbourmaster".into()),
            updated_at: Utc.with_ymd_and_hms(2026, 10, 18, 9, 0, 0).single(),
            payload: serde_json::json!({ "key": key, "title": title }),
            web_url: None,
            deleted: false,
        })
    }

    /// A frame item: the pipeline, or a run of it.
    fn frame_item(
        &self,
        key: &str,
        kind: &str,
        title: &str,
        payload: serde_json::Value,
    ) -> SyncItem {
        wit_item(knobas_source::SyncItem {
            entity: EntityRef::new(&self.id, key),
            kind: kind.into(),
            title: title.into(),
            body_text: String::new(),
            author: None,
            updated_at: Utc.with_ymd_and_hms(2026, 10, 18, 9, 4, 0).single(),
            payload,
            web_url: None,
            deleted: false,
        })
    }

    /// Every item, in the order the cursor counts them.
    fn corpus(&self) -> Vec<SyncItem> {
        let mut items: Vec<SyncItem> = SURVEYS.iter().map(|s| self.survey(*s)).collect();
        items.push(self.frame_item(
            builds::PIPELINE,
            "pipeline",
            "Crane telemetry",
            builds::pipeline_payload(),
        ));
        items.push(self.run(builds::RUN, "42"));
        items
    }

    fn run(&self, key: &str, number: &str) -> SyncItem {
        self.frame_item(
            key,
            "run",
            &format!("Crane telemetry #{number}"),
            builds::run_payload(key, number),
        )
    }

    /// The key of `entity` when it is one of this source's, else `None`.
    fn key<'a>(&self, entity: &'a str) -> Option<&'a str> {
        entity.strip_prefix(&self.id)?.strip_prefix(':')
    }

    /// Whether `entity` is the fixture's run: every run read answers it
    /// alone, and `None` for anything else.
    fn is_run(&self, entity: &str) -> bool {
        self.key(entity) == Some(builds::RUN)
    }

    fn no_workflow(&self) -> SourceError {
        protocol(format!(
            "the {KIND} adapter has no workflow: source {:?} offers no transitions",
            self.id
        ))
    }
}

/// `sink.walked`, which 1.3.0 added (#1380); the 1.0.0 build has none and
/// reports nothing.
fn walked(done: u64, of: u64) {
    #[cfg(not(feature = "v1_0"))]
    sink::walked(done, Some(of), "items");
    #[cfg(feature = "v1_0")]
    let _ = (done, of);
}

/// The SPI's item as the WIT world carries it: the entity as `ns:key`, the
/// instant as RFC 3339, the payload as its JSON text.
fn wit_item(item: knobas_source::SyncItem) -> SyncItem {
    SyncItem {
        entity: item.entity.to_string(),
        kind: item.kind,
        title: item.title,
        body_text: item.body_text,
        author: item.author,
        updated_at: item
            .updated_at
            .map(|at| at.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)),
        payload: item.payload.to_string(),
        web_url: item.web_url,
        deleted: item.deleted,
    }
}

/// Never returns, and never calls the host: only the deadline stops it.
fn spin() -> ! {
    let mut turns: u64 = 0;
    loop {
        turns = std::hint::black_box(turns.wrapping_add(1));
    }
}

/// Allocates a MiB at a time and keeps all of it: only the memory cap
/// stops it.
fn hog() -> ! {
    let mut held: Vec<Vec<u8>> = Vec::new();
    loop {
        held.push(std::hint::black_box(vec![1; 1 << 20]));
    }
}

impl GuestAdapter for Adapter {
    fn new(instance: Instance) -> Self {
        let config = serde_json::from_str::<serde_json::Value>(&instance.config).ok();
        let read = |key: &str| config.as_ref().and_then(|c| c.get(key)?.as_u64());
        let fetch = config
            .as_ref()
            .and_then(|config| config.get("fetch")?.as_str().map(str::to_owned));
        let fetch_options = fetch::Options {
            first_byte_ms: read("first_byte_ms"),
            post_mib: read("post_mib"),
        };
        Self {
            fault: read_fault(&instance.config),
            fetch,
            fetch_options,
            fetch_times: read("fetch_times").unwrap_or(1).max(1),
            download_mib: read("download_mib").unwrap_or(1),
            log_flood: read("log_flood").unwrap_or(0),
            descriptor_dwell_ms: read("descriptor_dwell_ms").unwrap_or(0),
            received: serde_json::json!({
                "id": instance.id,
                "kind": instance.kind,
                "display_name": instance.display_name,
                "base_url": instance.base_url,
                "config": instance.config,
            }),
            id: instance.id,
            name: instance.display_name,
        }
    }

    fn descriptor(&self) -> String {
        if self.descriptor_dwell_ms > 0 {
            wasip2::clocks::monotonic_clock::subscribe_duration(
                self.descriptor_dwell_ms.saturating_mul(1_000_000),
            )
            .block();
        }
        let [pipeline, run] = builds::kinds();
        let descriptor = SourceDescriptor {
            id: self.id.clone(),
            adapter_kind: KIND.into(),
            name: self.name.clone(),
            // It writes only its source action (#1407); the battery holds
            // `Write` and `write_ops` to each other.
            capabilities: vec![Capability::Write],
            adapter_version: "0.1.0".into(),
            // A token, which the fixture never reads: every source takes a
            // credential at Add source, and the host attaches it (#1203).
            auth_methods: vec![AuthMethod::Pat],
            accepts_account: false,
            min_sync_interval_secs: 60,
            write_ops: vec![SOURCE_ACTION.into()],
            entity_kinds: vec![
                KindInfo {
                    id: "survey".into(),
                    label: "Survey".into(),
                    plural: "Surveys".into(),
                    monogram: "SV".into(),
                    full_sync_exhaustive: true,
                },
                pipeline,
                run,
            ],
            config_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "fault": { "type": "string", "enum": ["none", "unauthorized", "unreachable", "swallow_sink", "spin", "hog"] },
                    "fetch": { "type": "string" },
                    "first_byte_ms": { "type": "integer" },
                    "post_mib": { "type": "integer" },
                    "fetch_times": { "type": "integer" },
                    "download_mib": { "type": "integer" },
                    "log_flood": { "type": "integer" },
                    "descriptor_dwell_ms": { "type": "integer" },
                    "mirror": { "type": "string" },
                    "username": { "type": "string" },
                },
                "additionalProperties": false,
            }),
            payload_paths: builds::payload_paths().into(),
            roles: vec![
                RoleClaim {
                    role: ROLE.into(),
                    reads: Vec::new(),
                },
                builds::role(),
            ],
            source_actions: vec![BOOK_REPAIR.into()],
            clone_from: Vec::new(),
            url_forms: Vec::new(),
        };
        serde_json::to_string(&descriptor).expect("a descriptor serializes")
    }

    fn test_connection(&self) -> Result<ConnectionInfo, SourceError> {
        self.healthy()?;
        flood(self.log_flood);
        if let Some(url) = &self.fetch {
            let mut trace = fetch::get(url, self.fetch_options);
            for _ in 1..self.fetch_times {
                if trace.error.is_some() {
                    break;
                }
                trace = fetch::get(url, self.fetch_options);
            }
            let mut answer = trace.to_json();
            answer["instance"] = self.received.clone();
            if trace.error.is_some() {
                return Err(SourceError::Unreachable(answer.to_string()));
            }
            return Ok(ConnectionInfo {
                account: None,
                server_version: None,
                secret_expires_at: None,
                detail: Some(answer.to_string()),
                discovered: Vec::new(),
            });
        }
        Ok(ConnectionInfo {
            account: Some("harbourmaster".into()),
            server_version: Some(format!("{KIND} 0.1.0")),
            secret_expires_at: None,
            detail: None,
            discovered: Vec::new(),
        })
    }

    fn sync(&self, cursor: Option<String>) -> Result<String, SourceError> {
        self.healthy()?;
        match self.fault {
            Ok(Fault::Spin) => spin(),
            Ok(Fault::Hog) => hog(),
            _ => {}
        }
        let from = match cursor.as_deref() {
            None => 0,
            Some(c) => c
                .parse::<usize>()
                .ok()
                .filter(|n| *n <= self.corpus().len())
                .ok_or_else(|| protocol(format!("cursor {c:?} is not one the fixture wrote")))?,
        };
        let swallow = matches!(self.fault, Ok(Fault::SwallowSink));
        let corpus = self.corpus();
        let walk = &corpus[from..];
        let of = walk.len() as u64;
        for (done, item) in (0u64..).zip(walk) {
            walked(done, of);
            match sink::item(item) {
                Err(_) if swallow => {}
                answer => answer?,
            }
        }
        walked(of, of);
        Ok(corpus.len().to_string())
    }

    /// [`BOOK_REPAIR`] on a survey answers a receipt and changes nothing;
    /// every other write is refused with a protocol error naming it.
    fn write(&self, op: String) -> Result<WriteReceipt, SourceError> {
        let refuse = || {
            protocol(format!(
                "the {KIND} adapter performs only {BOOK_REPAIR:?} on a survey, so it refuses {op}"
            ))
        };
        let parsed: WriteOp = serde_json::from_str(&op).map_err(|_| refuse())?;
        match parsed {
            WriteOp::SourceAction { entity, id, .. }
                if id == BOOK_REPAIR
                    && SURVEYS
                        .iter()
                        .any(|(k, _, _)| Some(*k) == self.key(&entity)) =>
            {
                Ok(WriteReceipt { remote_id: None })
            }
            _ => Err(refuse()),
        }
    }

    fn reachable_transitions(&self, _entity: String) -> Result<Vec<String>, SourceError> {
        Err(self.no_workflow())
    }

    /// A survey's block view, the custom role's whole surface (ADR-0029):
    /// the surveyor as a fact and the body as text, and [`BOOK_REPAIR`] as
    /// its one action (#1407). The run's is
    /// [`builds::view`]. `None` for the pipeline, and for an entity that is
    /// not the fixture's.
    fn view(&self, entity: String, section: Option<String>) -> Result<Option<String>, SourceError> {
        if self.is_run(&entity) {
            return builds::view(section.as_deref())
                .map(|view| Some(view.to_string()))
                .ok_or_else(|| protocol(format!("the run's view has no section {section:?}")));
        }
        let key = self.key(&entity);
        let Some((_, title, body)) = SURVEYS.iter().find(|(k, _, _)| Some(*k) == key) else {
            return Ok(None);
        };
        Ok(Some(
            serde_json::json!({
                "sections": [{
                    "id": "survey",
                    "title": title,
                    "count": null,
                    "deferred": false,
                    "blocks": [
                        { "type": "facts", "facts": [
                            { "label": "Surveyor", "value": "harbourmaster", "tone": null },
                        ] },
                        { "type": "text", "text": body, "markdown": false },
                    ],
                }],
                "actions": [{
                    "write_op": SOURCE_ACTION,
                    "label": "Book repair…",
                    "fields": [{
                        "name": "day",
                        "label": "Day",
                        "kind": { "type": "text", "suggestions": [] },
                        "value": null,
                        "required": true,
                        "placeholder": "Tuesday",
                    }],
                    "confirm": "Book a repair crew for this survey's finding?",
                    "primary": false,
                    "danger": false,
                    "disabled": null,
                    "values": { "id": BOOK_REPAIR },
                    "entity": null,
                }],
            })
            .to_string(),
        ))
    }

    fn pipeline_settings(&self, entity: String) -> Result<Option<String>, SourceError> {
        if self.key(&entity) != Some(builds::PIPELINE) {
            return Err(protocol(format!("{entity} is not the fixture's pipeline")));
        }
        Ok(Some(builds::pipeline_settings().to_string()))
    }

    /// One run older than the sync's window, `run-0`, before `run-1`.
    fn older_runs(
        &self,
        entity: String,
        before: String,
        count: u32,
    ) -> Result<Option<Vec<SyncItem>>, SourceError> {
        if self.key(&entity) != Some(builds::PIPELINE) {
            return Err(protocol(format!("{entity} is not the fixture's pipeline")));
        }
        let older = (self.is_run(&before) && count > 0)
            .then(|| self.run(builds::OLDER_RUN, "41"))
            .into_iter()
            .collect();
        Ok(Some(older))
    }

    /// The run's pipeline is in no chain.
    fn run_chain(&self, run: String) -> Result<Option<Vec<RunChainStep>>, SourceError> {
        self.the_run(&run)?;
        Ok(Some(Vec::new()))
    }

    fn run_log(
        &self,
        entity: String,
        from: Option<u64>,
        count: u32,
    ) -> Result<Option<String>, SourceError> {
        self.the_run(&entity)?;
        Ok(Some(builds::run_log(from, count).to_string()))
    }

    fn run_tests(&self, entity: String) -> Result<Option<String>, SourceError> {
        self.the_run(&entity)?;
        Ok(Some(builds::run_tests().to_string()))
    }

    fn test_history(&self, entity: String, _test: String) -> Result<Option<String>, SourceError> {
        self.the_run(&entity)?;
        Ok(Some(builds::test_history().to_string()))
    }

    fn run_artifacts(&self, entity: String, path: String) -> Result<Option<String>, SourceError> {
        self.the_run(&entity)?;
        builds::run_artifacts(&path)
            .map(|entries| Some(entries.to_string()))
            .ok_or_else(|| protocol(format!("the run has no artifact folder {path:?}")))
    }

    fn artifact_preview(
        &self,
        entity: String,
        path: String,
    ) -> Result<Option<String>, SourceError> {
        self.the_run(&entity)?;
        let text = builds::artifact_text(&path)
            .ok_or_else(|| protocol(format!("the run has no artifact file {path:?}")))?;
        Ok(Some(
            serde_json::json!({ "text": text, "truncated": false }).to_string(),
        ))
    }
}

impl Adapter {
    /// `Ok` for the fixture's run; a protocol error naming anything else.
    fn the_run(&self, entity: &str) -> Result<(), SourceError> {
        if self.is_run(entity) {
            Ok(())
        } else {
            Err(protocol(format!("{entity} is not a run of the fixture")))
        }
    }

    /// `Source::download` (1.1.0): the target's bytes into `download-sink`,
    /// one [`builds::CHUNK`] at a time.
    #[cfg(not(feature = "v1_0"))]
    fn download(&self, entity: &str, target: &str) -> Result<bool, SourceError> {
        use knobas::adapter::download_sink;
        self.the_run(entity)?;
        let target: serde_json::Value = serde_json::from_str(target)
            .map_err(|e| protocol(format!("the download target is not JSON: {e}")))?;
        let path = target["path"].as_str().unwrap_or_default();
        let (name, bytes): (String, Box<dyn Iterator<Item = Vec<u8>>>) = match target["kind"]
            .as_str()
        {
            Some("log") => {
                let text: String = builds::LOG
                    .iter()
                    .map(|(_, l, _)| format!("{l}\n"))
                    .collect();
                (
                    "run-42.log".into(),
                    Box::new(std::iter::once(text.into_bytes())),
                )
            }
            Some("file") => {
                let text = builds::artifact_text(path)
                    .ok_or_else(|| protocol(format!("the run has no artifact file {path:?}")))?;
                let name = path.rsplit('/').next().unwrap_or(path).to_owned();
                (name, Box::new(std::iter::once(text.as_bytes().to_vec())))
            }
            Some("folder" | "all") => {
                let chunks = self.download_mib * (1 << 20) / builds::CHUNK as u64;
                let chunk = vec![b'q'; builds::CHUNK];
                (
                    "artifacts.zip".into(),
                    Box::new((0..chunks).map(move |_| chunk.clone())),
                )
            }
            _ => {
                return Err(protocol(format!(
                    "the download target {target} is not one the fixture knows"
                )));
            }
        };
        download_sink::start(&name)?;
        for chunk in bytes {
            download_sink::write(&chunk)?;
        }
        Ok(true)
    }
}

#[cfg(not(feature = "v1_0"))]
impl exports::knobas::adapter::downloads::Guest for Fixture {
    fn download(instance: Instance, entity: String, target: String) -> Result<bool, SourceError> {
        <Adapter as GuestAdapter>::new(instance).download(&entity, &target)
    }
}

export!(Fixture);
