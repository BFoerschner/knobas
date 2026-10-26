- **`crates/knobas-spi-types` (`KindPaths`), `crates/knobas-source/src/**` (the contract battery)
  and Gitea's descriptor — issue #2173 (2026-10-26): the pull-request time declarations leave
  with their reader.** **Retires #929's `opened` and `merged_at`**, which #2164 left standing with
  no reader once the conversation went comments-only: `KindPaths.opened` and `KindPaths.merged_at`
  leave the struct, `field::OPENED` and `field::MERGED_AT` leave `field` and `field::ALL`, the
  battery's scalar weighing drops both (`Wanted::Instant` stays for `started` and `finished`),
  Gitea's pull-request declaration stops naming `created_at` and `merged_at`, and the mirrored
  `KindPaths` in `app/src/lib/ipc/generated/sources.ts` loses both keys. Wire-compatible:
  `KindPaths` does not deny unknown fields, so a descriptor from an older out-of-process adapter
  that still sends them decodes, keeps every other declaration and drops the two. No migration
  (no SQL macro or stored row reads either key), no command, no `commands/` or `ipc/` module. The
  Gitea pull-request view's own `merged_at` read (`view/pr.rs`) is untouched. A future reader of
  either time declares it again under its own entry. Witnessed by `knobas-spi-types`'
  `a_peer_that_still_declares_pull_request_times_decodes_without_them` and
  `the_field_names_the_sql_macros_use_are_the_serialized_ones`.
