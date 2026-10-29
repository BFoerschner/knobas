- **The IPC mirror's types are generated from Rust: `ts-rs`, the `generated/` folder, and §2.6
  amended — issue #1325 (2026-10-19).** Parent spec #1306, its first switching ticket. No
  migration, no event, no `WriteOp`, nothing under `crates/knobas-source/src/**`, no command
  added, removed or renamed, no argument changed, no field added or renamed, no wire shape
  changed, nothing added to either append-only barrel: `app/src/lib/ipc/index.ts` changes only its
  header comment, which repeated §2.6's old rationale. Ratified by the implementer on Opus under
  Björn's 2026-10-18 reserved-steps rule; the rulings are in
  `docs/archive/2026-10-unattended-rulings.md` under `## #1325`.

  **§2.6 amended.** "Mirrors stay hand-written (M0 rationale: `tauri-specta` is still an RC)"
  no longer holds. That reason was about one generator, and it still holds for it (`specta`
  is `2.0.0-rc.25` on 2026-10-19), but `ts-rs` is a stable generator that meets the spec's three
  requirements (below). Hand-copying has a cost: 277 types written twice, 82 fixture tests and a
  shared assertion module to catch the drift, and four edits per renamed field. From this entry
  on, **the types** of a barrel are generated from their Rust types once that barrel is switched,
  and **the `invoke` functions stay hand-written**. Argument names, the Rust-side barrel pins
  (`every_command_in_this_module_has_a_typescript_mirror` and its siblings) and the fake census
  are unchanged. §2.6's mapping table stands as written; it is now what the generator is
  configured and checked against. The other six barrels keep their hand-written types until
  their own tickets switch them.

  **The generator: `ts-rs = "=12.0.1"`** (workspace dependency, features `chrono-impl` and
  `uuid-impl`; the default `serde-compat` stays on), configured in code
  (`Config::new().with_large_int("number")`), not through `TS_RS_*` variables. Evidence from a
  probe crate with the derives in use in `crates/` (`grep -rhoE 'serde\([^)]*\)' crates`: 386
  `default`, 75 `rename_all`, 23 `tag`, 21 `rename`, 20 `skip_serializing_if`, 9 `flatten`, 2
  `transparent`), checked on the generated text:

  | requirement | Rust | generated TypeScript | verdict |
  | --- | --- | --- | --- |
  | doc comments → JSDoc | `///` on a type, a field, a field inside a variant, a flattened field | `/** … */` above each, paragraphs and lists kept | met |
  | | `///` on an **enum variant** | dropped | gap: a variant's meaning goes in the type's doc, its payload's rules on the fields (convention below) |
  | tagged unions | `#[serde(tag = "state", rename_all = "snake_case")]` | `{ "state": "starting", detail: string \| null } \| …` | met |
  | externally tagged | default enum with struct and newtype variants | `{ "Comment": { body: string } } \| { "Plain": string }` | met |
  | `rename_all` | `snake_case` on an enum, `camelCase` on a struct | `"ticket_view" \| "pull_request"`; `someField` | met |
  | `rename` | `#[serde(rename = "type")]` | `type: Kind` | met |
  | `flatten` | `#[serde(flatten)] inner: Inner` | `Inner`'s fields inlined, with their docs | met |
  | `default` | `#[serde(default)] v: Vec<String>` | `v: Array<string>` (always serialised, so required) | met |
  | `skip_serializing_if` | `#[serde(default, skip_serializing_if = "Option::is_none")] m: Option<String>` | `m?: string \| null`, the spelling `sources.ts` already writes by hand | met |
  | `transparent` | `#[serde(transparent)] struct W(String)` | `type W = string` | met |
  | timestamps → string | `DateTime<Utc>`, `NaiveDate` | `string` | met (`chrono-impl`) |
  | `Option<T>` → `T \| null` | `Option<i64>` | `number \| null` | met |
  | `i64`/`u64` → `number` | `i64`, `u64`, `HashMap<String, i64>` | `bigint` by default; `number` with `with_large_int("number")` | met by configuration |
  | `serde_json::Value` → `unknown` | `serde_json::Value` | no impl without `serde-json-impl`, whose `JsonValue` is not `unknown` | met by annotation: `#[ts(type = "unknown")]` on the field |

  Not chosen: `specta` 2 / `tauri-specta` (still a release candidate, the reason §2.6 gave);
  `typeshare` (an external CLI that parses source text, so not a cargo test, and a second tool
  to install and pin); `tsify` (built for `wasm-bindgen` exports, not an IPC mirror).

  **Module layout booked.** One new folder, `app/src/lib/ipc/generated/`, holding one file per
  barrel, each named after its barrel. This entry adds `generated/app.ts` (`AppStatus`,
  `DbState`). It also books the six that follow, one per ticket, each with its fixture tests
  deleted in the same PR and no further §10.8 entry needed for the file itself:

  | ticket | file | barrel switched |
  | --- | --- | --- |
  | #1326 | `generated/entity.ts` | `entity.ts` |
  | #1327 | `generated/sources.ts` | `sources.ts` |
  | #1328 | `generated/assets.ts` | `assets.ts` |
  | #1329 | `generated/search.ts` | `search.ts` |
  | #1330 | `generated/time.ts` | `time.ts` |
  | #1331 | `generated/backup.ts` | `backup.ts` |

  No other file goes in `generated/`: `the_generated_ipc_mirror_is_written` fails on a
  generated file with no barrel of the same name. The folder is not a module of the IPC surface:
  nothing in it calls `invoke`, the house rule and the fake census read the direct children of
  `app/src/lib/ipc/` only, and `index.ts` re-exports the barrels, not the folder. One new Rust
  file outside `commands/`, `crates/knobas-app/src/ipc_mirror.rs` (`#[cfg(test)]`), holds the
  writer test and the per-barrel list of root types.

  **The app barrel.** `AppStatus` and `DbState` derive `ts_rs::TS` with
  `#[ts(export_to = "app.ts")]`. `app/src/lib/ipc/app.ts` re-exports both names from
  `./generated/app` and keeps its five `invoke` functions. Before the switch, the two doc
  rules the TypeScript had and the Rust lacked were moved into the Rust: which payloads the
  union check guards (`detail` and `message`), and the keychain *suffix*. The two Rust-side
  fixture pins, `every_db_state_has_its_typescript_counterpart` and
  `app_status_serialises_the_keys_the_mirror_declares`, are deleted. The wire-shape test
  `db_state_serialises_as_the_contract_declares` and the barrel pin stay.
  `DbState`'s TypeScript changes from `state: "starting"` to `"state": "starting"` and
  `AppStatus` from an `interface` to a `type`; svelte-check, the tests and every caller treat them
  the same.

  **The check.** `cargo test -p knobas-app --lib ipc_mirror` writes the files, touching only
  those whose text changed, by rename. `just ipc-mirror` runs that test from the workspace's
  test binary and fails when `app/src/lib/ipc/generated/` differs from HEAD, untracked files
  included. It is a step of `just check`'s cargo half, between `inventory` and `test`.
  `just pre-gate` runs the writer, so a Rust change arrives with its TypeScript among the files
  to commit. `scripts/gate-scope.sh` treats `app/src/lib/ipc/generated/*` as Rust output: a head
  touching it takes the full gate, and a frontend-only head still takes `check-front`.

  **Conventions the six switches follow.** A type joins its barrel's file with
  `#[ts(export_to = "<barrel>.ts")]`, and its barrel's root types are listed in
  `export_roots` (a type a listed one reaches is exported with it). A type that reaches a type
  without `export_to` fails the writer test, so a missing annotation cannot become a new file.
  A `serde_json::Value` field carries `#[ts(type = "unknown")]`. Because ts-rs drops a
  variant's doc, a rule about a variant's payload goes on that payload's field (field docs inside
  a variant are carried), and what each variant means goes into the enum's own doc comment, one
  bullet per variant, with no copy left on the variants; `DbState` is the example. A type two barrels use is exported to one barrel's file, and ts-rs writes the
  `import type` into the other's.
