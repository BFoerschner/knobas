- **`crates/knobas-source/src/**` (the contract battery's own tests only) — issue #1201
  (2026-10-19): two clause-10 tests witness the custom-role rule in the battery.** No change to
  the `Source` trait, any DTO, any role's minimum or any battery clause; no migration, no IPC
  command or event, no `commands/` or `ipc/` module, no barrel line, nothing in
  `crates/knobas-http/**`. Parent spec #1192 (ADR-0029; glossary **Role**).

  `contract::tests` gains two `Behavior`s of its test adapter (kind `test`):
  `ClaimsItsCustomRole` claims `test:thing` with no reads and passes
  (`accepts_a_custom_role_namespaced_to_the_adapter`); `ClaimsAnotherAdaptersCustomRole` claims
  `other-adapter:thing` and is refused by name
  (`rejects_a_custom_role_namespaced_to_another_adapter`). The rule they witness is #1193's entry
  (`<adapter_kind>:<name>` claims) and the summary they would reach is #1196's
  (`SourceSummary.roles`); this entry records the touch, not a new surface.

  **Not frozen, ruled by the merge-manager of PR #1242:** the new shell route `#/item/<entity id>`
  (`app/src/lib/tf/item/`, `tf/route.ts`, `itemHash`'s default case). A hash route is the
  frontend's own address, not the IPC command and event schema: it adds no command, event or DTO
  field and no line to `app/src/lib/ipc/index.ts`, and the page reads only `get_entity` and
  `entity_view`, which exist. Its convention is recorded in
  `docs/specs/2026-10-15-ticket-first-shell-conventions.md`.
