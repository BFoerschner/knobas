- **A second importer on the one produce command, and one new field on its answer — issue #510
  (2026-09-08): the Docker host importer.**

  The second **importer** (`CONTEXT.md`; ADR-0015 — an importer produces an estate file and is
  **not** a source) reads the containers of every `container_engine` asset in the tree that
  carries a `docker_context`, by spawning the docker CLI under that context, and answers with an
  estate file *in the checked-in shape*; #508's Import commands consume it unchanged. Spec #491
  names the touch in advance — its stream map row 10 is *"one IPC produce command"* and its
  Implementation Decisions are *"Docker is read by spawning the docker CLI under the context named
  by the engine asset's property, which is a person-typed value and therefore within ADR-0016 …
  Docker containers take their engine as parent"* — and #510's fourth acceptance criterion asks
  for this entry by name (*"§10.8 entry for the produce command"*). **Björn keeps the gate for
  frozen contracts and this entry is flagged for his review.** In his absence the v1.5 loop's
  deputy is what exercises that gate, and its ruling is recorded on the issue and appended to
  `docs/archive/2026-09-v1-5-unattended-rulings.md` by the PR that acts on it; **no ruling had
  been posted on #510 when this entry was written**, so this sentence records the flag and claims
  no ratification — **ratified in his absence by the deputy's ruling of 2026-09-08 on #510**
  ([comment](https://github.com/BFoerschner/knobas/issues/510#issuecomment-5581090073),
  `docs/archive/2026-09-v1-5-unattended-rulings.md`), which exercised that gate on the second
  arm of `Importer`, on the third `PRODUCERS` row and on the one new field, and which ruled the
  fork named on the ticket before it started taken correctly as route 1, the field-name revert
  right and the `Produced` module move inside this ticket's scope. The sentences stand side by
  side rather than the first being rewritten, the treatment §10.8 gives every sentence it
  supersedes.

  **No new command, no new argument, no new event, no migration.** `produce_estate_file` is
  #509's, at #509's shape, and the second importer is a second arm of the enum it already
  dispatches on:

  ```rust
  #[tauri::command] pub async fn produce_estate_file<R>(app, lifecycle,
      producer: String, token: Option<String>, land_under: Option<assets::Landing>)
      -> Result<assets::Produced, IpcError>;
  ```

  * **`assets::Importer` gains `Docker`**, and the match on it in `commands::assets` still has no
    wildcard arm (ADR-0006's rule, which #509's entry recorded): adding this variant stopped that
    file compiling until somebody said what running it meant, which is the whole point of the
    enum.
  * **The return type moved from `assets::hcloud::Produced` to `assets::Produced`.** The same
    module already argues for it about `Token` and `remember` — *"beside `Producer` rather than
    inside `hcloud`, because nothing here names a live system: the second importer would otherwise
    import its credential handling from a module named after the first"* — and a type spelling
    every producer's answer is that case exactly. **The wire is unchanged by the move**: same tag
    `state`, same three arms, same TypeScript `Produced`.
  * **No field on that union is renamed, and the one that reads narrow is left alone
    deliberately.** `Ready.new_servers` holds *container* names when Docker filled it, and the
    Import dialog was drawing *"1 server is new"* over a list of containers. The **rendered
    sentence** is fixed here — it is nobody's frozen surface — and the **field name is not**:
    #509's §10.8 entry declared it a day earlier and a deputy ratified it, so renaming it is a
    §10.8 conversation of its own rather than something #510 was asked for. It *was* renamed
    (`new_assets`, and `LandingNeeded.servers` → `assets`) in this branch and **reverted before
    the PR**, on the reading comment 1 on #510 gives for the matching rule — *"that is a change to
    the matching rule, so raise it as a `**Fork:**` rather than deciding it"* — applied to a
    frozen wire field. `assets::Produced`'s own header and `app/src/lib/ipc/assets.ts` both say
    the name reads narrow and why it stands, so the next reader does not take the narrowness for a
    claim. Nothing decodes `Produced` — it derives `Serialize` only, rides in no archive, no
    settings row, no file and no `pg_dump` — which is why the added field below owes no
    `#[serde(default)]`, #509's own answer to the sentence every field-on-a-DTO entry since #39
    has had to answer.
  * **One field is new: `Ready.skipped: Vec<String>`.** The Docker importer's answer has to be
    able to say *this engine was not read*, because a produced file that quietly omitted an
    engine's containers is indistinguishable from an engine holding none, and the reader is the
    only one who can tell them apart. It is `[]` for hcloud, which skips nothing: one token either
    sees a server or does not know it exists. Serialize-only, so it owes no `default` for the same
    reason as above.
  * **`Landing` is untouched**, and Docker never sends one: a container lands under the engine
    whose context found it (spec #491, story 68), so `land_under` is ignored on that arm and
    `LandingNeeded` is unreachable from it. `TokenNeeded` is unreachable from it too — the Docker
    importer has **no credential**, so **the keychain is not touched at all on that arm**, and a
    token sent to it is refused by name (`invalid`) rather than written under an account nothing
    would read. The `importer:` namespace grows nothing; the envelope stays at version 2.
  * **`assets::PRODUCERS` grows a third row** — `docker`, *Docker host*, origin key
    `["docker_context", "container_name"]` — and `IMPORT_PRODUCERS` in `app/src/lib/ipc/assets.ts`
    grows the matching chooser entry. #509's entry recorded that a producer may be declared with
    no chooser entry and that `the_chooser_offers_producers_this_build_knows` therefore reads in
    one direction only; that stays true and this ticket closes the one instance of it.
  * **The barrel is unchanged.** `crates/knobas-app/src/lib.rs`'s handler list already carries
    `commands::assets::produce_estate_file`; no command was added, so `tests/wiring.rs`'
    `every_command_is_in_the_handler_list` sees the same list.

  **ADR-0016, made structural.** *"A spawned command takes no argument from the mirror"* names
  this importer in as many words, and the code is the sentence: the argument vector is
  `["--context", <context>, "ps", "--format", "json"]`, a constant with one hole, and what goes in
  the hole is an asset property — `knobas.asset` is knobas' own table, not a mirror, and no
  mirrored item is read anywhere in `assets::docker`. There is no shell:
  `std::process::Command` takes the vector itself, so a context spelling shell metacharacters is
  one argument called that and docker answers *no context by that name*. The **program** is the
  constant `docker` and is not a setting — ADR-0016's command templates are for *open in editor*
  and *open a terminal*, where the binary is a matter of taste. `tokio`'s `process` feature is
  **not** taken: the spawn is `std::process::Command` inside `spawn_blocking`, which is
  `knobas_secrets::spawn`'s arrangement and adds no dependency.

  **What is not touched.** **No migration** — `0026` is still the next free number and this entry
  claims none. No command added, renamed or removed; no argument added to an existing command; no
  new event; no settings key; no `Kind`; no reserved namespace; no `WriteOp` (ADR-0006 is
  untouched: an importer is not a source and queues nothing); no `Capability` —
  `Capability::Import` stays undeclared per ADR-0015, the opener's scope is untouched per
  ADR-0016, and no Tauri shell plugin is added. `crates/knobas-source/src/**` is absent from the
  diff: no `Source` implementation, no descriptor, no registry row, no battery clause.
  `crates/knobas-http/**` and `crates/knobas-secrets/**` are unchanged — this importer speaks no
  HTTP and holds no credential. `crates/knobas-app/src/{error,profile}.rs` are untouched. The
  estate file's **key vocabulary is unchanged**: `EstateFile`, `FileAsset` and `FileRoute` keep
  their fields and their `deny_unknown_fields`, and `knobas-core`'s `tests/estate_file.rs` keeps
  `ASSET_KEYS` and `ROUTE_KEYS` as they were — what a produced file carries is *properties*, which
  is the bag every estate file has always had. `testenv/hetzner/estate.json` gains
  `docker_context` on `asset:orbstack-docker` (the three Hetzner engines have carried theirs since
  M4.0) and `docker_context` + `container_name` on each of its nine containers — ordinary custom
  properties in the existing bag, no schema and no number.

  **The reading this entry has to be explicit about: a container's origin key repeats its name,
  and that is the mechanism rather than a redundancy.** `Producer::origin_key` is a list of
  **property** keys, matched every part or nothing on both sides, and an asset's name is a
  *column*. So a container's key is `docker_context` and `container_name`, the second sitting
  beside the entry's own name. The alternative — teaching the planner to reach across to a
  parent's property or down to an entry's field — is a second matching rule inside the second
  matching rule, and the deputy's ruling of 2026-09-08 on #508 (part 2) named it as the route
  *not* to take without a fork; `CONTEXT.md`'s **Origin key** already said *"the property an
  importer sets"* and is amended here to say that a part which repeats a column is still a
  property. What keeps the repeat honest is `knobas-core`'s
  `every_container_carries_the_context_and_the_name_it_is_matched_on`, which holds every
  container's `container_name` equal to its name and its `docker_context` equal to its engine's —
  the gate the three `hcloud_id`s structurally cannot have, because a container's key is implied
  by the rest of the same file and an hcloud id is implied by nothing.

  **What the gate can and cannot witness, said here because a green suite would otherwise imply
  the wrong thing.** `tests/assets_ipc.rs` runs the producer against a **stub executable** the
  test writes — a `#!/bin/sh` script that records its argument vector one argument per line and
  answers with a recorded `docker ps --format json`: that certifies the parse, the argument rule,
  the type filter, the landing, the recreate and the refusals, and it can never go red when docker
  changes its output. Those tests are `#[cfg(unix)]`, `tests/checkout_ipc.rs`' stub's arrangement
  and for its reason (`PermissionsExt`), so **on Windows this half is unwitnessed**. `just
  estate-live` is the only witness that the docker CLI still writes one JSON object per line, that
  the four `docker_context` values in `testenv/hetzner/estate.json` are contexts this machine has
  reaching the engines that file names, and that every running container on those engines is
  written down there (ADR-0013). It needs no tunnel — the contexts are `ssh://` to the servers'
  own addresses and the tunnel forwards only the products' HTTP ports.

  **Two producers, one envelope and one *what is new*.** `assets::render_estate_file` and
  `assets::new_asset_ids` are the halves both producers share, lifted out of `hcloud` by this
  ticket rather than copied into `docker`: the second is load-bearing, because *what is new* is
  the id-then-origin-key rule (#508) and a producer that answered it itself would be the second
  copy that goes stale against `matched_by_origin_key`. Both therefore decide it by **asking the
  Import**, at the cost of one extra preview per run, which writes nothing.

  Pinned by: `commands::assets`' `every_produced_state_matches_its_typescript_mirror` (three arms
  and every field of each, listed here and cross-checked against the mirror; the **tags** and the
  union's membership are read off the mirror rather than listed, which is what a fourth state
  added on the Rust side has to fail on) and `the_chooser_offers_producers_this_build_knows` (now
  over three entries and each entry's `importer` flag); `assets::docker`'s
  `the_origin_key_this_producer_writes_is_the_one_the_planner_matches_on`,
  `the_context_is_the_only_thing_substituted_and_it_is_one_argument`,
  `every_line_is_one_container_and_the_id_is_read_by_nothing`,
  `the_first_name_is_the_containers_and_an_unreadable_line_is_refused`,
  `a_container_lands_under_the_engine_whose_context_found_it` and
  `no_two_context_and_name_pairs_spell_one_id`; `tests/assets_ipc.rs`'
  `every_recorded_container_is_already_in_the_tree_and_nothing_would_change` (which also pins that
  one engine is spawned per engine and no more, the type filter's own witness),
  `a_recreated_container_keeps_its_asset` (the produced file byte-identical across a change of
  every container id — the strongest form of *the id is read by nothing*),
  `a_new_container_lands_under_its_engine_and_an_engine_with_no_context_is_named`,
  `a_column_this_build_has_never_read_does_not_refuse_the_run`,
  `a_context_this_machine_cannot_read_refuses_the_whole_run`,
  `the_context_property_reaches_docker_as_one_argument` (read off the stub's own record rather
  than off the code that built it, **and** the `touch` in that property asserted not to have
  happened),
  `one_context_is_read_once_and_only_a_container_engine_is_read_at_all`,
  `the_docker_producer_is_an_importer_and_declares_the_two_part_key` and
  `the_docker_importer_takes_no_credential_and_says_so` (the command's Docker arm, over an
  **unreachable** pool, so the refusal is shown to happen before anything queries — the
  arrangement `tests/ipc.rs`' `demo_load_is_refused_outside_the_demo_profile` uses), beside the
  `produce_estate_file` row `every_asset_command_is_registered_and_its_arguments_decode` gains for
  the producer with neither optional argument; `knobas-core`'s
  `every_container_carries_the_context_and_the_name_it_is_matched_on`; `just estate-live`'s
  `the_real_containers_are_already_in_the_tree`; and, on the rendered side,
  `AssetsView.import.test.svelte.ts`' `the Docker importer asks for no token and no landing, and
  names what it skipped`, beside the chooser test that now reads three entries.

