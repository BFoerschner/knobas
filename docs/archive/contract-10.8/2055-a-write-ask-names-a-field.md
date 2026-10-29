- **`crates/knobas-source/src/**` and the IPC schema — issue #2055 (2026-10-25): a
  `WriteAsk` can name one field of its write's payload.** Spec #1192, story 10. No migration,
  no event, no command, no `Source` method, no `WriteOp`, no WIT change; no new file under
  `commands/` or `app/src/lib/ipc/`.

  - **SPI.** `WriteAsk` and `RefusedWrite` (`knobas_source::grants`, #2040) each gain
    `field: Option<String>`, `#[serde(default, skip_serializing_if = "Option::is_none")]`: one
    field of the op's payload, by the name the payload carries it under (`parameters` and
    `first_in_queue` on a `run_build`). `None` asks about the write itself, as before, and an
    ask or answer without the key reads as `None`, so every existing caller and answer is
    unchanged on the wire. An ask with a field asks whether the write may carry that field set,
    and is answered by what the field needs beyond the write. An adapter that tells no field
    apart answers it as it answers the write, which is still true. `RefusedWrite::of` copies the
    ask's field and `RefusedWrite::refuses` compares it, so the app's filter
    (`sources::crud::refused_writes`, answers only for asks given) drops an answer about a field
    nobody asked about. Two constructors, `WriteAsk::new` and `WriteAsk::with_field`.
  - **TeamCity** (`crates/knobas-source-teamcity/src/grants.rs`) answers `run_build` carrying
    `parameters` from `customize_build_parameters` (held as `run_build` is: global, `_Root`, the
    project or an ancestor) and carrying `first_in_queue` from `reorder_build_queue` (held where
    any assignment names it, since TeamCity documents it as a permission no project carries). Same reads as #2040, no new
    request. Its configuration view's *Run…* drops `with_settings` and says why in its sentence
    for a user who may not override.
  - **IPC.** The generated `WriteAsk` and `RefusedWrite` in `app/src/lib/ipc/generated/entity.ts`
    gain `field?: string` (`ipc_mirror`). `refusalOf` in `app/src/lib/shell/refused-writes.ts`
    takes an optional field. The `?fake-ipc` handler still answers nothing refused.

  Witnessed by TeamCity's `grants::a_runner_may_run_and_may_not_override_parameters_or_jump_the_queue`,
  `grants::overrides_follow_the_project_and_the_queue_top_is_not_refused_on_a_guess` and
  `grants::the_views_run_offers_overrides_only_to_a_user_who_may_send_them` (`knobas-source-teamcity`'s
  `it`), `refused_writes::a_field_is_answered_as_its_own_ask` (`knobas-app`'s `it`), and
  `live_teamcity_seeded.rs`'s `a_runner_may_run_and_may_not_override_or_jump_the_queue` against the
  real server.
