- **IPC schema**, issue #39 (2026-08-28): `SearchFilters` gained `authors: Vec<String>`, with the
  matching field on the `app/src/lib/ipc` TS mirror. **Not a new grant** — the per-stream rulings
  above already record "**E-Q1** `SearchFilters.authors` granted"; what was missing was the §2.4
  listing and this entry. The field is additive and `#[serde(default)]`, so an older frontend
  keeps deserialising. Ratified by the orchestrator on the review of PR #79 (2026-08-28); the
  field sits last in the struct, after `mine`, matching `knobas-search/src/types.rs`.
  - `authors` is the people the query **named** (`@jonas`, `author:jonas`, or an author chip).
    What `mine` resolves to stays separate inside `EffectiveFilters` (`named_authors` vs
    `identity_authors`) and is unioned only when the SQL is bound — so `is_empty()` still counts a
    named person, and the response `echo()` never lists a username the user did not type.

