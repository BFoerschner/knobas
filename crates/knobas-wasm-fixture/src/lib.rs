//! The fixture external adapter (issue #1198, ADR-0028).
//!
//! A test adapter built to a WASM component against
//! `knobas:adapter@1.0.0`. It serves three surveys and nothing else, so that
//! the host can be tested with the contract battery and through the sync
//! engine into the mirror without a real product behind it. No external
//! adapter ships with knobas; this is what the host is tested with.
//!
//! * **The corpus** is [`SURVEYS`]. Every survey carries the word `quaywright`
//!   in its body, a token nothing else in knobas' fixtures spells, so a
//!   search for it finds these and only these.
//! * **The cursor** is the number of surveys already emitted, as a decimal
//!   string. A full sync emits all three and returns `"3"`; a sync from
//!   `"3"` emits nothing and returns `"3"`, as the battery's clause 2 asks.
//! * **The config** is `{"fault": "none" | "unauthorized" | "unreachable"}`,
//!   the battery's `Fault`. A faulted fixture fails `test_connection` and
//!   `sync` with the mapped error. A config it cannot read fails every call
//!   with a protocol error naming it.
//! * **The role** is the custom role [`ROLE`], and a survey's view is two
//!   blocks: who surveyed it, and its body (#1202).
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
//!   `"first_byte_ms"` and `"post_mib"` bend that request ([`fetch::Options`]). `mirror`
//!   and `username` are config keys it declares and never reads, for a
//!   manifest's host template and the host's basic auth.

mod fetch;

use exports::knobas::adapter::source::{Guest, GuestAdapter};
use knobas::adapter::sink;
use knobas::adapter::types::{
    ConnectionInfo, Fault as WitFault, Instance, RunChainStep, SourceError, SyncItem, WriteReceipt,
};

wit_bindgen::generate!({
    world: "adapter",
    path: "../knobas-wasm-host/wit",
});

/// The adapter kind this fixture is. A source's id is its own.
pub const KIND: &str = "wasm-fixture";

/// The custom role the fixture claims (ADR-0029): its surveys fit no built-in
/// role, so a survey opens the view `view` answers, and nothing else draws
/// them. Written out whole, not formatted from [`KIND`], so a test can find
/// the claim in the component's bytes (#1202's refusal of a role that is not
/// namespaced to the adapter).
pub const ROLE: &str = "wasm-fixture:survey-desk";

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

/// One instance, for one call.
pub struct Adapter {
    id: String,
    name: String,
    fault: Result<Fault, String>,
    /// The URL `test_connection` reads, when the config names one.
    fetch: Option<String>,
    fetch_options: fetch::Options,
    /// The instance record as the host handed it over, for the fetch answer.
    received: serde_json::Value,
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
        SyncItem {
            entity: format!("{}:{key}", self.id),
            kind: "survey".into(),
            title: title.into(),
            body_text: body.into(),
            author: Some("harbourmaster".into()),
            updated_at: Some("2026-10-18T09:00:00Z".into()),
            payload: serde_json::json!({ "key": key, "title": title }).to_string(),
            web_url: None,
            deleted: false,
        }
    }

    fn no_workflow(&self) -> SourceError {
        protocol(format!(
            "the {KIND} adapter has no workflow: source {:?} offers no transitions",
            self.id
        ))
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
        serde_json::json!({
            "id": self.id,
            "adapter_kind": KIND,
            "name": self.name,
            "capabilities": [],
            "adapter_version": "0.1.0",
            // A token, which the fixture never reads: every source takes a
            // credential at Add source, and the host attaches it (#1203).
            "auth_methods": ["Pat"],
            "accepts_account": false,
            "min_sync_interval_secs": 60,
            "write_ops": [],
            "entity_kinds": [{
                "id": "survey",
                "label": "Survey",
                "plural": "Surveys",
                "monogram": "SV",
                "full_sync_exhaustive": true,
            }],
            "config_schema": {
                "type": "object",
                "properties": {
                    "fault": { "type": "string", "enum": ["none", "unauthorized", "unreachable", "swallow_sink", "spin", "hog"] },
                    "fetch": { "type": "string" },
                    "first_byte_ms": { "type": "integer" },
                    "post_mib": { "type": "integer" },
                    "mirror": { "type": "string" },
                    "username": { "type": "string" },
                },
                "additionalProperties": false,
            },
            "payload_paths": [],
            "roles": [{ "role": ROLE, "reads": [] }],
        })
        .to_string()
    }

    fn test_connection(&self) -> Result<ConnectionInfo, SourceError> {
        self.healthy()?;
        if let Some(url) = &self.fetch {
            let trace = fetch::get(url, self.fetch_options);
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
                .filter(|n| *n <= SURVEYS.len())
                .ok_or_else(|| protocol(format!("cursor {c:?} is not one the fixture wrote")))?,
        };
        let swallow = matches!(self.fault, Ok(Fault::SwallowSink));
        for survey in &SURVEYS[from..] {
            match sink::item(&self.survey(*survey)) {
                Err(_) if swallow => {}
                answer => answer?,
            }
        }
        Ok(SURVEYS.len().to_string())
    }

    fn write(&self, op: String) -> Result<WriteReceipt, SourceError> {
        Err(protocol(format!(
            "the {KIND} adapter declares no write ops, so it refuses {op}"
        )))
    }

    fn reachable_transitions(&self, _entity: String) -> Result<Vec<String>, SourceError> {
        Err(self.no_workflow())
    }

    /// A survey's block view, the custom role's whole surface (ADR-0029):
    /// the surveyor as a fact and the body as text. `None` for an entity
    /// that is not one of the fixture's surveys.
    fn view(
        &self,
        entity: String,
        _section: Option<String>,
    ) -> Result<Option<String>, SourceError> {
        let key = entity.strip_prefix(&format!("{}:", self.id));
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
                "actions": [],
            })
            .to_string(),
        ))
    }

    fn pipeline_settings(&self, _entity: String) -> Result<Option<String>, SourceError> {
        Ok(None)
    }

    fn older_runs(
        &self,
        _entity: String,
        _before: String,
        _count: u32,
    ) -> Result<Option<Vec<SyncItem>>, SourceError> {
        Ok(None)
    }

    fn run_chain(&self, _run: String) -> Result<Option<Vec<RunChainStep>>, SourceError> {
        Ok(None)
    }

    fn run_log(
        &self,
        _entity: String,
        _from: Option<u64>,
        _count: u32,
    ) -> Result<Option<String>, SourceError> {
        Ok(None)
    }

    fn run_tests(&self, _entity: String) -> Result<Option<String>, SourceError> {
        Ok(None)
    }

    fn test_history(&self, _entity: String, _test: String) -> Result<Option<String>, SourceError> {
        Ok(None)
    }

    fn run_artifacts(&self, _entity: String, _path: String) -> Result<Option<String>, SourceError> {
        Ok(None)
    }

    fn artifact_preview(
        &self,
        _entity: String,
        _path: String,
    ) -> Result<Option<String>, SourceError> {
        Ok(None)
    }
}

export!(Fixture);
