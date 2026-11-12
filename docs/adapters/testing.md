# Testing

knobas checks an adapter at two moments: install runs the **static checks**, and a source's first sync runs the **data checks**. A refused install tells the reader why. A failed data check does not stop the sync; by [ADR-0028](../adr/0028-an-external-adapter-is-a-sandboxed-wasm-component-installed-from-a-release.md) the shell then draws no surface of that role for the source, and its health line names the path that missed. Neither tells you about a wrong cursor, a swallowed sink error or a misclassified fault. Those are the **contract battery**'s, which knobas holds every built-in adapter to and which you run yourself. Run all of them before every release, the same way knobas would.

There is no `knobas adapter check` command. The checks are library functions in two knobas crates, and a host-side crate of your own calls them over your built component. The XWiki adapter's [`check/`](https://github.com/BFoerschner/knobas-adapter-xwiki/tree/main/check) is about 350 lines; copy it and change `instance()`.

## The layers

| Layer | Runs on | What it catches | Where |
|---|---|---|---|
| unit tests | the host, `cargo test` | your parsing, mapping, markup and cursor logic, over recorded responses | your crate |
| static checks | the built `.wasm` | what install refuses | `knobas_wasm_host::manifest::check` |
| role data check | records from a real sync | a role path that misses on real records | `knobas_source::role::data::DataCheck` |
| docs contract | the component and its real sync | a projection and edits that disagree | `knobas_wasm_host::docs_contract::check` |
| contract battery | the component against a real instance | the SPI's rules, clause by clause | `knobas_source::contract::battery` |
| your write tests | the component against a test instance | the writes the battery never performs | yours |

## Unit tests on the host

Make the crate an `rlib` and link the component with `cargo rustc --crate-type cdylib` for `wasm32-wasip2` only. A `cdylib` cannot link on the host, where the WIT imports do not exist; an `rlib` test binary links, because tests do not reach the imports. Keep the HTTP module's `send` behind `#[cfg(target_family = "wasm")]` with a host stub that fails, and test everything else.

Record real responses once and test against them: the XWiki adapter's `adapter/testdata/` holds five pages as XWiki rendered them, and the projection's tests assert the blocks they draw. A recording certifies your parsing, not the product ([ADR-0013](../adr/0013-the-real-instance-is-the-witness-a-mock-certifies-nothing.md)); the live layers below are what do.

## The check crate

```toml
[dependencies]
knobas-wasm-host = { git = "https://github.com/BFoerschner/knobas", tag = "v3.2.9-ux7" }
knobas-source = { git = "https://github.com/BFoerschner/knobas", tag = "v3.2.9-ux7" }
serde_json = "1"
tokio = { version = "1", features = ["macros", "rt-multi-thread", "time"] }
```

Pin the same tag as your SDK. The host is wasmtime and the first build takes a few minutes.

### Static: what install checks

```rust
let manifest = Manifest::parse(&std::fs::read_to_string(path)?)?;
let artifact = std::fs::read(dir.join(&manifest.artifact.file))?;
let checked = knobas_wasm_host::manifest::check(manifest, artifact)?;
```

In order: the manifest is well formed, its interface version is hosted, the artifact's hash is the manifest's, the artifact is a knobas adapter built against that version, its descriptor is well formed and names the manifest's kind and version, each host template names `base_url` or a `config_schema` key, every role claim is covered, and a wiki claim comes with the docs projection. `checked.adapter` is the component loaded with the manifest's hosts and limits, exactly as the app loads it. Install refuses one thing this does not check: a kind a built-in or an installed adapter already has.

### Live: the rest

```rust
let source = checked.adapter.build(instance(Fault::None))?;
source.test_connection().await?;
let mut sink = VecSink(Vec::new());
source.sync(None, &mut sink).await?;

let mut data = knobas_source::role::data::DataCheck::new(&source.descriptor());
for item in &sink.0 { data.observe(&item.kind, &item.payload); }
for verdict in data.finish() { assert!(verdict.miss.is_none(), "{verdict:?}"); }

knobas_wasm_host::docs_contract::check(&checked.adapter, &source.descriptor(), &sink.0)?;

knobas_source::contract::battery(|fault| checked.adapter.build(instance(fault)).unwrap()).await;
```

`battery` panics on the first clause that fails, and its message names the rule. It needs three instances of your source: healthy, `Fault::Unauthorized` and `Fault::Unreachable`. Make the faults real:

- **Unreachable**: a base URL nothing listens on, `http://127.0.0.1:9`.
- **Unauthorized**: a credential the instance refuses. Prefer a user that does not exist to a wrong password for a real one: products lock accounts after failed logins. XWiki does after three in five minutes, and a locked account is served as the guest with the right password too.

The battery is safe against a live instance: it never performs a write you declare. It does run every read: two syncs, `view` of every item, a refresh of every item. It needs an instance that does not change between its first sync and its second. The XWiki adapter's first battery run failed clause 2 because a background job edited pages outside the synced spaces and its cursor moved for them; the fix was in the adapter, not the test ([sync.md](sync.md#cursors)).

### The battery's clauses

| Clause | Holds |
|---|---|
| 1 | a full sync hands over items, every id namespaced and parseable, every kind declared, the source id a usable instance id |
| 2 | an idle incremental sync hands nothing and returns the same cursor |
| 3, 4 | `test_connection` succeeds; an auth failure is `Unauthorized` and a connectivity failure `Unreachable`, in `test_connection` and in `sync` |
| 5 | `Capability::Write` agrees with `write_ops`; an undeclared op is refused |
| 6 | every declared path lands on your records with its type, or on nothing; none names a key your records lack |
| 7 | a sink failure fails the sync |
| 8 | without `transition`, `reachable_transitions` refuses by name |
| 9 | `view` answers every item without failing |
| 10 | every role claim is covered |
| 11 | source actions are declared exactly when `source_action` is |
| 12 | URL forms read as the paste resolver needs |
| 13 | `Capability::Refresh` agrees with what `refresh` answers, kind by kind, and a refresh hands back what the sync did |
| 14 | code page templates are well formed |

## Writes

Write a test that performs each declared write against a test instance and reads the result back through `refresh` and your projection. The XWiki adapter's `check writes` makes a page from `body_from_text`, edits section 1 through `edit_section` and `update_page`, sends the same edit again and expects `Moved`, posts a comment, and asserts the edited text and the comment on a fresh read. It leaves one page behind per run, under a parent you choose.

## A test instance

Run the real product, seeded, somewhere your tests reach. The XWiki adapter's [`testenv/`](https://github.com/BFoerschner/knobas-adapter-xwiki/tree/main/testenv) runs XWiki 17.10 LTS with PostgreSQL in Docker, installs the Standard flavor without its setup wizard, makes the wiki private like a company's, and seeds the Tidewater fixture from knobas' `fixtures/tidewater/work.json` plus a showcase page for each construct the reader maps. Its seed is idempotent and has a `--verify` mode. `testenv/samples/` keeps real responses next to the requests that made them, which is where most of the adapter's design came from.

Give the checks a way to watch one run: the XWiki check's `sync` mode prints what a sync from a given cursor hands over, which is how a deletion in the wiki was seen coming back as a tombstone.
