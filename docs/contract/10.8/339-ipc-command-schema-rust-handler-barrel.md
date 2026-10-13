- **The IPC command schema, the Rust handler barrel and the event schema, issue #339 (2026-09-04):
  one command, `notify`, and one event, `notification:clicked` — a desktop notification's click gets
  a channel.**

  **This revises #290's "nothing new crosses the bridge as an event."** #290's entry recorded the
  click as built and, on desktop, unreachable: `tauri-plugin-notification` 2.4.0's desktop `notify`
  hands the desktop notification to `notify-rust` and drops the handle (`let _ = notification.show()`),
  and `register_listener`, the command behind `onAction`, exists on mobile only. Nothing that already
  crossed the bridge could carry a click, because nothing in the process could *learn* of one — the
  handle a click arrives on did not survive the plugin's own `notify`. So the send moves into knobas
  and the click comes back as an event. Decided in the 2026-09-04 grilling of #339 (the two Triage
  Notes on the issue) and **ratified by Björn there**; the load-bearing fact — `wait_for_action`
  reporting a banner click from a worker thread inside a signed Tauri bundle — was witnessed first on
  the #339 prototype; its verdict table on the issue is the record that remains (the
  `prototype/notification-click` branch was deleted 2026-10-11).

  **The command.** `notify(draft: NotificationDraft) -> Result<(), IpcError>`, `NotificationDraft` =
  `{ title, body, address }`, appended to the **foot** of `crates/knobas-app/src/lib.rs`'s
  `generate_handler!` list and mirrored at the foot of `app/src/lib/ipc/entity.ts`, in the existing
  `commands::entity` module — the notifier's setting lives there (#290) and the `commands/` + `ipc/`
  layout is untouched. The rules live in `crates/knobas-app/src/notify.rs`, which is not a frozen
  surface: `notify-rust = "=4.18.0"` with `preview-macos-un` (the `UNUserNotificationCenter`
  backend, Björn's ruling on the prototype's evidence) shows the desktop notification, and the handle's
  `wait_for_action` runs on a thread of its own. The wait is unbounded — the prototype showed it
  returns on a click or on the reader clearing the desktop notification, never on the banner sliding
  away — so there is a registry: one waiter per address (a second send for an address already
  waited on shows and starts no second wait) and at most sixteen concurrent waiters (a send beyond
  the cap shows fire-and-forget and says so at `debug`); a completed wait frees its slot. No
  timeout is invented: the OS gives none and a made-up one would drop real clicks.

  **The event.** `notification:clicked`, payload `NotificationClicked = { address }`, appended to
  `knobas_app::events` and `EVENTS` in `app/src/lib/ipc/index.ts`. Emitted through the existing
  `sources::events::TauriEvents` adapter under a trait of its own, `notify::NotificationEvents`,
  rather than through `knobas_sync::scheduler::SyncEvents` — that trait is the sync crate's and a
  click is not a sync fact; what is shared is the bridge and the reason for a trait at all, that a
  test observes the emit without Tauri. `"default"` (or any action id) emits with the address;
  `"__closed"` emits nothing. The frontend's `NotifyPorts.send` / `onAction` keep their shape: the
  real `send` invokes `notify` and the real `onAction` is a `listen` on the event handing the store
  `{ extra: { address } }`, so the store's own read of the address is unchanged.

  **What leaves.** `notification:allow-notify` leaves `capabilities/default.json` — knobas no longer
  calls the plugin's `notify` — and the file's description says two calls, not three. The plugin
  stays, pinned as it was, for `is_permission_granted` and `request_permission`. Features unify, so
  the plugin's copy of `notify-rust` is on the UN backend too, which changes nothing knobas still
  calls (its desktop permission calls answer `Granted` without reaching `notify-rust`).

  **Untouched:** `crates/knobas-source/src/**`, migrations (none — nothing is stored),
  `crates/knobas-http/**`, `crates/knobas-app/src/{error,profile}.rs`, and every existing command,
  DTO field and event name.

  **Witnessed and not.** macOS, in the signed bundle launched from the Launch-Services-registered
  path: `testenv/README.md`'s "Signed dev build" carries the click step and the caveat that a UN
  click activates the bundle registered for `dev.knobas.desktop`, so a copy elsewhere spawns a
  second instance. A bare `tauri dev` binary is refused by `UNUserNotificationCenter` (*no bundle
  identifier*); the send rejects, the store swallows it, and the dev terminal says so at `warn` —
  the trade Björn's UN ruling makes, recorded in the README. **Linux (the freedesktop `default`
  action) and Windows (the handle API) are written from `notify-rust` 4.18.0's sources and are
  unwitnessed**, disclosed in the three places #290 used: `notify.rs`'s module header,
  `notify.svelte.ts`'s module header, and the README.

  **Pinned by:** `notify::tests` (the registry over an injected backend: the second send, the
  seventeenth waiter, the freed slot, the slot held until the click has left, the click emitted
  and the clear not, a refused show),
  `tests/entity_mirror.rs` (`NotificationDraft`, `NotificationClicked`), `lib.rs`'s
  `the_event_names_match_their_typescript_mirror`, `commands::entity`'s barrel-versus-mirror test,
  `tests/wiring.rs` (the two grants and no third), and `notify.test.svelte.ts` (the store with the
  real click channel and the real send over faked Tauri APIs).

  Ratified by Björn in the 2026-09-04 grilling of #339, whose Agent Brief specifies the command,
  the event, the registry's two rules, the capability change, the live check and this entry.

