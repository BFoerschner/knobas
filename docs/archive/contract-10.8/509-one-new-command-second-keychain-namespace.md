- **One new command and a second keychain namespace — issue #509 (2026-09-08): the hcloud
  importer.**

  An **importer** (`CONTEXT.md`; ADR-0015 — an importer produces an estate file and is **not** a
  source) reads every server one Hetzner Cloud token can see and answers with an estate file *in
  the checked-in shape*; #508's Import commands consume it. Spec #491 names the touch in advance
  — its stream map row 9 is *"a new IPC command; the `importer:` keychain namespace"* and its
  Implementation Decisions are *"App-side producers in the assets module … each returns an estate
  file's text in the checked-in shape, and the existing preview and apply commands consume it"* —
  and #509's fifth acceptance criterion asks for these entries by name (*"§10.8 entries for the
  produce command and the namespace"*). **Björn keeps the gate for frozen contracts and this
  entry is flagged for his review.** In his absence the v1.5 loop's deputy is what exercises that
  gate, and its ruling is recorded on the issue and appended to
  `docs/archive/2026-09-v1-5-unattended-rulings.md` by the PR that acts on it; **no ruling had
  been posted on #509 when this entry was written**, so this sentence records the flag and claims
  no ratification — **ratified in his absence by the deputy's ruling of 2026-09-08 on #509**
  ([comment](https://github.com/BFoerschner/knobas/issues/509#issuecomment-5579197449),
  `docs/archive/2026-09-v1-5-unattended-rulings.md`), which exercised that gate on the command,
  on the `importer` flag and on the keychain namespace, **and its `Landing` argument by the second
  ruling of 2026-09-08**
  ([comment](https://github.com/BFoerschner/knobas/issues/509#issuecomment-5579926452), same file,
  `## #509 (second ruling)`), which is the one part of this entry the first ruling ratified in a
  shape the code no longer has. The sentences stand side by side rather than the first being
  rewritten, the treatment §10.8 gives every sentence it supersedes.

  **The IPC schema — one new command, two new DTOs, no change to any existing one:**

  ```rust
  #[tauri::command] pub async fn produce_estate_file<R>(app, lifecycle,
      producer: String, token: Option<String>, land_under: Option<assets::Landing>)
      -> Result<assets::hcloud::Produced, IpcError>;
  ```

  mirrored in `app/src/lib/ipc/assets.ts` as
  `produceEstateFile(producer, token, landUnder: Landing | null)`.

  * **On `commands::assets`, and the `commands/` + `ipc/` module layout is unchanged.** The estate
    is what this produces and the two Import commands that consume it already live there — the
    treatment `reachable_transitions` got on `commands::entity` (#498) and the three smart-list
    writes got on `commands::search` (#506). An importer is emphatically **not** a source, so
    `commands::sources` was never a candidate.
  * **`ImportProducer` gains `importer: boolean`**, mirroring `Producer::importer`. Stated and not
    derived: a frontend reading `id !== "estate_file"` would be measuring a representation of the
    thing, which is the mistake `Producer::importer`'s own doc refuses one surface over, and
    `the_chooser_offers_producers_this_build_knows` now holds the flag as well as the id.
  * **`Produced`** is the outward wire shape: a tag-`state` union of `token_needed`,
    `landing_needed { servers }` and `ready { file, new_servers }`, mirrored as
    `TokenNeeded | LandingNeeded | ProducedFile`. A union rather than a record of optionals
    because two of the three fields are meaningless in each state and `{ file: null, needs: null }`
    is a state the backend is never in; the tag is `PropertyValue`'s `kind` arrangement one
    surface over. **No `#[serde(default)]` is owed on any of it** — the sentence every
    field-on-a-DTO entry since #39 has had to answer. `Produced` derives `Serialize` only. It is
    constructed in `assets::hcloud`, serialized outwards, and decoded nowhere: it rides in no
    archive, no settings row, no file and no `pg_dump`, so there is no older shape for an absent
    field to describe. The day one is decoded is the commit that owes the attribute and an entry.
  * **`Landing` is the inward one**, added by the second ruling of 2026-09-08 and the only shape
    this entry declares that the backend *decodes*: `struct Landing { parent: Option<String> }`,
    `deny_unknown_fields`, `Deserialize` only, mirrored as `Landing { parent: string | null }`.
    **`#[serde(default)]` is owed here and is present, on `parent`** — the sentence every
    field-on-a-DTO entry since #39 has had to answer, and the answer is the opposite of
    `Produced`'s: `{}` reads as the top, because an absent key and an explicit `null` are one
    answer *inside* the record. The distinction the type exists for is one level out — between the
    **argument** being absent and this field being `null` — and those two do cross the bridge as
    different things. It is decoded from the bridge and from nowhere else: no archive, no settings
    row, no file, no `pg_dump`.
  * **`token` and `land_under` are `Option`, and that is not #508's argument in reverse.** #508's
    `producer` is required because a missing producer would have to be read as *some* producer and
    the only safe reading is the one that copies every asset. Here absence has an exact meaning
    and it is the common case: an absent `token` means *read the keychain*, which is what makes
    *asked once* true, and an absent `land_under` means *nothing has been said yet*, which is the
    question `landing_needed` exists to ask. Both callers on the bridge are the Import dialog.
    **A *present* `land_under` has a domain of its own, and that is the second ruling of
    2026-09-08.** It was `Option<String>` when the first ruling read this paragraph, which gave
    *the top of the estate* and *nothing has been said yet* one spelling between them: the dialog
    offered the top, promised it in words, sent a bare `null`, and the run asked the same question
    again — on an estate with no assets, the only button there was did nothing, for ever. So a
    present value is now a `Landing`, whose `parent` is `None` for the top, the spelling
    `create_asset` and `move_asset` already take and the one `FileAsset::parent` has read as a
    top-level asset since #439. What the first ruling settled about **absence** is untouched and
    still exact; what it did not see is that the present value carried two answers in one.
    `Option<Option<String>>` was the shape not taken — it would rest on the mirror sending
    `undefined` for one and `null` for the other, an invisible distinction on the wire — and a
    reserved string for the top is a reserved namespace, which the *what is not touched* list below
    rules out by name.
  * **No event.** The Tree re-reads after the apply, by the session that made it — the Import's
    own arrangement since #439, unchanged.
  * **One barrel grows one line.** `crates/knobas-app/src/lib.rs`'s handler list gains
    `commands::assets::produce_estate_file` **at its foot** (the list is append-only and never
    re-sorted), which `tests/wiring.rs`' `every_command_is_in_the_handler_list` requires.
    `app/src/lib/ipc/index.ts` lists **modules and events**, not functions, and `./assets` is
    already re-exported — so it is unchanged, and that is the file being append-only rather than
    an omission.

  **The keychain grows a second namespace** (interfaces §3's convention, which §10.8's list does
  not name but #509's criterion does, exactly as #452's envelope version was recorded here).
  `knobas_secrets::KeychainAccount` is the account an item is stored under, with two constructors
  and no others: `source:<source_id>` and `importer:<producer_id>`. The **envelope is untouched
  at version 2** — an importer's token is an ordinary `api_token` secret with no account beside
  it — and no migration is claimed: nothing about a credential is in Postgres (§14), which is the
  whole point.

  **A newtype, and the alternative is what argues for it.** The namespace used to be applied
  inside `KeyringStore::entry`, so `SecretStore`'s argument was a *source id* in one reading and
  an *account* in another. Adding a second namespace to that arrangement leaves every direct
  caller free to pass either, and the failure is silent in the worst direction: a test asserting a
  credential is **absent** would pass against a key nothing was ever written under, and
  `tests/sources_crud.rs` alone holds eight such assertions. With the newtype the only two ways to
  make an account are the two constructors, so the compiler asked all forty-odd call sites which
  they meant. `SecretStore`'s three methods, `spawn`'s three wrappers, `MemoryStore` and
  `KeyringStore` take `&KeychainAccount`; nothing else in that crate moved.

  **What is not touched.** **No migration** — `0026` is still the next free number and this entry
  claims none. No command renamed or removed, **no return type changed**, no argument added to an
  existing command (#508's `producer` is the current shape and is recorded above), no new event,
  no settings key, no `Kind`, no reserved namespace, no `WriteOp` (ADR-0006 is untouched: an
  importer is not a source and queues nothing), no `Capability` — `Capability::Import` stays
  undeclared per ADR-0015, and the download is an `<a download>` over a blob the webview already
  holds rather than a filesystem grant. `crates/knobas-source/src/**` is absent from the diff: no
  `Source` implementation, no descriptor, no registry row, no battery clause — an importer
  implements none of it, which is ADR-0015 made structural rather than promised.
  `crates/knobas-http/**` is **unchanged**; `knobas-app` gains it as a *dependency*, which is the
  opposite of a change to it — the one HTTP stack, with its rate limiter, retry budget and fault
  mapping, rather than a `reqwest::Client` of this crate's own (the hole `Request`-with-no-`send`
  exists to close, and the argument #452's entry already made for `Request::text`).
  `crates/knobas-app/src/{error,profile}.rs` are untouched. The estate file's **key vocabulary is
  unchanged**: `EstateFile`, `FileAsset` and `FileRoute` keep their fields and their
  `deny_unknown_fields`, and `knobas-core`'s `tests/estate_file.rs` keeps `ASSET_KEYS` and
  `ROUTE_KEYS` as they were — a producer's files are *"an estate file's text in the checked-in
  shape"*, which is why the produced file says nothing about hcloud. The share export's part list
  is unchanged, and its secret-free assertion grew the importer's half rather than a part.
  `testenv/hetzner/estate.json` gains two properties on each of its three servers (`location`, and
  the `knobas` label) — an ordinary custom property in the existing bag, no schema and no number.

  **The reading this entry has to be explicit about: a token is stored only after a run that
  reached the far end.** `assets::remember` -- producer-generic, beside `Producer` rather than
  inside `hcloud`, because nothing in it names a live system -- takes the run's `Result` as an
  *argument*
  rather than being called after it has been checked, so that sentence is a property of one
  function its own tests hold it to rather than a property of the order two lines are written in;
  and only a token the caller typed is written, because one read out of the keychain is already
  there and rewriting it is a keychain prompt for nothing. A credential the far end refused is never written, so *asked
  once* does not become *asked once and then wrong forever*; the dialog puts the field back on an
  `unauthorized` refusal, which is the only way back from a token that stopped being accepted, and
  an importer has no sources view to send anybody to. For the same reason the refusal carries **no
  `source_id`**: that field is what the shell hangs *re-enter this source's credential* off, and
  there is no source here.

  **What the gate can and cannot witness, said here because a green suite would otherwise imply
  the wrong thing.** `tests/assets_ipc.rs` runs the producer against a **recording** of what
  `GET /v1/servers` really answered on 2026-09-08, served by wiremock: that certifies the decode,
  the file, the origin-key match, the landing and the refusals, and it can never go red when
  Hetzner changes its JSON. `just estate-live` is the only witness that the real API still answers
  in that shape **and** that the three `hcloud_id` values #508 wrote into
  `testenv/hetzner/estate.json` are the ids of the three real servers — a wrong one is unmatched
  by key, so its server previews as *new* and the run goes red (ADR-0013; the deputy's ruling of
  2026-09-08 on #508, part 3, which named this recipe as that gap's discharge).

  Pinned by: `commands::assets`' `every_produced_state_matches_its_typescript_mirror` (three arms,
  their tags, and the union read out of the mirror rather than listed),
  `the_mirror_invokes_the_commands_by_their_registered_names`,
  `the_mirror_sends_the_argument_names_tauri_expects` (which gains
  `("produce_estate_file", "producer")`, `("produce_estate_file", "token")` and
  `("produce_estate_file", "landUnder")` — the camelCase one is the argument Tauri renames and
  the one a mistyped mirror would drop silently) and
  `the_chooser_offers_producers_this_build_knows` (now over two entries, and over each entry's
  `importer` flag as well as its id); `tests/wiring.rs`'
  `every_command_is_in_the_handler_list` and `assets_ipc.rs`'
  `every_asset_command_is_registered_and_its_arguments_decode`, which now drives **all three
  spellings of the landing** through the real Tauri pipeline — the argument absent, `{parent:null}`
  and `{parent:"asset:…"}` — because two of the three decoding to one value is the defect the
  second ruling was raised on; `knobas_secrets`'
  `an_importers_token_lives_under_its_own_namespace` (the prefix, and that no source id can be
  spelled so as to reach an importer's account) beside
  `the_account_is_the_source_id_prefixed`; `assets`'
  `a_token_is_owed_only_when_the_keychain_holds_none`,
  `a_typed_token_is_kept_only_when_the_run_answered` (all three ways a run can end, and the
  importer's account written where a source's is not), `a_stored_token_is_not_written_back`,
  `a_tokens_debug_prints_no_credential` (`Token` hand-writes its `Debug` for
  `knobas_secrets::Secret`'s reason, and has that type's own test) and
  `the_estate_files_producer_is_refused_by_name_and_hcloud_is_not`; `backup_ipc.rs`'
  `a_shared_source_carries_no_secret_and_lands_as_missing_secret`, which now scans the archive for
  the importer's token, the `importer:` namespace and the `api_token` envelope kind; five in
  `assets::hcloud` over the file builder — the origin key held to `PRODUCERS`' declaration, the id
  written as text, a label keeping its own key and one shadowing a written key refused by name, a
  parent written only for the entries the import would create, and a server with no IPv4 or image
  carrying neither property; **six** in `crates/knobas-app/tests/assets_ipc.rs`, **four** of them
  over a scratch database seeded with the real estate file —
  `every_recorded_server_is_already_in_the_tree_and_nothing_would_change` (the recorded half of
  the live recipe, and the assertion that holds the producer's property spellings to
  `estate.json`'s), `a_server_the_estate_does_not_hold_is_asked_about_and_lands_where_it_is_told`
  (both round trips, the parent written only on the new entry, and the two matched servers not
  re-parented), `a_field_this_build_has_never_heard_of_does_not_refuse_the_run` and
  `a_refused_token_is_unauthorized_and_not_an_internal_fault` (and carrying no `source_id`); a
  fifth over a scratch database holding **nothing at all**,
  `on_an_empty_estate_the_top_of_the_estate_is_a_landing_place` — the first run the second ruling
  of 2026-09-08 was raised on, where every server is new, there is no asset to land under, and the
  only answer a reader can give is the top: `LandingNeeded` when nothing has been said and `Ready`
  when `Landing { parent: None }` has, no entry in the file carrying a `parent`, and all three
  applied at the top level — those five over `assets::hcloud::client`, the **one** constructor the
  command, the recipe and this suite share, so the live run exercises the client the command uses
  rather than a second one spelled the same way (part 4c of the first ruling); the sixth,
  `the_estate_file_producer_is_not_an_importer_and_hcloud_is`, is a plain `#[test]` that asks
  `PRODUCERS` and needs neither a database nor a client, because that is where the answer lives;
  `just estate-live`'s
  `the_real_estate_is_already_in_the_tree`; and, on the rendered side, five in
  `AssetsView.import.test.svelte.ts` — the token asked for once and not before it is owed, *land
  under* asked only when the importer found a server the estate lacks (both halves, the token
  field gone once the run reached the far end, and **the top of the estate pressed from where the
  picker opens**, which sends `{parent: null}` and reaches a file rather than the same question
  again), the produced file offered for download, and the
  chooser dropping the preview the last producer drew (a plan under one origin key is not a plan
  under another), and a token the far end refuses putting the field back where an unreachable
  Hetzner does not — story 62's second half, and the only way back from a credential that stopped
  being accepted, since an importer has no sources view (ADR-0015).

  *Amended 2026-10-12 (#634, the deputy's ruling of that day on #634):* the Hetzner servers were
  decommissioned and left `testenv/estate.json` (moved there from `testenv/hetzner/`), so the
  pinned live test `the_real_estate_is_already_in_the_tree` is rewritten as
  `the_real_project_round_trips_through_the_importer` — produce, apply, produce again over whatever
  the real project holds, all known with no changes, at a server count `ESTATE_LIVE_HCLOUD_SERVERS`
  declares. The witness paragraph above is history; ADR-0015's amended consequence and
  `docs/archive/2026-10-unattended-rulings.md` (`## #634`) are the current statement. No command,
  type or wire shape in this entry changes.

