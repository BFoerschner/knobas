- **One new command, `refresh_entity`, and the detail's `refreshes` flag — issue #2107
  (2026-10-25): *Refresh* on an item's detail.** Parent spec #2085, stories 25 to 31, ADR-0031. No
  migration, no event, no new module in `commands/` or `ipc/`, nothing under
  `crates/knobas-source/src/**` or `crates/knobas-http/**`, no change to `error.rs` or
  `profile.rs`.

  **The command**, in `crates/knobas-app/src/commands/sources.rs` beside `sync_role`, appended at
  the foot of `lib.rs`'s handler list:

  ```rust
  pub async fn refresh_entity<R: tauri::Runtime>(
      app: tauri::AppHandle<R>,
      entity_id: String,
  ) -> Result<crate::sources::refresh::EntityRefresh, IpcError>
  ```

  It runs `knobas_app::sources::refresh::refresh` (the one way the app refreshes, #2095) with no
  write behind it, and answers once the refresh has ended. A refresh that read something ends on
  `item:refreshed` with `write_id: null`, as #2095's entry says. Errors: `not_ready` while the
  engine is starting or stopping; `invalid` for an id that is not an entity's; `not_found` for a
  namespace no source is configured for, as `get_entity` answers both.

  **One new DTO**, `crate::sources::refresh::EntityRefresh`, internally tagged on `outcome`
  (snake_case): `{ outcome: "refreshed" }` (the item is mirrored as the source holds it, live again
  if it was tombstoned), `{ outcome: "gone" }` (tombstoned), `{ outcome: "not_offered" }` (the
  adapter does not read one item; nothing was written), `{ outcome: "turned_off" }` (the source is
  turned off; nothing was read), `{ outcome: "failed", reason: String }` (the source's own error,
  or the adapter's build failure; nothing changed). In the generated
  `app/src/lib/ipc/generated/sources.ts` (`ipc_mirror`).

  **One new field on an existing DTO**, `EntityDetail.refreshes: bool` (what `get_entity`
  answers), in the generated `app/src/lib/ipc/generated/entity.ts`: whether the item's adapter
  declares `Capability::Refresh`, read off the registry's template for the configured adapter kind
  (`SourceDescriptor::refreshes`), so the read builds no source and reads no keychain. `false` for
  a source with no configuration row or an adapter kind not installed. An instance whose own
  descriptor answers otherwise (the mock's `"refresh": false`) gets `not_offered` from the
  command, and the shell runs its source.

  **TypeScript:** `refreshEntity(entityId): Promise<EntityRefresh>` and the re-export of
  `EntityRefresh` in `app/src/lib/ipc/sources.ts`. The `?fake-ipc` fixture answers it from its
  corpus (`app/src/lib/shell/dev/fake/entity.ts`): `refreshed` for an item of an adapter that
  refreshes on main, `not_offered` otherwise, `turned_off` for a turned-off source, and `failed`
  under `?fake-refresh-failed`; `get_entity` answers `refreshes` by the same set. Its answer type is
  in `app/src/lib/shell/dev/fake-answers.ts`. Witnessed by `knobas-app`'s
  `tests/it/ipc.rs::refresh_entity_answers_each_outcome_and_the_detail_says_it_can` and the
  registry check in the same file, `commands::entity::detail::tests::can_refresh_is_the_adapters_declaration`,
  `fake-features.test.ts`, and the shell tests in `TfItem.test.svelte.ts` and `TfPr.test.svelte.ts`.
