- **The IPC answer of `launcher_home` — issue #1036 (2026-10-17): `notes_shown`.** The launcher
  drops its `note:` syntax row and the `?` card's entry while *Show notes and quick capture*
  (R-13a.6) is off, so the board's answer gains `notes_shown: bool`, the one read of
  `capture.notes_shown` (`crate::capture::shown`) the searcher already makes. No migration, no
  grammar key, no `WriteOp`, no new command, no `commands/` or `ipc/` module.

  Mirrored in `app/src/lib/ipc/search.ts` (`LauncherHome.notes_shown`) and held by
  `the_launcher_home_shape_matches_its_typescript_mirror`; the fake IPC answers it from its
  notes setting (`fake/launcher.ts`, `fake/capture.ts`). Witnessed by `session.test.svelte.ts`
  and `notes_search_ipc.rs`.
