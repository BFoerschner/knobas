- **No migration, a twenty-first `assets` command and one new DTO, issue #449 (2026-09-07):** the
  Monitors tab's open-alert cards and its *Not monitored* roster. **Ratified in advance by the spec
  (#427) Björn approved** — story 68's other half, *"open-alert cards at the top (monitor, asset
  path, since when, acked or not, with Ack and Open asset) and a **Not monitored** roster of assets
  that have no `monitored-by` link"* — and by the #428 entry above, whose module-pair paragraph says
  that module is where **every** asset command lives, "including the route, import and alert
  commands #432, #439 and M4.1 add". Written with the implementing PR, per #428's, #431's, #434's,
  #435's, #439's, #443's, #444's, #448's and #446's pattern.

  **`0023` is still the next free number.** Both halves are reads over tables that already exist:
  the cards are `0022`'s `knobas.monitor_alert` through the `open_alerts` command #444 already
  landed, and the roster is a `not exists` over `knobas.confirmed_link` (`0007`) and
  `knobas.asset` (`0017`). Nothing here writes anything the ack (#446) did not already write.

  **The cards add nothing to the wire at all.** `open_alerts` answers every field a card draws —
  the monitor, its name, the state, `opened_at`, `acked_at` and the assets it watches — and
  `ack_alert(monitorId)` is the write, unchanged since #446. That is the point of the shape #446
  chose: its own entry says "#449's cards carry `OpenAlert::monitor_id` and are served by the same
  shape", and they are, with no argument, no field and no command added for them.

  **The command and the DTO, which are the roster's.**

  ```rust
  #[tauri::command] pub async fn unmonitored_assets(..) -> Result<Vec<assets::UnmonitoredAsset>, IpcError>;

  pub struct UnmonitoredAsset {
      pub id: String, pub type_id: String, pub type_label: String,
      pub monogram: String, pub name: String, pub path: Option<String>,
  }
  ```

  Mirrored in `app/src/lib/ipc/assets.ts` as `unmonitoredAssets()` and the interface. **One line
  appended** at the foot of `crates/knobas-app/src/lib.rs`'s `generate_handler!` list, under #446's;
  neither barrel is rewritten (`app/src/lib/ipc/index.ts` already re-exports `./assets` wholesale)
  and the `commands/` + `ipc/` module **layout is untouched** — no new module pair, which is the
  thing §10.8 freezes about that directory.

  **A read of its own and not a field on `monitor_roster`.** The roster is a list of *monitors* and
  this is a list of *assets*; they have no row in common, and one shape carrying both would be an
  answer whose halves are read for two different surfaces and whose failure modes are separate — a
  tab whose monitors loaded and whose gaps did not still tells the reader most of what they came
  for, and the two `$effect` reads are deliberately not awaited together for that reason.

  **No type argument, although the tab filters by type.** `monitor_roster`'s chips argument,
  applied to the other list: the filter's options are the types the *answer* holds, so the frontend
  cannot draw them without the whole list, and a filtered read would be a second answer that could
  disagree with the counts beside it. `app/src/lib/assets/monitors.ts`' `typeCounts` and `byType`
  both key on `type_id`, so an option reading `VM 2` over a list of one is not a state the tab can
  reach. The one place the type filter deliberately differs from the state chips is that it draws
  **only the types present**, where the chips draw all six including zeroes: six fixed states where
  a vanishing chip would move its neighbour under the pointer, against up to nineteen types over an
  estate somebody built by hand, where an option with nothing behind it can only empty the list.
  *Superseded in part by the #493 amendment in §9 above (2026-09-11): up to sixteen types since v1.5
  dropped `scenario`, `step` and `connector`; the sentence stands as history.*

  **`not exists` and not a left join**, because the question is about a set: an asset watched by
  three monitors is one row of the answer either way, and a join would need a `distinct` over a
  shape the read then has to re-sort. The inner statement is `ROSTER_ASSETS`' join read backwards
  with its guards intact — **undirected** (`0011` made the pair unordered), `m.id <> a.id`, and
  `m.kind = 'monitor'` so a `monitored-by` link to a ticket is not an attachment. `m.id <> a.id` is
  belt-and-braces here rather than load-bearing, unlike in `ROSTER_ASSETS`, which joins
  `knobas.asset` at both ends: this read binds the other end to `knobas.entity`, so the kind guard
  already refuses an asset's own row and dropping `m.id <> a.id` fails no test — verified by
  mutation while merging #482, and written down here rather than left to be rediscovered. `knobas.confirmed_link` is what leaves an unconfirmed **proposal**
  (#478's `monitor_url_host`) on the roster, which is the answer the reader wants: a guess nobody
  has agreed to is exactly the gap this list points at.

  **"Not monitored" is a statement about attachment, not about attention — the one judgement in
  this ticket, made deliberately.** An asset whose only monitor Kuma has **paused** is *off* the
  roster. Somebody wired a check to it and then silenced it, which is a different fact from nobody
  ever having wired one, and it is already said in as many words by the *Paused* chip on the roster
  above, with the monitor's own history under it — the reading `CONTEXT.md`'s **Live item** entry
  records for #448's exemption. A list that grew a row the moment somebody paused a check would
  send a reader to do work that has already been done. Recorded in `CONTEXT.md`'s **Monitors**
  entry as a 2026-09-07 amendment, and pinned as the fifth case of
  `only_a_confirmed_monitored_by_link_to_a_monitor_takes_an_asset_off_the_roster`.

  **A disabled source's monitor is still an attachment, and the consequence makes an asset quiet in
  both lists.** `monitor_roster` drops a monitor whose source the reader turned off (`0012`'s
  `coalesce(s.enabled, true)`, #448's entry above), so an asset watched only by such a monitor has
  no row on the roster and no row on this one. Surfaced by this PR's spec review, and decided the
  same way as the paused case for a reason of its own: a source-aware read would put the **whole
  estate** on the *Not monitored* roster the moment somebody turned their one Uptime Kuma source
  off, telling a reader to attach monitors they had already attached, while the roster above says
  "nothing is mirrored yet" and the Sources view says why. Pinned by
  `a_monitor_from_a_source_the_reader_turned_off_still_counts_as_attached`, which asserts both
  halves — the asset off this roster and the roster above empty — so the quiet is a decision on the
  record rather than a gap somebody rediscovers.

  **Every type is in the read**, although a *site* or a *network* is not usually something anybody
  checks: which types are worth monitoring is a judgement about a particular estate, and a read that
  made it would be a read hiding gaps it had decided did not count. The type filter is where that
  judgement belongs, and it is why the filter exists.

  **`alerts.svelte.ts` grows an `ack` and its ports become `Partial`** — not a frozen surface (the
  store is frontend state, no wire type and no command), recorded because it changes what three
  surfaces share. The cards read the **same module singleton** the top strip's badge and the Assets
  view's strip read, so an ack pressed on a card moves all three at once, and a monitor that
  recovers takes its card away through the store's existing `sync:state` re-read rather than through
  anything the tab decides. The write-then-re-read-whatever-happened shape is `createInbox`'s `act`
  and its reason: a `not_found` means the alert closed while the card was on screen, so the stale
  list is what caused it. `createAlerts(ports?: Partial<AlertPorts>)` is what lets the eight
  existing test call sites keep passing the two ports they passed before.

  **One card per alert, and not one per watched asset** — the one place this deliberately differs
  from #444's strip in the Tree, whose entry argues the opposite for itself. That strip is a list of
  *where to go next* and splits a monitor watching two assets into two rows for exactly that reason;
  a card is a list of *what to do next*, and the thing to do is one ack. Two cards offering the same
  ack would be a reader wondering what the second one does.

  **A fake-tauri handler for `unmonitored_assets` and one for `ack_alert`**, both derived from
  `estate.json` the way `fakeMonitorRoster` is: the gaps are the assets the file names no Uptime
  Kuma monitor for, and the ack is remembered in module state because *acked* is the one fact on
  that surface a reader makes themselves. Without the second, the card's own button was the last
  control on this tab answering "command not found" in red under `?fake-ipc`.

  Pinned by: `commands::assets::tests::the_unmonitored_asset_matches_its_typescript_mirror` and the
  registration and argument-name loops (now twenty-one commands); `tests/wiring.rs`; four tests in
  `tests/assets_ipc.rs`
  (`the_roster_lists_the_assets_nothing_watches_and_leaves_out_the_ones_it_does`,
  `only_a_confirmed_monitored_by_link_to_a_monitor_takes_an_asset_off_the_roster` with its five
  cases, `the_roster_is_ordered_by_where_an_asset_sits_and_then_by_name`,
  `a_monitor_from_a_source_the_reader_turned_off_still_counts_as_attached`) plus that file's handler
  list and its argument-decode loop; two more in `alerts.test.svelte.ts` for the ack and for a
  failed ack outliving the re-read that follows it; three tests in `monitors.test.ts` for the type
  filter's
  arithmetic; eleven in `MonitorsView.test.svelte.ts` for the two surfaces (the cards' order and
  content, the empty state, the ack's argument and the acked card that stays, an alert acked
  elsewhere, the recovery that removes a card, an alert watching nothing, a failed alert read said
  out loud with no cards to say it beside, the roster with its paths, the type filter, the
  all-watched sentence and the failed roster read); and two in `fake-tauri.test.ts`.

  **Björn keeps the gate for frozen contracts and this entry is flagged for his review**, and in
  particular the new command, the new DTO, and the tombstoned-monitor reading of *Not monitored*.

