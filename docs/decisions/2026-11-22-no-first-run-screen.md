# No first-run screen (2026-11-22)

The ux31 pass, branch `ux31-no-first-run`. Björn: "I want you to also remove the first start screen. It's an old
artifact of earlier times of the app. For ux31". He was away; the forks below were ruled by a Fable agent (xhigh) and
are recorded here as his standing rule asks.

## What it was

`App.svelte` drew `sources/FirstRun.svelte` in place of the shell while `AppStatus.first_run` was true (no source and
no `first_run.completed` row in `knobas.setting`). Four panels (Database, Source, First sync, Done) under a breadcrumb:
*Add a source* through the same `TfAddSource` dialog Settings opens, on the demo profile *Load the Tidewater dataset*,
the first sync over `sync_now_with_progress` with a bar, and "knobas mirrored N items". Its ways out (*Set up later in
Settings*, *Restore from a backup in Settings*, *Skip for now*) called `complete_first_run`.

## What a fresh profile does now

- **It lands on home like any other start:** All tickets (`#/tickets`), with no chosen job (`2026-11-20-knobas-opens-on-all-tickets.md`).
  No branch on the source count: a conditional home would be the wizard's special case surviving.
- **All tickets with no source at all** (the health store answered empty, not still reading) draws: "No sources yet.
  Add one and its tickets, pull requests, builds and pages come into knobas." with *Add a source…* (primary, to
  `#/settings/sources/add`), on the demo profile *Load the Tidewater dataset* beside it, and "Have a backup or a share
  export? Restore it before you add a source: knobas restores only into a database that holds no data yet." with
  *Open Settings › Backup and export*. With a source and no ticket, its sentence is unchanged.
- **`#/settings/sources/add`** is Settings › Sources with *Add a source* already open. Closing the dialog replaces the
  address with `#/settings/sources`; it is not remembered as Settings' tab.
- **Settings › Sources with no source** keeps its sentence, adds the restore line (link *Backup and export*), and on the
  demo profile *Load the Tidewater dataset* after *Add a source…* in its header.
- **Any source added** (from either way in) opens the new source's page, where the first sync's walk draws through
  `watch_sync` and a by-hand source's *Sync now* stands. The toasts are unchanged.
- **The Tidewater dataset** (`demo_load`, only the wizard called it): the button above, shown on the demo profile with
  no source only. While it runs it is disabled and reads *Loading…* in the room of its longer label. "Loaded the
  Tidewater dataset." on success, "Could not load the Tidewater dataset: <why>" on failure, no navigation. It reseeds
  credential health (`demo_load` emits no `source:health`), which also ends Tickets' zero-source state; in Settings
  the list's own re-read does that. Not loaded automatically: after `just live-sources-remove`, or a live demo with
  every product down, the profile has no source, and an automatic load would put the mock corpus into the profile
  Björn uses. No launcher row (the 2026-10-21 ruling stands).
- **The launcher's empty board** says "Nothing has been synced yet. Add a source under Settings › Sources."
- **Dropped with nothing in their place:** the "mirrored N items" sentence and the first-sync bar (a source's row and
  page show its count and its run), and the launcher opening itself after setup.

## What was removed

- `app/src/lib/sources/FirstRun.svelte`, its test, `tf/css/first-run.css`; the wizard branch of `App.svelte`.
- IPC (a frozen surface, contract §10.8): `complete_first_run`, `sync_now_with_progress`, and `AppStatus.first_run`.
  `settings::sources_and_first_run` became `settings::source_count`. No migration: the `first_run.completed` row stays
  in databases that have it, and nothing reads or writes it; the backup and share code that names the key keeps it.
- The legacy address `#/first-run` (from `tf/address.ts` and the old `shell/router.ts`): the webview opens on no
  address and knobas keeps none, so nothing can hold it.
- The dev fake's `?fake-first-run` became `?fake-empty=sources`: no source and no ticket, only what *Add a source*
  stores since.
- Kept: `demo_load`, `watch_sync`, `SyncTrigger::FirstRun` (the wake after *Add a source* logs it), `AppStatus.source_count`
  and `demo`, `sources/walk.ts` (a source's page draws it), the boot screen (`Booting.svelte`, the database start and
  its errors, the updater's line), restore into a fresh profile, `knobas --default-theme`.
- The engine's claim on a source's first sync (`Scheduler::trigger` with `FirstRun` and a sink, ADR-0005) is kept and
  tested in `knobas-sync`, though no app command asks that way any more. Retiring it is a separate change.

## Found in the real app

A fresh demo profile's *Load the Tidewater dataset* loaded, said so, and left Tickets reading "No tickets synced yet"
over a full mirror. The wizard had handed over to a shell mounted after the load; now the shell is up first. A role
view re-reads when a run of a source filling its role ends with something new (`tf/role-sync.svelte.ts`, `ran`), and
that was judged only as the run's `sync:state` arrived: the load's run ends before the shell has read the source list
again, so the run counted for no role. Now a changed run of a source the list does not name yet is held, and counts
once the list names it as filling the role (`SourceRoles.knows`, `role-sync.test.ts`). The same holds for any source
whose first run ends before the list is read again.

## Not affected

`2026-11-15-first-opens-without-the-forge.md` is about pages' first opens waiting on the forge, not the wizard.

## Reversing it

Bring back `FirstRun.svelte` and the `App.svelte` branch, the two commands and the `AppStatus` field (from this
branch's parent), and `?fake-first-run`. The zero-source state can stay either way.
