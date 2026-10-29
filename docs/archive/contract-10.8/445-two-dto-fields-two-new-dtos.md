- **Two DTO fields and two new DTOs on the `assets` pair, issue #445 (2026-09-07):** attaching a
  monitor to an asset — the import's report of what found nothing, and the pane's *monitoring*
  section. Ratified in advance by the spec (#427) Björn approved, whose Assets-IPC paragraph
  licenses the `assets` module pair on both sides of the bridge and whose import sentence and story
  33 name exactly these two reads, and left open by the #439 entry above, which built the resolution
  rule with nothing to resolve against. **No migration** — `0020`'s `monitors` column is the one
  this needs and it exists; `0021` is still the next free number. **No new command, no argument
  change to any existing command, no event, no settings key, no `Kind`, no reserved namespace**, no
  line added to `generate_handler!`, and neither barrel is rewritten.

  **The two fields and the two DTOs.**

  ```rust
  pub struct AssetDetail { /* … */ pub monitoring: Vec<AttachedMonitor> }
  pub struct ImportPreview { /* … */ pub unresolved: Vec<UnresolvedMonitor> }

  pub struct AttachedMonitor {
      pub entity_id: String,
      pub name: String,
      pub state: Option<String>,    // up | down | pending | maintenance
      pub web_url: Option<String>,  // its page in Kuma
      pub tombstoned: bool,         // it has left the mirror: paused, or deleted
  }

  pub struct UnresolvedMonitor { pub asset_id: String, pub asset_name: String, pub monitor_name: String }
  ```

  Mirrored in `app/src/lib/ipc/assets.ts` as `AssetDetail.monitoring`, `ImportPreview.unresolved`,
  `interface AttachedMonitor` and `interface UnresolvedMonitor`, and pinned by
  `commands::assets::tests::the_asset_detail_matches_its_typescript_mirror` and
  `the_import_preview_matches_its_typescript_mirror`, both against populated fixtures — `state` and
  `web_url` are `Option`, so an empty one would let the mirror declare them anything at all.

  **`unresolved` is a field and not a line in `changes`, because it is not a change.** `changes`
  lists what an apply would *write*, so a monitor name already on the asset is absent from it by
  construction: the second preview of an unchanged file has an empty `changes`, an empty
  `monitor_links`, and every unresolved name still unresolved. Issue #445's criterion is that such a
  name is "kept **and reported in the preview**", and before this there was no preview that reported
  it. `monitor_plan` (was `monitor_links`) now answers both halves in one walk of one map, and its
  early return on an empty `mirrored` is gone — that branch is taken *exactly* when no Kuma is
  configured, which is when the report has the most to say.

  **`monitoring` is read out of `AssetDetail::links` and not by a second walk of `knobas.link`**, so
  the section and the *Linked* panel under it cannot disagree about what is attached. Narrowed by
  two conditions and not one — `relation = 'monitored-by'` **and** the other end's kind is `monitor`
  — because a `related` link to a monitor is not monitoring and a `monitored-by` link to a ticket is
  not a monitor. The state and the page come from `sync.live_item` (`MONITOR_READINGS`), so a
  monitor whose source the reader disabled contributes no reading; the *attachment* survives both
  that and the tombstone, which is what makes a **paused** monitor — #442 tombstones a monitor that
  leaves `/metrics` — read as *still attached, no reading, no page* rather than vanishing. A section
  that dropped it would tell an asset somebody deliberately silenced a check on that nothing watches
  it. The field is `tombstoned` and not `withdrawn`, which is `CONTEXT.md`'s distinction: an item
  deleted-at-source is tombstoned, and a *withdrawn* thing is a queued write pulled back. The
  *Linked* panel under this section renders the same fact — `LinkEnd.deleted_at` — with the older
  spelling, and that prose is left as it is.

  **One stored relation word, offered from both ends.** `knobas_app::assets::MONITORED_BY` stays the
  only word the import writes and every estate read filters on. What *Link to…* gains is a second
  **offered** word: `relations.ts` grows `{ id: "monitors", …, inverseOf: "monitored-by" }`, and
  `drawn(relation, openId, targetId)` stores `inverseOf` with the ends swapped — so `monitors`
  picked on a monitor's own detail and `monitored-by` picked on an asset produce one identical row,
  drawn from the asset. Without it a reader standing on the monitor would draw the row backwards and
  the panel would say *this monitor is monitored by that container*. The alternative — a second
  stored word — was rejected: every estate read (`MONITORED_ASSETS`, `MONITOR_LINKS`,
  `attached_monitors`, the import) would have to know both, forever. `inverseOf` is a **frontend**
  concept, where the relation vocabulary already lives, and it is pinned to the backend constant by
  `commands::assets::tests::the_relation_a_source_rooms_tile_reads_is_in_the_frontend_vocabulary`,
  which now asserts both `id: "monitored-by"` and `inverseOf: "monitored-by"`.

  Pinned by, on the seam: `tests/assets_ipc.rs`'
  `an_estate_with_no_monitors_anywhere_reports_every_name_as_unresolved` (the no-Kuma state, all
  seven names), `a_name_the_mirror_lacks_is_reported_by_every_preview_and_not_only_the_first` (six
  reported with `gitea` as the positive control, and still reported after an apply),
  `the_pane_lists_the_monitors_watching_an_asset_with_their_state_and_a_link_to_kuma` (with three
  negatives: a `related` link to a monitor, a `monitored-by` link to a ticket, and a monitor on
  another asset) and `a_paused_monitor_stays_in_the_section_with_no_state_and_no_link`; and, at the
  join no seam can answer alone, `tests/adapter_to_mirror.rs`'
  `after_kuma_syncs_the_estate_files_monitor_names_become_links` — the real adapter over real HTTP
  against the pinned recording, into the mirror, with the real estate file imported over it and all
  seven names resolving. On the frontend: `relations.test.ts` (four tests on `drawn` and the two
  readings, as literal words), `LinkDialog.test.svelte.ts` (both ends producing one row),
  `AssetsView.test.svelte.ts` (the section's rows, *Open in Kuma* handing over the mirror's own URL,
  only the still-waiting names listed underneath, and neither section on an asset nothing watches)
  and `AssetsView.import.test.svelte.ts` (the dialog's new group).

  **Certified live**: `just kuma-live`, 5 passed, and the estate file's seven names checked against
  the real Kuma's `/metrics` roster — every one of them held.

  Ratified by the orchestrator as spec #427 and issue #445, whose acceptance criteria specify the
  re-import drawing a link per named monitor with the unheld name kept and reported, *Link to…* from
  either end with the inverse label, and the pane's list of attached monitors with state and a deep
  link. **Björn keeps the gate for frozen contracts and this entry is flagged for his review**, and
  in particular the `inverseOf` addition to the relation vocabulary.

