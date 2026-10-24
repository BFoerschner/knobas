- **IPC schema, issue #1959 (2026-10-24): `UnmonitoredAsset` gains
  `monitor_on_its_way: Option<QueuedWrite>`, and `submit_write` refuses a second create of a
  monitor whose create is on its way.** No new command, event, migration, type or module in
  `commands/` or `ipc/`; the generated mirror (`app/src/lib/ipc/generated/assets.ts`) is
  regenerated for the field, which imports `QueuedWrite` from `./sources`. Reserved for Björn,
  ruled by the implementer on Opus (`## #1959` in `docs/decisions/2026-10-unattended-rulings.md`);
  the issue has no parent spec.

  1. **`UnmonitoredAsset.monitor_on_its_way`**, answered by `unmonitored_assets` for every asset
     and route row: the newest `create_monitor` row on its way whose payload name
     (`payload->'CreateMonitor'->>'name'`) the row records in `knobas.asset.monitors` or
     `knobas.route.monitors`, as `knobas_core::write_queue::get` reads it; `null` where none is.
     **On its way** (`assets::ON_ITS_WAY`) is `pending`, or `sent` within
     `sources::write_queue::ARRIVAL_PATIENCE` (7 days, now `pub(crate)`) with neither the
     receipt's monitor (`<source_id>:<remote_id>`, in `sync.item`, tombstoned rows included) nor a
     live monitor of the create's name in its source in the mirror. Held, refused and discarded
     rows are not on their way. Two rows recording one name both carry the create, since the
     poll attaches the one monitor to both (`knobas_sync::attach`). `UnmonitoredAsset` no longer
     derives `PartialEq`/`Eq` (`QueuedWrite` does not); nothing compared it.
  2. **`submit_write` (and every `submittable` path) refuses a `CreateMonitor`** whose name has a
     create on its way to the same source (`assets::monitor_on_its_way`), with `conflict` and the
     message `a monitor named "<name>" is already on its way to <display name> (write <id>); it
     attaches once the next poll mirrors it`, before a row exists. The same name to another source,
     or once the first create is held, refused or withdrawn, is queued as before.
  3. **No receipt change.** Kuma's create already answers the minted `monitorID` as the receipt's
     `remote_id` (#453), which item 1 reads as the monitor's entity key, so no adapter changes.

  Answered by the fake IPC (`fake/monitors.ts`): each fixture create of a monitor is remembered
  and named on the rows recording its name, and a recorded name only takes an asset off the list
  when the seeded Kuma holds it; `edit_route` records `monitors` on the fixture route. Called by
  the Monitors tab's *Assets nothing watches* (`TfMonitors.svelte`), which draws *Monitor queued*
  for a row carrying a create, follows a pending one (`followWrite`, whose `Following` gains
  `onleave`), and reads the list again at the end of each sync while a row is on its way.
  Witnessed by `tests/it/monitor_on_its_way_ipc.rs` (new, four tests) and
  `sources::write_queue::tests::a_second_create_of_a_monitor_on_its_way_is_refused_before_it_is_queued`.
