# The `knobas:adapter` interface, 1.5.0

An external adapter is one WebAssembly component that knobas runs under wasmtime as one more source. Elsewhere this is called a WASM plugin; knobas does not use that word. The component is built against the WIT package `knobas:adapter`, and this page is the reference for its version 1.5.0: every interface, every function, which Rust type each JSON string is, when the host calls it and what the host does with the answer.

What an adapter says about itself is on its own page, [descriptor.md](descriptor.md). How to start is in [quickstart.md](../quickstart.md).

Source: `crates/knobas-wasm-host/wit/adapter.wit`, the one copy of the current WIT. The frozen older worlds are `crates/knobas-adapter-sdk/wit-1.0.0/adapter.wit` and `crates/knobas-adapter-sdk/wit-1.3.0/adapter.wit`. Decision: [ADR-0028](../../adr/0028-an-external-adapter-is-a-sandboxed-wasm-component-installed-from-a-release.md).

## Package and worlds

```wit
package knobas:adapter@1.5.0;

world hosted {
    import sink;
    import log;
    import download-sink;
    export source;
}

world adapter {
    include hosted;
    export downloads;
    export query;
    export refresh;
    export docs;
}
```

`adapter` is the world an external adapter is built against. The Rust SDK generates its bindings from it.

`hosted` is the part the host binds statically. Its one export, `source`, is the only export a component must have. The host looks up `downloads`, `query`, `refresh` and `docs` one by one at load, so a component that leaves any of them out still links and gets that read's default. The host provides `sink`, `log` and `download-sink` to every component, and a component imports only the ones it uses.

The host reads the component's interface version from the name of its `knobas:adapter/source@<version>` export. A component that exports no `knobas:adapter/source` is refused with `the component exports no knobas:adapter/source, so it is not a knobas adapter`.

Source: `interface_version`, `optional_export` and `WasmAdapter::from_bytes` in `crates/knobas-wasm-host/src/lib.rs`.

## How the SPI's types cross

knobas' adapter SPI is the `Source` trait in `crates/knobas-source/src/lib.rs`. The WIT is a transcription of it. Two kinds of type cross:

- WIT records and variants, for the types the host branches on: the instance, a sync item, the connection info, a write receipt, a run-chain step, the error and the refresh answer.
- `type json = string`, for every other SPI type. The string is exactly what `serde_json` writes for the named Rust type in `knobas-source`, and the host decodes it with that type's own `Deserialize`. Each `json` parameter or result below names its type.

A `json` answer that does not decode fails the call with `SourceError::Protocol` and the message `the adapter's <what> does not decode: <serde error>`, where `<what>` is `descriptor`, `view`, `pipeline settings`, `run log`, `run tests`, `test history`, `run artifacts`, `artifact preview` or `query translation`. A docs projection that does not decode is logged and drawn as the source's failure instead.

| Where | Direction | Rust type of the JSON |
|---|---|---|
| `instance.config` | to the component | the object `config_schema` describes, as stored in `source_config.config` |
| `sync-item.payload` | from the component | `serde_json::Value`, the raw source record |
| `adapter.descriptor` result | from | `knobas_source::SourceDescriptor` |
| `adapter.write` parameter `op` | to | `knobas_source::WriteOp` |
| `adapter.view` result | from | `knobas_source::view::SourceView` |
| `adapter.pipeline-settings` result | from | `knobas_source::pipeline::PipelineSettings` |
| `adapter.run-log` result | from | `knobas_source::run::RunLog` |
| `adapter.run-tests` result | from | `knobas_source::run::RunTests` |
| `adapter.test-history` result | from | a list of `knobas_source::run::TestOutcome` |
| `adapter.run-artifacts` result | from | a list of `knobas_source::run::ArtifactEntry` |
| `adapter.artifact-preview` result | from | `knobas_source::run::ArtifactPreview` |
| `downloads.download` parameter `target` | to | `knobas_source::run::DownloadTarget` |
| `query.translate-query` parameter `saved-view` | to | the saved view's payload as this adapter emitted it |
| `query.translate-query` result | from | `knobas_source::query::Translation` |
| `docs.project` parameter `payload` | to | a record's payload as this adapter emitted it |
| `docs.project` result | from | `knobas_source::docs::DocContent` |
| `docs.edit-section` parameter `payload` | to | a record's payload as this adapter emitted it |

