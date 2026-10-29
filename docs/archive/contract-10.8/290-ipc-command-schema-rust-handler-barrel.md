- **The IPC command schema and the Rust handler barrel, issue #290 (2026-09-03): two commands on
  `commands::entity` — which inbox categories may raise a desktop notification, and setting them.**
  `notification_kinds` and `set_notification_kinds` are appended to the **foot** of
  `crates/knobas-app/src/lib.rs`'s `generate_handler!` list — not beside the other
  `commands::entity::` lines — because that barrel is append-only and #288 was appending to it in
  the same week; the mirror halves go at the foot of `app/src/lib/ipc/entity.ts`.
  `app/src/lib/ipc/index.ts` gains no line: it re-exports whole modules (`export * from "./entity"`),
  so a function added to an existing mirror is already exported. No existing command, DTO field or
  event name changes meaning, and the `commands/` + `ipc/` module layout is untouched — the setting
  is about the inbox's own categories and the inbox lives in `entity`, so no new module pair was
  needed and none was asked for.

  **No migration.** `knobas.setting` exists for exactly this (`0002`, comment 6) and the key is
  `inbox.notification_kinds`, a JSON array of category words. The precedent is
  `backup::SCHEDULE_KEY` and, on the same settings screen, #282's `time.passive_attribution`.
  The backup export needed no change either, for the reason #283's entry records: the dump is
  schema-scoped, so the row rides in it already.

  **The write side is strict and the read side is forgiving, deliberately.** `set_notification_kinds`
  takes `Vec<String>` rather than `Vec<Category>` — the shape `unlink` takes its id in — so a word
  this build has no category for arrives at the settings section as `invalid` with a sentence in it
  rather than as Tauri's own bare decode failure, which carries no `IpcErrorCode` and would read to
  the reader as a window that broke. The read drops what it cannot parse and answers `[]` for a
  stored value that is not a list of category words at all: the setting is a *permission to
  interrupt somebody*, so the safe direction for one knobas cannot read is silence. Both directions
  are pinned in `tests/inbox_ipc.rs`, including a partly-readable list keeping the half this build
  knows.

  **What comes back is one spelling for one set** — deduplicated and in `Category::ALL`'s order,
  never the caller's. The section sends the whole set back on every click, so an order that followed
  the ticking would make two identical settings compare unequal.

  **The plugin and its capability are *not* on the frozen list, and this entry says so rather than
  leaving a reader to infer it.** Spec #272's sub-milestone map calls "notification plugin and
  capability" an M3.3 frozen-surface touch; §10.8's list is migrations, `crates/knobas-source/src/**`,
  the IPC command and event schema with the two barrels, `crates/knobas-http/**` and
  `crates/knobas-app/src/{error,profile}.rs`, and neither `crates/knobas-app/Cargo.toml` nor
  `capabilities/default.json` is in it. They are recorded here anyway because they are the only
  other place this ticket touches something a reader cannot see fail:
  `tauri-plugin-notification = "=2.4.0"` is pinned exactly and to the same version as
  `@tauri-apps/plugin-notification` (`tests/wiring.rs` now checks that for **both** plugins, which
  nothing did before), and the capability grants
  `notification:allow-is-permission-granted`, `notification:allow-request-permission` and
  `notification:allow-notify` one at a time rather than `notification:default`, which bundles
  sixteen — channels, scheduling, cancelling, reading back what is on screen. A missing grant is
  denied at run time with nothing failing in the build, which is what the two capability tests are
  for. *Superseded in part by #339's entry below (2026-09-04): the `notification:allow-notify` grant
  left `capabilities/default.json` when knobas stopped calling the plugin's `notify`, and the list
  above is left as history rather than rewritten, the treatment the #53 entry gives the #52 sentences
  it supersedes.*

  **Nothing new crosses the bridge as an event.** The notifier listens to the inbox moving, which is
  `activity:new` and `sync:state` re-read through the existing store — `inbox.svelte.ts`'s own
  argument for why there is no `inbox:*` event, applied one layer up. The one addition on that store
  is `answered`, shell state and not a wire shape: it says the stream is a *read* rather than the
  empty list the store was built with, which is what keeps the reader's backlog from arriving as a
  burst of desktop notifications the moment knobas opens.

  **The click is built and, on desktop, unreachable — recorded here because it is a criterion.**
  A desktop notification carries the item's address in `extra` and the store subscribes to the plugin's own
  action channel to navigate there. `tauri-plugin-notification` 2.4.0 registers exactly three
  commands on desktop (`is_permission_granted`, `request_permission`, `notify`); `register_listener`,
  which `onAction` invokes, is mobile-only, and the desktop `notify` hands the desktop notification to
  `notify-rust` and returns. So the subscription rejects on macOS, the store swallows that, and the
  navigation is proven against the stub and not against the OS. The alternative was to ship no click
  path at all; this way the door exists the day the plugin reports a click, and the gap is written
  down instead of being a criterion nobody can check. *Revised by #339's entry below (2026-09-04):
  the click has a channel now — knobas' own `notify` command and `notification:clicked` event — and
  the sentence above about nothing new crossing the bridge as an event no longer holds; the door
  this entry describes is the one that event feeds.*

  Ratified by the orchestrator as spec #272 and issue #290, whose acceptance criteria specify the
  plugin with its capability, the setting key with its per-category toggles, the listener with its two
  gates, the click, the component tests and this entry.
