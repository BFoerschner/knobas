- **Five IPC commands, one event, one plugin and a second capability — issue #503 (2026-09-08):
  the capture window and its global shortcut.**

  The third of spec #491's three OS-level features, and the one that reaches furthest outside the
  application: a shortcut registered with the window server, held while knobas is not the
  application being typed at. Ratified in advance by spec #491 (stories 40–43) and by the ticket's
  own first criterion, which asks for this entry by name. **Björn keeps the gate for frozen
  contracts and this entry is flagged for his review — ratified in his absence by the deputy's
  ruling of 2026-09-08 on #503 (`docs/archive/2026-09-v1-5-unattended-rulings.md`).**

  **No migration.** The shortcut is one `knobas.setting` row — `capture.shortcut` — under `0002`,
  comment 6's store, for the clones root's reason: one accelerator a person types once is not a
  relation. `knobas_app::capture::SHORTCUT_KEY` carries the spelling and
  `the_shortcut_is_stored_under_its_own_key` pins it. `crates/knobas-db/migrations/**` gains
  nothing. What the main window records for a capture — the room and the foreground — is **in
  memory** and deliberately not stored: both are facts about *this run*, and a room the reader
  stood in last Tuesday surviving a restart is the one direction nobody would notice.

  **The IPC schema — five commands, three DTOs and one event, all additive:**

  * `capture_shortcut() -> ShortcutView` and
    `set_capture_shortcut(accelerator: Option<String>) -> ShortcutView` — read and write the
    setting, and register what was written. The write **answers the fresh view** so the field draws
    what is now stored, which is `set_checkout_command`'s rule.
  * `record_capture_context(context: Option<String>, foreground: Option<String>)` and
    `capture_context() -> Recorded` — the pair the **main** window records and the **capture**
    window reads. Neither takes the pool, so both answer during bring-up: the capture window can be
    opened seconds after launch, and where the reader was standing is true whether or not
    PostgreSQL is.
  * `reveal_note(note_id)` — bring the main window forward and tell it which note to open. It does
    not close the capture window; the capture window closes itself, so a main window that will not
    come forward leaves the reader holding the words they typed.
  * `ShortcutView { accelerator: Option<String>, refusal: Option<String> }` and
    `Recorded { context: Option<String>, foreground: Option<String> }`, mirrored in
    `app/src/lib/ipc/entity.ts` as `CaptureShortcut` and `CaptureContext`.
  * **`capture:open-note`**, the first event added since #339's `notification:clicked`. Payload:
    the note's entity id, as a bare string. Sent **to the `main` window by label** and not
    broadcast, because the capture window is closing and has no use for it.
    `knobas_app::events::CAPTURE_OPEN_NOTE`, mirrored in `app/src/lib/ipc/index.ts`'s `EVENTS` and
    pinned by `the_event_names_match_their_typescript_mirror`.

  **A refused shortcut is state, not an exception.** `set_capture_shortcut` stores what the reader
  asked for and the answer says why it is not holding; it does **not** reject. The alternative —
  refusing the write the way `set_checkout_command` refuses an unusable template — was rejected
  because it covers only half the population: a combination another application grabs *after* it
  was stored has no write to refuse, and the settings pane would then need a second way of saying
  the same thing. The stored-but-not-registered state is therefore the one shape both cases reach,
  and bring-up records it the same way.

  **The plugin, and where its capability is not.** `tauri-plugin-global-shortcut = "=2.3.2"`,
  pinned exactly like the two plugins before it — but for the opposite reason, and the difference
  is the entry: **there is no npm half**. The shortcut is registered from Rust
  (`knobas_app::capture`), at bring-up and on every change; the plugin's four commands
  (`register`, `unregister`, `unregister_all`, `is_registered`) are reachable from **no window**,
  because no capability grants them. So `app/package.json` gains nothing and
  **`capabilities/default.json` is unchanged** — a webview able to register a global shortcut would
  be a webview able to take a key combination away from every other application on the machine.
  `the_global_shortcut_plugin_is_pinned_and_has_no_javascript_half` and
  `the_global_shortcut_plugin_is_registered_with_a_handler` are what hold both halves of that.

  **A second capability file, `capabilities/capture.json`.** `default.json` is scoped to
  `["main"]`, so the capture window is covered by nothing, and a window with no decorations has no
  way to shut itself. It grants **one** permission — `core:window:allow-close` — and not
  `core:default`, whose window set is read-only getters and carries no close at all.
  Everything else the capture window does is one of knobas' own `#[tauri::command]`s, which the ACL
  does not gate while this application declares no manifest of its own.
  `the_capture_window_may_close_itself_and_nothing_more` asserts the list whole, and that neither
  capability mentions `global-shortcut:`.

  **A second frontend entry point, `app/capture.html`.** Not a route in the shell: the capture
  window mounts one component with none of the shell's subscriptions behind it — no context
  switcher, no health listener, no timer, no inbox poll — because the shortcut's promise is that a
  thought costs one keystroke. `app/vite.config.ts` names both inputs, and
  `the_capture_window_loads_its_own_document` fails if either the file or the Vite input goes,
  which would otherwise show up only as a blank always-on-top rectangle in a bundled build.

  **What is not touched.** The `commands/` + `ipc/` module layout is **unchanged**: all five live
  on the `entity` pair beside #499's four and #501's three, and `app/src/lib/ipc/index.ts`
  re-exports `./entity` already, so **neither append-only barrel grows a module line** —
  `crates/knobas-app/src/lib.rs`'s handler list grows the five commands, which is what that list is
  append-only for. Nothing in `crates/knobas-source/src/**`: no adapter learns that a note can be
  captured. Nothing in `crates/knobas-http/**`. `crates/knobas-app/src/{error,profile}.rs` are
  untouched — a refused registration is a field on a DTO, not a `CoreError` variant and not a new
  `IpcErrorCode`. No `WriteOp`, no keychain, no migration. **`create_note` is unchanged**: the
  capture is its second caller and passes the same two links *New note* passes, which is the whole
  of the deputy's ruling of 2026-09-08 on #502 — one command, one vocabulary, and no second
  recording of *was a detail open*. `knobas_app::settings` is new and is a **move**: the
  `knobas.setting` reader, writer and blank rule were `checkout.rs`'s private functions and now
  have two callers; no behaviour changed. The share export's part list is unchanged, and the
  shortcut row rides in `knobas.setting`, which is in no part: a key combination means nothing on a
  colleague's machine.

  Pinned by: `knobas_app::capture`'s `the_shortcut_is_stored_under_its_own_key`,
  `the_capture_window_loads_its_own_document`, `nothing_stored_registers_nothing`,
  `a_refused_accelerator_answers_the_refusal`,
  `the_plugin_parses_an_accelerator_and_refuses_a_string_that_is_not_one` and
  `what_the_main_window_recorded_is_what_comes_back`; eight in
  `crates/knobas-app/tests/capture_ipc.rs`, four of them over scratch databases —
  `nothing_is_registered_until_a_shortcut_is_chosen` (the empty default, asserted on the setting
  **and** on what the registrar was asked for),
  `a_stored_shortcut_is_registered_now_and_again_at_the_next_start`,
  `a_refused_shortcut_is_stored_and_the_answer_says_why` (and still says it on a later read) and
  `a_refusal_does_not_outlive_the_shortcut_that_earned_it` — and two over the recorded pair alone,
  which needs none: `the_recorded_room_and_foreground_are_what_the_capture_reads` and
  `a_later_record_replaces_the_earlier_one_whole`; plus
  `every_capture_command_is_registered_and_its_arguments_decode` and
  `the_recorded_pair_crosses_the_bridge_before_the_database_is_up` through the mock runtime;
  `tests/wiring.rs`' three new tests above beside `every_command_is_in_the_handler_list`; and, on
  the rendered side, twenty-eight in `app/src/lib/capture/` (the store's five decisions and the
  window's keys, including that a **bare** Return is a new line and not an exit), eight in
  `CaptureSection.test.svelte.ts`, and one in `App.test.svelte.ts` —
  `what a capture would attach is the room and the foreground the timer records` — which asserts
  the recorded pair **against the timer's own two answers** rather than against literals beside
  them, so a third derivation of the foreground fails rather than passing quietly.

  **The desktop witness ran and is green**, which no criterion in this milestone had managed
  before: `just desktop-witness capture`, 2026-09-08, a shortcut registered with the window server
  and pressed while **Finder** had the screen, asserted frontmost and not assumed. **That is a run
  of the head it was made on and not of the merged one**: it was made at 04:55, before the review's
  commits — `b4dc71d7` moved the capture window's `close` port from a component prop into
  `createCapture`'s defaults, which is the wiring behind the driver's last assertion — the
  merge-manager's re-run on the merge head refused with `screen-locked` at 06:20 CEST, and the
  re-run is **owed to #525**, which carries all three runs again (the deputy's second ruling of
  2026-09-08 on #503, parts 1 and 2). Three things it
  does **not** assert are listed in `testenv/README.md`, *What is still not witnessed*, and **none
  of them is a debt**: `captured-in`, which needs a stored room the `--demo` profile does not
  carry and which is pinned at the seam spec #491's stream map puts the capture links' witness at
  — *"the app's commands over a scratch database"* — so the driver asserting `CAPTURED FROM` is
  already one link more than that row asks of it; a combination the operating system refuses,
  which needs another application holding one and is pinned at the same seam with a stand-in
  registrar; and **which entity the born link points at**, deliberately not asserted from a
  driver, because the ticket's key is on the room behind the slide-over and a driver reading the
  whole window for it would pass with no link drawn at all.

  **One sentence in the #501 entry above is superseded, and this is where the new truth is.**
  That entry closes with *"the desktop witness has not run"*, which was true when it was written.
  §10.8 is append-only for its entries, so the sentence stands and the entry gains an italic
  pointer here, the treatment Björn's ruling of 2026-08-31 in the #112 section fixes — *"the old
  text stands and the amendment carries the new truth"* — and the idiom this file already uses
  twice. The new truth: `open-in-editor` now reaches the step it always named — the `--demo`
  corpus carries no repo entity — so **#501's criterion is still open, for that reason and no
  longer for a locked screen**. Its driver was also *unrunnable as merged*, looking for three
  accessible names no element carries; that is fixed in this change and pinned by
  `rendered_label`'s cases in `witness-unit`. There is **no #500 entry in §10.8** — that ruling
  closed by recording that the frozen surface was untouched — so nothing is owed there, and that
  #500's criterion is met is recorded by a transcript on its own ticket.

  *The paragraph above was true when it was written and was superseded the same day by **#537**,
  which gave `knobas_source_mock::items` the fixture's three repositories and three branches: the
  `--demo` corpus **does** carry a repo entity now, `mock:payout-service` under the name the
  driver asks for, and `crates/knobas-app/tests/demo.rs`'s
  `the_demo_corpus_answers_the_checkout_the_desktop_driver_opens` holds the chain from the demo
  load to `checkout::view` matching the very remote the driver plants. #501's criterion is still
  open, and now for the run alone, owed to **#525** — `just desktop-witness open-in-editor` on
  #537's head was refused by the harness's readiness check (`screen-locked 1`, exit 1) before the
  build, before Launch Services and before the driver was invoked, so nothing past that refusal
  has been observed on it. There is **no #537 entry in §10.8**:
  `crates/knobas-source-mock` is on no frozen list, and its change touches no migration, no IPC
  schema or layout, neither barrel, nothing in `crates/knobas-source/src/**` and nothing in
  `crates/knobas-http/**` — checked against the list above rather than against the ticket's claim.
  So the new truth is carried here, in the entry whose sentence it supersedes, rather than by an
  entry that is not owed. Deputy's ruling of 2026-09-08 on #537, part 2.*