Source: `crates/knobas-wasm-host/src/convert.rs`, `crates/knobas-wasm-host/src/source.rs`.

## What each call runs in

The host instantiates the component afresh for every call, constructs its `adapter` resource from the instance, calls one function and drops the store. Nothing the component keeps outlives the call. An adapter carries state from one call to the next only in its cursor. A component that trapped leaves nothing behind.

Every call has a wall-clock deadline and a memory cap. The deadline covers instantiation, the component's own work and every host call it waits on, the sink and HTTP requests included. The memory cap bounds the linear memories and tables of one call's store together, and it also bounds an outgoing request body, which the host buffers whole.

| Limit | Default | Most a manifest may ask for |
|---|---|---|
| Deadline per call | 600 s | 3600 s |
| Memory per call | 256 MiB | 1024 MiB |
| Deadline of a `docs` call | the adapter's deadline, held to at most 5 s | |
| One log line | cut to 4 KiB | |
| Log lines per call | 100, then one per second; the rest dropped and counted | |

A call that passes the deadline or the cap fails with `SourceError::Protocol` and `the external adapter "<kind>" was stopped at <limit>`. A trap fails with `SourceError::Protocol` and `the external adapter "<kind>" failed: <trap>`. When the call is a sync, its run ends failed. The manifest's `limits` field is described in [release.md](../release.md).

Source: the module docs of `crates/knobas-wasm-host/src/lib.rs`, `crates/knobas-wasm-host/src/limits.rs`, and `failed` in `crates/knobas-wasm-host/src/source.rs`.

## What the component may reach

The host links all of WASI 0.2 and gives it an empty context: no preopened directory, no environment variable, no argument, stdio that goes nowhere, and a socket and name-lookup policy that refuses every address. Anything a component prints to stdout or stderr is lost; write to `log` instead. The wasip2 clocks work, and the fixture component waits on `wasi:clocks/monotonic-clock`.

The only way out is `wasi:http` 0.2's outgoing handler. The host lets a request leave only for a host the adapter's manifest declares, attaches the source's credential itself to requests for the source's own address, redacts the secret out of responses from that address, and paces them on the instance's rate limit. The component never sees the secret. [http.md](../http.md) covers this in full.

Two kinds of call get no way out at all. Their policy allows no host, so every request is refused before a socket opens, whatever the manifest declares:

- every `docs` call;
- the two `descriptor` calls at load, before any source exists.

A component that imports anything the host does not provide fails to link and is refused at load with `the component cannot be linked: ...`.

Source: `HostState::new` and `from_bytes` in `crates/knobas-wasm-host/src/lib.rs`, the module docs of `crates/knobas-wasm-host/src/net.rs`, `docs_store` and `probe_store` in `crates/knobas-wasm-host/src/source.rs`, and the fixture's `descriptor` in `crates/knobas-wasm-fixture/src/lib.rs`.

## Interface `types`

```wit
type json = string;
type cursor = string;
record fault { status: option<u16>, message: string }
record moved { base: string, current: string }
variant source-error {
    unauthorized(option<u16>),
    unreachable(string),
    unconfirmed(fault),
    protocol(fault),
    sink(string),
    moved(moved),
}
record instance { id: string, kind: string, display-name: string, base-url: string, config: json }
record sync-item {
    entity: string, kind: string, title: string, body-text: string,
    author: option<string>, updated-at: option<string>, payload: json,
    web-url: option<string>, deleted: bool,
}
record connection-info {
    account: option<string>, server-version: option<string>,
    secret-expires-at: option<string>, detail: option<string>,
    discovered: list<tuple<string, string>>,
}
record run-chain-step { pipeline: string, name: string, run: option<sync-item> }
record write-receipt { remote-id: option<string> }
```

`cursor` is `knobas_source::Cursor`, an opaque position the adapter defines. knobas stores it and hands it back.

