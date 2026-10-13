- **The IPC surface, issue #326 (2026-09-04): `ConnectionReport` carries the adapter's connection
  note.** `knobas_app::sources::ConnectionReport` gains `detail: Option<String>`, mirrored in
  `app/src/lib/ipc/sources.ts` as `detail: string | null`: `knobas_source::ConnectionInfo::detail`
  copied through by `crud::test` on a successful test, `None` on a failed one. The field's doc
  comment used to argue *against* carrying it; it now says what the field is — the **connection
  note** of `CONTEXT.md`, the one line an adapter says about the far end that nothing else on the
  report already says, shown wherever a *Test connection* result is shown and never stored.

  **Ratified by Björn in the 2026-09-04 grilling of #326**, with every decision below his; this
  entry records them so the surface stays single-writer. **Björn keeps the gate for frozen
  contracts and this entry is flagged for his review**, as every entry above is.

  **Why nothing else on the wire could carry it.** The note reached the screen by exactly one path:
  `crud::set_secret` — the re-enter-a-credential command — wrote it into `source_config.auth_detail`,
  the sources view drew it under *Credential health*, and the next good sync cleared it
  (`SyncOutcome::Ok` writes a `None` detail). So it was visible from a credential re-entry until the
  next good run, and **never at add time**, which is when a reader would act on Jira's *found but not
  configured* clause (#297). `account`, `server_version` and `discovered` each say a different thing;
  the note is by definition what none of them say. A field on the report is the only place the
  moment of *Test connection* has.

  **What did not change.** No new command, no new event, no argument change; the `commands/` +
  `ipc/` module layout is untouched and neither append-only barrel grows a line. **No migration**:
  the note is never stored — `auth_detail` keeps its column and its meaning narrows to what the
  last check said went wrong. `knobas_source::ConnectionInfo` and `crates/knobas-source/src/**` are
  untouched; only what adapters put in the frozen field changed: Gitea and Confluence return `None`
  (`Gitea {version}` was `server_version`, `Confluence Data Center -- {name} ({key})` was `account`),
  and Jira's note drops the `{deployment} {version} ·` prefix and carries the Epic Link clause alone,
  in three arms — `Epic Link <id>`; `Epic Link <id> found but not configured: epic membership is not
  mirrored`; `no Epic Link field: a classic project's epic membership is not mirrored`.

  **Credential health stops carrying the note.** On a successful re-entry check, `set_secret` records
  `Ok` with **no** detail; on failure it keeps recording the error text. The glossary's *Credential
  health* was sharpened to match in `ec05ac4`, ahead of this change: what the last check said went
  wrong, not what an adapter had to say when it went right.

  **Two surfaces, one rule.** The Add-source dialog renders the note as its own line beneath
  `Connected as … · version · ms`, nothing when absent; every source row gains a *Test* action that
  calls `test_source` with a draft naming the saved source and no typed secret (the backend tests the
  stored row and the stored secret, `crud::test`'s existing path), and shows the same two lines
  **transiently** — until the row's next action or the next re-list — persisting nothing and patching
  no health; `code == "unauthorized"` offers *Re-enter* on the strength of the result alone. Both draw
  the line from `app/src/lib/sources/connection.ts`, so there is one spelling of it. The row's draft
  still carries an `auth_kind` (`SourceDraft.auth_kind` is not nullable on the wire; the row sends its
  own or `Pat`), which `crud::test` ignores for a saved source in favour of the stored row's -- a value
  on the wire that means nothing there is the one cost of not widening `SourceDraft`, and widening it
  would be an entry of its own here.

  Pinned by: `tests/sources_mirror.rs`'s `the_connection_report_shape_matches_its_typescript_mirror`
  (the field exercised as `Some`, since `null` satisfies any declared type);
  `tests/sources_crud.rs`'s `testing_a_draft_writes_nothing_at_all` (copy-through),
  `a_failed_test_discovers_nothing` (`None` on failure),
  `re_entering_a_secret_overwrites_it_tests_it_and_releases_the_backoff` (a good re-entry stores no
  note) and `a_credential_that_is_still_wrong_does_not_release_the_backoff` (a bad one keeps the error
  text); `knobas-source-jira`'s `source::tests::the_connection_note_is_the_epic_link_clause_in_three_arms`
  and `tests/field_discovery.rs`; `knobas-source-gitea`'s `tests/client.rs` and `knobas-source-confluence`'s
  `source::tests::a_successful_test_reports_the_account_and_no_note` (both `None`);
  `AddSource.test.svelte.ts` and `SourcesView.test.svelte.ts` for the two surfaces; and, against the
  seeded real Jira, `tests/atlassian_live.rs`'s
  `test_source_carries_the_epic_link_note_for_a_draft_and_for_a_saved_source` (containment of
  `Epic Link customfield_`, never the id — it differs per instance run).

