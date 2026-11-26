# Quickstart

From an empty directory to an adapter installed in knobas: a memo board that lists memos from `GET <base URL>/memos.json`. It claims a custom role, so each memo opens a view the adapter draws. The code below builds, passes install's checks and passes the contract battery against a local memo server, as written. For a real adapter with writes and a built-in role, read [the XWiki adapter](https://github.com/BFoerschner/knobas-adapter-xwiki) beside this page.

## 1. Toolchain

A component is built for `wasm32-wasip2`. Pin the toolchain knobas builds with, so the SDK compiles:

```toml
# rust-toolchain.toml
[toolchain]
channel = "1.99"
components = ["rustfmt", "clippy"]
targets = ["wasm32-wasip2"]
```

`rustup show` installs it.

## 2. The crate

```toml
# Cargo.toml
[package]
name = "knobas-adapter-memos"
version = "0.1.0"
edition = "2024"

# An rlib, so `cargo test` links on the host. The component is the same
# crate linked as a cdylib for wasm32-wasip2 (step 5). A cdylib cannot be
# linked on the host, where the imports the WIT bindings name do not exist.
[lib]
crate-type = ["rlib"]

[dependencies]
# The SDK by git, at a knobas release tag. The tag decides the interface
# version you build against (the SDK's own version: 1.5.0 here). Keep the
# name: the export macro refers to `knobas_adapter_sdk`.
knobas-adapter-sdk = { git = "https://github.com/BFoerschner/knobas", tag = "v3.2.9-ux7" }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
chrono = { version = "0.4", default-features = false, features = ["serde", "std"] }

# The guest half of wasi:http 0.2, the component's one way out.
[target.'cfg(target_family = "wasm")'.dependencies]
wasip2 = "=1.0.4"

[profile.release]
opt-level = "s"
lto = true
strip = true
```

The SDK is not on crates.io: its bindings read the WIT from the knobas repository, which cargo checks out whole for a git dependency.

## 3. HTTP