`source-error` is `knobas_source::SourceError`, variant for variant. Which variant to raise when is in [descriptor.md](descriptor.md#sourceerror). It crosses as this WIT variant, not as JSON. For comparison, the Rust type's serde form, which a test or a log may show, is `{"Unauthorized":{"status":401}}`, `{"Unreachable":"connection refused"}`, `{"Protocol":{"status":422,"message":"no such space"}}` or `{"Protocol":{"status":null,"message":"page 4711 has no version"}}`.

`instance` is `knobas_source::instance::SourceInstance` without its auth method, secret and account. `id` is the instance id the person chose, and the namespace of every entity the source emits. `kind` is the adapter kind. `config` is JSON text; the SDK reads text that is not JSON as `null`.

`sync-item` is `knobas_source::SyncItem`. The host converts each one and fails the call with `SourceError::Protocol` when:

- `entity` does not parse as an `EntityRef`: `item entity "<text>" is not an entity id: <why>`;
- `payload` is not JSON: `item <entity> payload is not JSON: <why>`;
- `updated-at` is not RFC 3339: `item updated-at "<text>" is not RFC 3339: <why>`.

An `EntityRef` is `<namespace>:<key>`, split on the first `:`, both halves holding something other than whitespace. The key may hold further `:` and `#`. The namespace must be the instance id; the contract battery checks that, not the host. The host does not check that `kind` is a declared entity kind either; the battery does. See [sync.md](../sync.md).

A sync item as it crosses, for a wiki page:

| Field | Value |
|---|---|
| `entity` | `"tidewiki:4711"` |
| `kind` | `"page"` |
| `title` | `"Spring tides"` |
| `body-text` | `"Spring tides"` |
| `author` | `some("hal")` |
| `updated-at` | `some("2026-11-12T09:30:00Z")` |
| `payload` | `"{\"id\":\"4711\",\"title\":\"Spring tides\",\"space\":{\"key\":\"OPS\",\"name\":\"Operations\"},\"ancestors\":[]}"` |
| `web-url` | `some("https://wiki.example.org/page/4711")` |
| `deleted` | `false` |

`connection-info` is `knobas_source::ConnectionInfo`. `secret-expires-at` must be RFC 3339 or `test-connection` fails with `SourceError::Protocol`. `discovered` becomes a map keyed by `config_schema` property, described under [`config_schema`](descriptor.md#config_schema).

`run-chain-step` is `knobas_source::pipeline::RunChainStep`: `pipeline` is the step's pipeline as an `EntityRef` string, and `run` its run, which converts as a sync item does. `write-receipt` is `knobas_source::WriteReceipt`; [writes.md](../writes.md) says when `remote-id` is set.

Source: `crates/knobas-wasm-host/wit/adapter.wit`, `crates/knobas-wasm-host/src/convert.rs`, `crates/knobas-spi-types/src/entity.rs`, `crates/knobas-adapter-sdk/src/convert.rs`.

## Interface `sink`, imported

Where a syncing adapter hands its items. It is `knobas_source::Sink`, owned by the host.

### `item: func(item: sync-item) -> result<_, source-error>`

Hands one item to the sync and waits until the engine has answered it, so one item is in flight at a time. Called only during `sync`; outside one it answers `protocol` with `sink.item was called outside a sync`.

The host converts the item first. A `page` item, when the component exports `docs`, gets its search text rebuilt from the projection before the engine sees it, as [`docs`](#interface-docs-exported-optional-since-150) says.

An error here is the sink's, and the adapter returns it from `sync` unchanged. The host also remembers the first error, whether the engine refused the item or the item did not convert: from then on every `item` call answers that error, and the sync fails with it whatever `sync` returns. A component that swallows the error and returns a cursor still fails the run.

### `walked: func(done: u64, of: option<u64>, unit: string)`

Since 1.3.0. How far the run's outer walk has got: `done` units of `of`, with `none` when the adapter cannot know the total. `unit` is a plural noun the UI prints as it is, such as `"repositories"`. Call it before each unit and once after the last, so a run that hands no item still moves a progress bar. It returns nothing and cannot fail the sync. The host ignores it outside a sync and after the sink has failed. A component built against 1.0.0 to 1.2.0 does not import it and reports nothing.

Source: `sink::Host` and `WasmSource::sync` in `crates/knobas-wasm-host/src/source.rs`, `Sink` in `crates/knobas-source/src/lib.rs`.

## Interface `download-sink`, imported, since 1.1.0

Where a download's bytes go: `knobas_source::run::DownloadSink`, owned by the host, which writes each chunk to the file as it arrives. The adapter streams into it, so neither side holds a download whole.

### `start: func(file-name: string) -> result<_, source-error>`

Once, before the first byte: the file name the source suggests. The host does not trust it as a path.

### `write: func(bytes: list<u8>) -> result<_, source-error>`

The next bytes, in order. Each call waits until the host has written them.

Called only during `downloads.download`; outside one, either function answers `protocol` with `download-sink was called outside a download`. A second `start` or a `write` before `start` fails the download with `protocol`. As with `sink.item`, the first error is remembered and the download fails with it whatever `download` returns.

Source: `DownloadSlot` in `crates/knobas-wasm-host/src/source.rs`.

## Interface `log`, imported

### `log: func(level: level, message: string)`

`level` is one of `trace`, `debug`, `info`, `warn`, `error`. The line goes into knobas' own log, tagged with the source id, or with the adapter kind for a call that has no source, such as a `docs` call. It is cut to 4 KiB, and a call writes 100 lines and then one per second; the host drops the rest and logs how many it dropped.

Source: `log::Host` in `crates/knobas-wasm-host/src/source.rs`, `LogBudget` in `crates/knobas-wasm-host/src/limits.rs`.

## Interface `source`, exported, required

`knobas_source::Source`, one function per trait method that crosses, in the trait's order. Everything is on one resource:

```wit
resource adapter {
    constructor(instance: instance);
    descriptor: func() -> json;
    test-connection: func() -> result<connection-info, source-error>;
    sync: func(cursor: option<cursor>) -> result<cursor, source-error>;
    write: func(op: json) -> result<write-receipt, source-error>;
    reachable-transitions: func(entity: string) -> result<list<string>, source-error>;
    view: func(entity: string, section: option<string>) -> result<option<json>, source-error>;
    pipeline-settings: func(entity: string) -> result<option<json>, source-error>;
    older-runs: func(entity: string, before: string, count: u32) -> result<option<list<sync-item>>, source-error>;
    run-chain: func(run: string) -> result<option<list<run-chain-step>>, source-error>;
    run-log: func(entity: string, %from: option<u64>, count: u32) -> result<option<json>, source-error>;
    run-tests: func(entity: string) -> result<option<json>, source-error>;
    test-history: func(entity: string, test: string) -> result<option<json>, source-error>;
    run-artifacts: func(entity: string, path: string) -> result<option<json>, source-error>;
    artifact-preview: func(entity: string, path: string) -> result<option<json>, source-error>;
}
```

The host binds this interface statically, so a component must export every one of these functions. The ones that are optional reads answer `none` when the adapter has nothing to say; the Rust SDK gives them that default. Every `entity` parameter is an `EntityRef` in string form, in the instance's namespace.

| Function | In the Rust SDK's `Adapter` |
|---|---|
| `constructor` | `new`, which you write |
| `descriptor` | you write it |
| `test-connection` | you write it |
| `sync` | you write it |
| `write` | you write it |
| `reachable-transitions` | you write it |
| `view` | you write it; `Ok(None)` for every kind is a valid answer |
| `pipeline-settings` | defaults to `Ok(None)` |
| `older-runs` | defaults to `Ok(None)` |
| `run-chain` | defaults to `Ok(None)` |
| `run-log` | defaults to `Ok(None)` |
| `run-tests` | defaults to `Ok(None)` |
| `test-history` | defaults to `Ok(None)` |
| `run-artifacts` | defaults to `Ok(None)` |
| `artifact-preview` | defaults to `Ok(None)` |

### `constructor(instance: instance)`

Called at the start of every call on the resource. The resource lives for that one call.

### `descriptor: func() -> json`

The `SourceDescriptor` as JSON, described in [descriptor.md](descriptor.md).

The host calls it at load, twice, with no network: first with an instance whose `id` and `kind` are both `wasm-probe`, then with `id` and `kind` both set to the `adapter_kind` the first answer named. Both probes carry an empty `base-url` and `config` `{}`. The second answer is the adapter's template: the descriptor the install checks read, and the one whose `config_schema` the host resolves a source's allowed hosts over. The host calls it again each time it builds a source from a configured instance, with that instance and the source's network policy, and refuses the build with `protocol` when the answer's `id` is not the instance's id or its `adapter_kind` is not the instance's kind.

When the component exports no `refresh`, the host removes `Refresh` from the descriptor's capabilities whatever the JSON says. A descriptor that does not decode refuses the load with `the component's descriptor is refused: ...`.

### `test-connection: func() -> result<connection-info, source-error>`

Reach the source with the configured credential and say what answered. Called when a source is added, when its secret is re-entered, and by the credential-health poll. Classify failures as `sync` does.

### `sync: func(cursor: option<cursor>) -> result<cursor, source-error>`

Push every item changed since `cursor` through `sink.item`, then return the new cursor. `none` is a full sync. A sync that emitted nothing returns the cursor it was given, unchanged. The deadline holds the whole sync, the time the engine takes over each item included. What a full sync must emit and how cursors behave is in [sync.md](../sync.md).

### `write: func(op: json) -> result<write-receipt, source-error>`

Perform one write. `op` is a `WriteOp` as JSON, externally tagged, so the variant name is the object's one key:

```json
{"UpdatePage":{"entity":"tidewiki:4711","base_version":7,"body":"<h2>Tides</h2><p>Spring tides run high.</p>"}}
```

```json
{"Comment":{"entity":"tidewiki:4711","body":"Checked against the 2026 tables."}}
```

Before the call crosses, the host checks the op against the descriptor the source was built with. An op whose identifier is not in `write_ops`, or a source action whose id is not in `source_actions`, is refused with `protocol` and never reaches the component: `source "<id>" does not declare the write "<identifier>", so it was not sent`. Every variant and its fields are in [descriptor.md](descriptor.md#write_ops); how to perform them is in [writes.md](../writes.md).

### `reachable-transitions: func(entity: string) -> result<list<string>, source-error>`

The statuses the entity's workflow offers from where it stands now, in the source's own spelling. Read when a detail opens; nothing stores it. It has no default on purpose: an adapter without a workflow refuses it with `protocol`, naming itself. The contract battery holds a source that does not declare `transition` to that refusal, and the ticket-system role requires the read.

### `view: func(entity: string, section: option<string>) -> result<option<json>, source-error>`

The source view of one of the adapter's entities, as a `view::SourceView`: sections of typed blocks from the fixed vocabulary in `crates/knobas-source/src/view.rs`. Read live when a detail opens. `section: none` asks for the whole view, where a deferred section comes with its id, title and count and no blocks. `section: some(id)` asks for that one section, answered as a view holding exactly it; an id the kind's view does not have is refused with `protocol`. `none` means the adapter draws no view for this kind, and the detail draws the generic payload projection. A failed read must be an error, never `none`.

### `pipeline-settings: func(entity: string) -> result<option<json>, source-error>`

A pipeline's settings, as a `pipeline::PipelineSettings`: the repositories it builds, its triggers and its parameters, scrubbed of credentials. `none` means the adapter keeps no pipelines. CI role, optional read `pipeline_settings`.

### `older-runs: func(entity: string, before: string, count: u32) -> result<option<list<sync-item>>, source-error>`

Runs of the pipeline `entity` older than the run `before`, newest first, at most `count`, capped at `pipeline::OLDER_RUNS_MAX`, 100. Each run is the sync item the sync would emit for it; nothing is stored. `before` is not itself in the answer, and an empty list is the start of the history. Asked when the reader scrolls past the window the sync keeps. CI role, optional read `older_runs`.

### `run-chain: func(run: string) -> result<option<list<run-chain-step>>, source-error>`

The chain the run is in: one step per step of its pipeline's chain, in order, each with its run on the run's branch. Empty for a run in no chain. CI role, optional read `run_chain`.

### `run-log: func(entity: string, %from: option<u64>, count: u32) -> result<option<json>, source-error>`

A range of the run's log and its steps, as a `run::RunLog`: `count` lines, capped at `run::LOG_LINES_MAX`, 20,000, from line `from`, counted from 1, or the last `count` lines when `from` is `none`. Stream the log and keep only the range. CI role, required read `run_log`.

### `run-tests: func(entity: string) -> result<option<json>, source-error>`

The run's tests, failed first, as a `run::RunTests`. CI role, optional read `run_tests`.

### `test-history: func(entity: string, test: string) -> result<option<json>, source-error>`

How the test `test`, a `run::RunTest` id, came out in the last 10 runs of the run's pipeline, newest first, as a list of `run::TestOutcome`. CI role, optional read `test_history`.

### `run-artifacts: func(entity: string, path: string) -> result<option<json>, source-error>`

The entries of one folder of the run's artifacts, `""` for the root, as a list of `run::ArtifactEntry`. CI role, optional read `run_artifacts`.

### `artifact-preview: func(entity: string, path: string) -> result<option<json>, source-error>`

The head of one artifact file, for *Preview*, as a `run::ArtifactPreview`. CI role, optional read `artifact_preview`.

Source: `WasmSource` and `descriptor` in `crates/knobas-wasm-host/src/source.rs`; `from_bytes`, `build` and `describe` in `crates/knobas-wasm-host/src/lib.rs`; `Source` and `SourceDescriptor::declares` in `crates/knobas-source/src/lib.rs`; `crates/knobas-source/src/pipeline.rs`; `crates/knobas-source/src/run.rs`; `Adapter` in `crates/knobas-adapter-sdk/src/lib.rs`.

## Interface `downloads`, exported, optional, since 1.1.0

```wit
download: func(instance: instance, entity: string, target: json) -> result<bool, source-error>;
```

`Source::download`: stream `target` of run `entity` into `download-sink`, reading from the source and writing nothing to it. `target` is a `run::DownloadTarget`, tagged on `kind`:

```json
{"kind":"log"}
{"kind":"file","path":"reports/junit.xml"}
{"kind":"folder","path":"reports"}
{"kind":"all"}
```

`false` means the adapter keeps no runs. A free function, not a method of `adapter`: it takes the instance as its first parameter. A component that does not export it is never called, and its source answers `false`. CI role, optional read `download`.

Source: `WasmSource::download` in `crates/knobas-wasm-host/src/source.rs`, `DownloadTarget` in `crates/knobas-source/src/run.rs`.

## Interface `query`, exported, optional, since 1.2.0

```wit
translate-query: func(instance: instance, saved-view: json) -> result<option<json>, source-error>;
```

`Source::translate_query`: a saved view's query read into knobas' search grammar, for *Make an editable copy*. `saved-view` is the payload this adapter emitted for a kind that declares the `saved_view` frame. The answer is a `query::Translation`:

```json
{"conditions":[{"key":"under","negated":true,"values":["PAY-12"]}],"kept":1,"left_out":[{"clause":"fixVersion is EMPTY","reason":"knobas does not sync fix versions"}]}
```

`key` is one of `project`, `status`, `issuetype`, `priority`, `assignee`, `under`, `label`, `sprint`, `has`, `updated`, `assigned`. `none` means the adapter does not translate this view's query, and the view still shows without an editable copy. The trait documents it as pure: it reads nothing from the source. A component that does not export it translates nothing. Ticket-system role, optional read `translate_query`.

Source: `WasmSource::translate_query` in `crates/knobas-wasm-host/src/source.rs`, `crates/knobas-source/src/query.rs`.

## Interface `refresh`, exported, optional, since 1.4.0

```wit
variant refresh-answer {
    item(sync-item),
    gone,
    not-offered,
}
refresh: func(instance: instance, entity: string) -> result<refresh-answer, source-error>;
```

`Source::refresh`: read one item by id. `entity` is an `EntityRef` string in the instance's namespace. knobas calls it after a write lands, on the item the write's receipt names or else the write's target, before it runs the source for what the write changed elsewhere. It also calls it when the reader presses *Refresh* on an item's detail. It never overlaps a run of the same source.

- `item` is the item as the source holds it now, in the shape `sync` hands to `sink.item`: same entity, kind, title and declared paths. The host converts it as a sync item, rebuilding a page's search text, and writes it through the run's own item write.
- `gone` means the source no longer has the item; the engine tombstones it.
- `not-offered` means the adapter does not read this item's kind by id; knobas runs the source instead.

A failed read is an error with `sync`'s fault classification, never `gone`. An adapter that answers anything but `not-offered` for some kind declares `Capability::Refresh`. A component that does not export `refresh` is never called: its source answers `not-offered` for every id, its descriptor loses `Refresh`, and its item details offer *Sync now*. See [descriptor.md](descriptor.md#capabilities) for the per-kind flag.

Source: `WasmSource::refresh` and `descriptor` in `crates/knobas-wasm-host/src/source.rs`, `Source::refresh` and `RefreshAnswer` in `crates/knobas-source/src/lib.rs`, [ADR-0031](../../adr/0031-refresh-is-an-optional-read-of-one-item-serialized-behind-the-run.md).

## Interface `docs`, exported, optional, since 1.5.0

The docs projection and the docs writes. All three are free functions with no instance, called with no HTTP capability under a deadline of at most 5 s. A component that exports `docs` must export all three; one that exports the interface without one of them is refused at load with `the component cannot be linked: knobas:adapter/docs exports no <function>`. Install refuses a descriptor that claims the `wiki` role from a component without `docs`. A component built against 1.4.0 or earlier draws no page. The guide is [docs-sources.md](../docs-sources.md).

### `project: func(kind: string, payload: json) -> option<json>`

One mirrored record drawn as a docs page. `payload` is the record's payload as this adapter emitted it, of entity kind `kind`. The answer is a `docs::DocContent`, the docs block vocabulary in its serde form. `none` is a kind the adapter draws no page for, or a payload it cannot read.

The host calls it when a page is opened, and at sync and refresh for every live `page` item, to build the item's search text with the shared `docs::search_text` function. The `body-text` the adapter sent for a `page` is then ignored; a page the projection does not draw is found by its title alone. The space in the search text is the page's declared `project_key`.

Before the answer is drawn, the host clears what knobas fills at read time: an include's `entity_id`, `blocks` and `unfilled`, a page link's `entity_id`, a ticket reference's `entity_id`, `title` and `status`, and a mention's `me`. A URL embed whose `url` is not an absolute `http` or `https` URL with a host becomes a placeholder. A call that traps, passes a limit or answers what does not decode is logged and returns no content, and the page draws as the source's failure.

A `DocContent` fragment. `labels`, `blocks` and `comments` are required keys; the other fields may be left out:

```json
{
  "version": 7,
  "editor_name": "Hal Master",
  "labels": ["harbour"],
  "blocks": [
    {"type": "heading", "level": 2, "text": [{"type": "text", "text": "Tides"}]},
    {"type": "paragraph", "text": [
      {"type": "text", "text": "Spring tides run "},
      {"type": "text", "text": "high", "strong": true},
      {"type": "text", "text": "."}
    ]},
    {"type": "panel", "kind": "warning", "title": null, "blocks": [
      {"type": "paragraph", "text": [{"type": "text", "text": "Buoy 3 is unlit."}]}
    ]}
  ],
  "comments": [],
  "sections": [{"number": 1, "text": "Spring tides run high.", "refusal": null}]
}
```

The WIT comments name the filled fields in WIT spelling, `entity-id`. In the JSON they are snake case, `entity_id`, as everywhere in `DocContent`.

### `edit-section: func(kind: string, payload: json, number: u32, text: string) -> result<section-edit, edit-refused>`

```wit
enum section-refusal { table, code, %macro, formatting }
variant edit-refused { section(section-refusal), missing }
record section-edit { body: string, base-version: s64 }
```

Section `number` of the record, numbered from 1 for the first top-level heading as the projection's `sections` number them, its body replaced with `text`, the editor's plain text in the spelling of the section's `text`, and everything else kept byte for byte. The answer is the record's whole new body in the source's markup and the version it was made against. The host queues it as an `update_page` write, which `adapter.write` then performs; this function makes markup and sends nothing. `section(...)` refuses a section knobas does not edit, for the reason the projection gave it. `missing` is a record with no such section, no body or no version. A failure is logged and the edit is not made.

### `body-from-text: func(text: string) -> string`

A new page's body in the source's markup from the editor's plain text: `## ` starts a section, `- ` a list item, a blank line a paragraph. The host queues it as a `create_page` write. The Rust SDK's default is `html_from_text`, plain HTML with the text escaped.

Source: `project`, `settle`, `edit_section`, `body_from_text` and `WasmSource::indexed` in `crates/knobas-wasm-host/src/source.rs`; `check_docs` in `crates/knobas-wasm-host/src/manifest.rs`; `crates/knobas-source/src/docs.rs`; `Adapter::project` and its neighbours in `crates/knobas-adapter-sdk/src/lib.rs`; [ADR-0040](../../adr/0040-a-docs-source-projects-at-read-time-through-a-shared-reader.md).

## What the interface does not carry

The WIT transcribes most of the `Source` trait, not all of it. These trait methods have no WIT function, so every external adapter gets the trait's default:

| Trait method | What an external adapter therefore answers |
|---|---|
| `undeclared_reason` | `None`: an undeclared write is refused with the generic wording |
| `refused_writes`, `refusal_standing` | nothing refused; asked every time |
| `view_standing`, `bytes_standing` | `Live`: no view is kept in memory |
| `file_bytes` | `None`: reads no file bytes |
| `artifact_bytes` | `None`: reads no artifact bytes |
| `attachment_bytes`, `download_attachment` | `None`, `false`: reads no attachments |

The `Sink` trait's `walked_whole` and `left_out` have no import either. An external adapter cannot tell the engine it walked a kind whole on a run that was handed a cursor, and cannot report a part of the source it left out.

Source: `Source` and `Sink` in `crates/knobas-source/src/lib.rs`, and the `Source` impl in `crates/knobas-wasm-host/src/source.rs`, which forwards nothing else.

## Version history

| Version | Added | Issue |
|---|---|---|
| 1.0.0 | `types`, `sink.item`, `log`, `source` with the `adapter` resource and all 15 of its functions | #1198 |
| 1.1.0 | the `download-sink` import and the `downloads` export | #1205 |
| 1.2.0 | the `query` export, `translate-query` | #1208 |
| 1.3.0 | `sink.walked` | #1380 |
| 1.4.0 | the `refresh` export | #2101 |
| 1.5.0 | the `docs` export: `project`, then `edit-section` and `body-from-text` | #2442, #2443 |

A minor version only adds, and every change is a §10.8 entry in `docs/contract.md`, like any frozen surface. Each addition is an import an older component does not use or an export the host looks up as optional, which is how an older component keeps linking.

The Rust SDK's crate version is the interface version it binds; a test holds the two together. Its `v1_0` and `v1_3` features build against the frozen 1.0.0 and 1.3.0 worlds, for the host's own tests of older components. An adapter has no use for them.

Source: `crates/knobas-wasm-host/wit/adapter.wit`, `crates/knobas-adapter-sdk/Cargo.toml`, `crates/knobas-adapter-sdk/src/lib.rs`.

## Hosting rule

A knobas release hosts one major version of `knobas:adapter`. This one hosts 1.x up to 1.5.0. A component built against the same major and a minor no newer than the host's is hosted; the patch does not count. Every other is refused at load with `the component is built against knobas:adapter@<found>, and this knobas hosts knobas:adapter@1.x up to 1.5.0`.

The host finds an optional export by asking for `knobas:adapter/<interface>@1.5.0`, and wasmtime's semver-compatible lookup finds the interface a component built against any 1.x exports. An interface of another major or another name is not found. When the component does export an optional interface, the host checks at load that each function has the type 1.5.0 gives it, and refuses the load otherwise, for example `its refresh does not have the type knobas hosts: ...`.

The release manifest names the interface version too. Install refuses a manifest whose `interface_version` this knobas cannot host, and an artifact built against any other version than the one the manifest names, patch included. See [release.md](../release.md).

Source: `INTERFACE_VERSION`, `Version::is_hosted` and `optional_export` in `crates/knobas-wasm-host/src/lib.rs`; `check_download`, `check_refresh`, `check_project` and `check_doc_writes` in `crates/knobas-wasm-host/src/source.rs`; `check` in `crates/knobas-wasm-host/src/manifest.rs`.

## Building without the Rust SDK

Only the Rust SDK, `crates/knobas-adapter-sdk`, is maintained. Nothing else in the knobas repository builds, tests or documents a component in another language, and the fixture the host is tested with is written against the SDK.

The host does not care how a component was made. To build one without the SDK:

1. Produce a WASI 0.2 component for `wasm32-wasip2` that exports `knobas:adapter/source@1.5.0`, with every function of the `adapter` resource. A world that includes `hosted` and exports only the optional interfaces you implement is enough; the full `adapter` world exports all of them.
2. Import nothing beyond WASI 0.2, `wasi:http` 0.2's types and outgoing handler, and `knobas:adapter`'s `sink`, `log` and `download-sink`.
3. Write every `json` answer in the exact serde form of its Rust type. The serde attributes in `crates/knobas-source/src/lib.rs`, `crates/knobas-spi-types/src/payload.rs` and the modules each type lives in are the specification. Enums are externally tagged unless the type says otherwise; `DocBlock`, `Inline` and `view::Block` are tagged on `type`, `DownloadTarget` and `EmbedContent` on `kind`. Every JSON example on this page and in [descriptor.md](descriptor.md) decodes with the real types.
4. Write timestamps in RFC 3339 and entity ids as `<instance id>:<key>`.

The free functions `download`, `translate-query` and `refresh` take the instance as a parameter, because no resource is constructed for them. The `docs` functions take none.

Source: `crates/knobas-wasm-host/src/lib.rs`, `crates/knobas-adapter-sdk/src/glue.rs`, `crates/knobas-wasm-fixture/`.
