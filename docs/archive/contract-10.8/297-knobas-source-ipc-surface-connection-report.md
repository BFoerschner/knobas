- **`crates/knobas-source/src/**` and the IPC surface, issue #297 (2026-09-03): a connection report
  can carry configuration the adapter *discovered about its own instance*, and the Add-source
  dialog fills an empty field from it.** The occasion is Jira's Epic Link custom field, whose id is
  minted per instance: #276 measured `customfield_10101`, `customfield_10109` and
  `customfield_10101` on three seeds of one script, and on one of them `customfield_10102` was
  *Epic Status*. A classic Data Center project keeps epic membership in that field and **nowhere
  else** — `fields.parent` is absent from every issue in the seeded corpus — so as shipped, a Jira
  source mirrored no epic membership at all unless the user had typed a per-instance id nothing
  told them, and a copied id read the wrong field silently rather than failing.

  **The SPI field.** `ConnectionInfo::discovered: BTreeMap<String, String>`, `#[serde(default)]` so
  a report from a peer built before this decodes as an adapter that discovered nothing. Keyed by
  the `config_schema` property each value belongs in, which is the whole reason it is a **map and
  not a second named field**: the facts are per-adapter, and the next adapter with a per-instance
  id of its own adds a key rather than another field on this frozen struct. A key naming a property
  the adapter does not declare fills nothing. Nothing in it is a secret — it crosses to the form and
  into `source_config.config`, which is Postgres (§14) — and the SPI doc says so where an adapter
  author will read it. Every other `ConnectionInfo` field is untouched, and so is every fault class,
  every mapping downstream and the contract battery, which gains no clause: an adapter that
  discovers nothing is a correct adapter.

  **`#[serde(default)]` covers decoding, not construction.** A Rust struct literal must still name
  every field, so growing this struct costs one line at every construction site — eight in this
  repo today: the five adapters (Jira, Confluence, Gitea, TeamCity and the mock) and three test
  fakes. That is the price of the frozen struct being a plain `struct` and it is charged once per
  site, not per call; the two sites that build it as `..ConnectionInfo::default()` are unaffected,
  and so is any stored or in-flight report, which decodes as an adapter that discovered nothing.

  **The IPC touch.** No new command, no new event, no new module on either side, and no DTO the
  frontend acts on changes meaning: `ConnectionReport` (`knobas-app`'s, the §2.2 shape that already
  deviates from the interfaces doc by carrying a `String` + `IpcErrorCode` instead of a
  `SourceError`) gains the same map, carried through verbatim from `ConnectionInfo`, and
  `app/src/lib/ipc/sources.ts` gains `discovered: Record<string, string>` on its mirror. Neither
  barrel is touched. `crates/knobas-app/tests/sources_mirror.rs` pins the key set and the wire
  shape — a map, not a list of pairs. **A failed test reports an empty map**, in `crud::test`: a
  test that did not connect learned nothing, and a stale map would fill a form with another
  server's ids.

  **The adapter's endpoint set grows one read**, which is a §4.2 row and not this list:
  `GET /rest/api/2/field`, sent by `test_connection` and by nothing else — never per sync run,
  never per issue. The field is picked by the Greenhopper **plugin key**
  (`com.pyxis.greenhopper.jira:gh-epic-link`), the one property of it an administrator cannot
  rename, with an exact-name fallback for an instance that answers no `schema`; the sibling
  `gh-epic-status` sits one id along and is what an id-shaped guess picks. **A 404 (and a 501) on
  that path reads as "this instance has no field table"** — a Jira Core, a proxy exposing only the
  M1 paths — and does not fail the connection, because losing the account, the version and the
  credential's verdict over an optional convenience would make a working source unsaveable. Every
  other status still propagates: a 401 there is a refused credential, and reporting it as a healthy
  source missing one convenience is the failure that arm exists to prevent.

  **`knobas-mockd` gains nothing** (it is deprecated and frozen, ADR-0013). It serves no
  `api/2/field` handler while the WADL declares the path, so its own fallback records one
  `Unimplemented` violation per probe; `knobas-source-jira/tests/mockd.rs` asserts *that* violation
  and no other rather than dropping the check, so an invented path, verb or query parameter still
  fails there. The path itself is pinned against the WADL tables directly
  (`api::tests::the_field_table_is_a_path_the_wadl_declares`), the success direction against a
  canned-bytes socket (`tests/field_discovery.rs`), and the whole round trip — discovered id in,
  PAY-219's membership of PAY-200 out — against the real seeded product in
  `tests/live_jira_seeded.rs`.

  **The dialog fills, the backend does not.** `AddSource.svelte` puts a discovered value into the
  matching form field **only when it is empty and only when it is a text control**, keyed on the
  property name — the same rule and the same reasoning as #82's `username` fill, generalised, so
  the dialog still holds no table of what an adapter's config keys mean. *Test connection* writes
  nothing, which is the property that command is built around and which this does not relax.

  **What the issue asked for, and what was widened.** Issue #297's acceptance criteria specify the
  endpoint (and say in as many words that *that* is a §4.2 row and "not a §10.8 surface"), the
  dialog fill, the help text and the live test; on the report they say only "reported on the
  `ConnectionReport` the way the account name is", which read literally is a **named** field.
  The map is the wider shape, directed by the orchestrator when the issue was dispatched so that
  the next adapter with a per-instance id of its own needs no second touch of this frozen crate.
  The endpoint needed no entry; the map does, and this is it.

  **Ratified by the orchestrator, 2026-09-03**, as the §10.8 exception for #297: the generic
  `ConnectionInfo::discovered` map keyed by `config_schema` property, in place of a named
  `epic_link_field`, on the reasoning above. **Björn keeps the gate for frozen contracts and this
  entry is flagged for his review**, as #284's is.