The SDK has no HTTP client. Copy [`adapter/src/http.rs`](https://github.com/BFoerschner/knobas-adapter-xwiki/blob/main/adapter/src/http.rs) from the XWiki adapter into `src/http.rs`. It sends one request through `wasi:http`, reads the whole answer, maps a status onto knobas' error classes, and on the host target fails every request, so the pure code still tests there. [http.md](http.md) says what the host does to each request.

## 4. The adapter

```rust
// src/lib.rs
mod http;

use knobas_adapter_sdk::spi::role::RoleClaim;
use knobas_adapter_sdk::spi::view::{Block, SourceView, ViewSection};
use knobas_adapter_sdk::spi::{AuthMethod, EntityRef, KindInfo};
use knobas_adapter_sdk::{
    Adapter, ConnectionInfo, Cursor, Instance, Sink, SourceDescriptor, SourceError, SyncItem,
    WriteOp, WriteReceipt,
};

use std::collections::BTreeMap;

const KIND: &str = "memos";

/// FNV-1a: a digest that is the same in every build, unlike `std`'s hasher.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// One memo, as `GET {base_url}/memos.json` lists it.
#[derive(serde::Deserialize)]
struct Memo {
    id: String,
    title: String,
    text: String,
    author: Option<String>,
    /// RFC 3339.
    updated: Option<String>,
}

pub struct Memos {
    id: String,
    name: String,
    base_url: String,
}

impl Memos {
    fn memos(&self) -> Result<Vec<serde_json::Value>, SourceError> {
        let url = format!("{}/memos.json", self.base_url.trim_end_matches('/'));
        let response = http::check(http::send(http::Method::Get, &url, None)?, "the memo list")?;
        response.json("the memo list")
    }

    fn item(&self, raw: serde_json::Value) -> Result<SyncItem, SourceError> {
        let memo: Memo = serde_json::from_value(raw.clone())
            .map_err(|e| SourceError::protocol(format!("a memo does not read: {e}")))?;
        Ok(SyncItem {
            entity: EntityRef::new(&self.id, &memo.id),
            kind: "memo".into(),
            title: memo.title,
            body_text: memo.text,
            author: memo.author,
            updated_at: memo
                .updated
                .as_deref()
                .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
                .map(|t| t.to_utc()),
            payload: raw,
            web_url: None,
            deleted: false,
        })
    }
}

impl Adapter for Memos {
    fn new(instance: Instance) -> Self {
        Self {
            id: instance.id,
            name: instance.display_name,
            base_url: instance.base_url,
        }
    }

    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: self.id.clone(),
            adapter_kind: KIND.into(),
            name: if self.name == KIND { "Memos".into() } else { self.name.clone() },
            capabilities: Vec::new(),
            adapter_version: env!("CARGO_PKG_VERSION").into(),
            auth_methods: vec![AuthMethod::ApiToken],
            accepts_account: false,
            min_sync_interval_secs: 60,
            write_ops: Vec::new(),
            entity_kinds: vec![KindInfo {
                id: "memo".into(),
                label: "Memo".into(),
                plural: "Memos".into(),
                monogram: "ME".into(),
                full_sync_exhaustive: true,
                refreshes: false,
            }],
            config_schema: serde_json::json!({
                "type": "object", "properties": {}, "additionalProperties": false
            }),
            payload_paths: Vec::new(),
            roles: vec![RoleClaim { role: "memos:board".into(), reads: Vec::new() }],
            source_actions: Vec::new(),
            clone_from: Vec::new(),
            url_forms: Vec::new(),
            code_pages: None,
        }
    }

    fn test_connection(&self) -> Result<ConnectionInfo, SourceError> {
        let count = self.memos()?.len();
        Ok(ConnectionInfo {
            detail: Some(format!("{count} memos")),
            ..ConnectionInfo::default()
        })
    }

    fn sync(&self, cursor: Option<Cursor>, sink: &mut Sink) -> Result<Cursor, SourceError> {
        // The board has no "changed since" and reports no deletion, and
        // knobas runs a full sync only for a source's first run. So the
        // cursor carries a digest of every memo: a run hands over the memos
        // whose digest moved and tombstones the ones that are gone.
        let seen: BTreeMap<String, u64> = cursor
            .as_deref()
            .and_then(|c| serde_json::from_str(c).ok())
            .unwrap_or_default();
        let mut now = BTreeMap::new();
        let memos = self.memos()?;
        let total = memos.len() as u64;
        for (done, raw) in memos.into_iter().enumerate() {
            sink.walked(done as u64, Some(total), "memos");
            let item = self.item(raw)?;
            let digest = fnv1a(item.payload.to_string().as_bytes());
            let key = item.entity.key.clone();
            if seen.get(&key) != Some(&digest) {
                sink.item(item)?;
            }
            now.insert(key, digest);
        }
        for key in seen.keys().filter(|k| !now.contains_key(*k)) {
            sink.item(SyncItem {
                entity: EntityRef::new(&self.id, key),
                kind: "memo".into(),
                title: key.clone(),
                body_text: String::new(),
                author: None,
                updated_at: None,
                payload: serde_json::Value::Null,
                web_url: None,
                deleted: true,
            })?;
        }
        sink.walked(total, Some(total), "memos");
        // The same memos give the same text, so an idle run hands back the
        // cursor it was given.
        Ok(serde_json::to_string(&now).expect("a map of digests serializes"))
    }

    fn write(&self, op: WriteOp) -> Result<WriteReceipt, SourceError> {
        Err(SourceError::protocol(format!(
            "the memos adapter performs no {}",
            op.identifier()
        )))
    }

    fn reachable_transitions(&self, _entity: &str) -> Result<Vec<String>, SourceError> {
        Err(SourceError::protocol(format!(
            "the memos adapter has no workflow: source {:?} offers no transitions",
            self.id
        )))
    }

    fn view(&self, entity: &str, _section: Option<&str>) -> Result<Option<SourceView>, SourceError> {
        let key = entity.strip_prefix(&format!("{}:", self.id));
        for raw in self.memos()? {
            let memo: Memo = match serde_json::from_value(raw) {
                Ok(memo) => memo,
                Err(_) => continue,
            };
            if Some(memo.id.as_str()) == key {
                return Ok(Some(SourceView::new(vec![ViewSection {
                    id: "memo".into(),
                    title: "Memo".into(),
                    count: None,
                    deferred: false,
                    blocks: vec![Block::Text { text: memo.text, markdown: false }],
                }])));
            }
        }
        Ok(None)
    }
}

knobas_adapter_sdk::export!(Memos);
```

What each part is held to:

- `descriptor` must not depend on the instance beyond `id` and `name`. knobas reads it at load with an instance whose id and display name are the adapter kind, config `{}` and no network, and builds the Add-source form from that answer. That is why `name` turns `memos` into `Memos`. [reference/descriptor.md](reference/descriptor.md).
- `adapter_kind` is a constant. Echoing `instance.kind` would name the adapter after the probe instance knobas loads it with, and install refuses it for disagreeing with the manifest.
- `sync` hands every changed item to the sink and returns the sink's error with `?`. An idle run returns the cursor it was given, because the same memos make the same cursor text. A memo that disappeared is handed over as deleted: knobas sweeps deleted items only after a source's first run, so an adapter that leaves deletions to knobas never has them removed. [sync.md](sync.md).
- `write` refuses every op, because `write_ops` is empty; knobas refuses an undeclared op before it reaches you anyway. `reachable_transitions` refuses by name, which is what an adapter without a workflow answers.
- The optional reads (`refresh`, the build reads, `project` and the docs writes, `translate_query`, `download`) have defaults on the `Adapter` trait that answer "not offered".

## 5. Build the component and its manifest

```sh
cargo rustc --target wasm32-wasip2 --release --crate-type cdylib
mkdir -p dist
cp target/wasm32-wasip2/release/knobas_adapter_memos.wasm dist/
shasum -a 256 dist/knobas_adapter_memos.wasm
```

```json
{
  "adapter_kind": "memos",
  "adapter_version": "0.1.0",
  "interface_version": "1.5.0",
  "artifact": { "file": "knobas_adapter_memos.wasm", "sha256": "<the 64 hex digits>" },
  "hosts": ["{base_url}"]
}
```

Save it as `dist/knobas-adapter.json`. `hosts: ["{base_url}"]` lets the component reach the address the reader gives the source and nothing else. The XWiki adapter's [`scripts/build.sh`](https://github.com/BFoerschner/knobas-adapter-xwiki/blob/main/scripts/build.sh) does this step, hash included. [release.md](release.md) has every manifest field.

## 6. Run install's checks before knobas does

Copy the XWiki adapter's [`check/`](https://github.com/BFoerschner/knobas-adapter-xwiki/tree/main/check) crate into a workspace beside yours and run

```sh
cargo run -p check -- static dist/knobas-adapter.json
```

It loads the component with knobas' own WASM host and runs the checks install runs. Its `live` mode needs an instance of your source and an `instance()` function written for it. [testing.md](testing.md).

## 7. Install it

In knobas, open *Add source* and paste the absolute path of `dist/knobas-adapter.json` under *Install an adapter*. A local file is for development: it has no tag, so its hash alone pins it. *Install…* runs the checks again and shows what the adapter may reach. Confirm, then add a source of kind *Memos* with a base URL and a token.

To publish, put the two files on a GitHub release and paste the repository URL instead ([release.md](release.md)).
