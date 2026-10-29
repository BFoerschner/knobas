- **The IPC command schema and the Rust handler barrel — issue #1481 (2026-10-21): four uncalled
  commands retire, `clones_root`, `checkout_commands`, `set_checkout_command` and
  `create_context`, with the `CheckoutCommand` DTO.** No frontend file outside the dev fake, the
  generated mirror and tests called any of them. A retirement, not an addition: nothing is added
  to either barrel, and no file is added to or removed from `commands/` or `app/src/lib/ipc/`.
  The handler list is append-only (§10.8's frozen list); its four deleted lines are the exception
  this entry ratifies.

  **What it supersedes.** The entry files are not edited; these parts of them no longer hold:

  - #499's `clones_root() -> Option<String>`. The root reaches the app on `checkout_settings`
    (`CheckoutSettings.clones_root`, #844) and on every `CheckoutView.clones_root`, and
    `set_clones_root` stays. `knobas_app::checkout::clones_root`, the Rust read the scan, the
    clone (#1442) and `checkout_settings` go through, stays.
  - #501's `checkout_commands() -> Vec<OpenCommandView>` and `set_checkout_command(action,
    template)`, with `OpenCommandView` (mirrored as `CheckoutCommand`), and #844–846's bullet that
    kept the two "as a view over the list". #844's ruling that kept them
    (`docs/archive/2026-10-unattended-rulings.md`) names its reversal, "retire both once the
    rooms UI goes", and that is met: #881 removed the rooms UI, and the desktop witness's
    open-in-editor driver writes the list through Settings › Checkouts and Open in…
    (`checkout_settings`, `set_checkout_applications`). #501's three stored templates
    (`checkout.command.vscode`, `.jetbrains`, `.terminal`) are still read over the shipped list
    until a list is saved, so a database that holds one keeps it; nothing writes those keys any
    more.
  - #47's `create_context(title) -> ContextRow`, the ad-hoc mint, which #881 left uncalled and the
    2026-10-21 grilling (#1427) left "until a cleanup pass". Stored ad-hoc contexts are still read
    (`list_contexts`, `context_members`, the scope on `list_entities`), and `promote_context`
    still emits `contexts:changed`. `knobas_core::context::create_adhoc` stays.

  **The diff, by surface.**

  - `crates/knobas-app/src/lib.rs`'s handler list loses four lines: `commands::entity::
    create_context`, `::clones_root`, `::checkout_commands` and `::set_checkout_command`.
  - `commands/entity/checkouts.rs` loses the three checkout commands; `commands/entity/contexts.rs`
    loses `create_context` and `create_context_inner`.
  - `knobas_app::checkout::open` loses `OpenCommandView`, `commands` and `set_command` (and
    `checkout`'s re-export of them); `ipc_mirror.rs` stops exporting `OpenCommandView`, and the
    regenerated `app/src/lib/ipc/generated/entity.ts` no longer has `CheckoutCommand`.
  - `app/src/lib/ipc/entity.ts` loses `createContext`, `clonesRoot`, `checkoutCommands`,
    `setCheckoutCommand` and its `CheckoutCommand` import and re-export.
  - The fake IPC loses the four handlers (`fake/checkouts.ts`, `fake/contexts.ts`).

  **What is not touched.** No migration, no event, no `WriteOp`, nothing under
  `crates/knobas-source/src/**` or `crates/knobas-http/**`, `crates/knobas-app/src/{error,
  profile}.rs` unchanged. No other uncalled command is removed: `detect_suggestions` is #1485's.

  Pinned by `fake-tauri.test.ts`'s census (every command the fake answers is one the barrel
  invokes, and every barrel command is answered or on its `UNANSWERED` record), `ipc_mirror` (the
  generated barrel is current), and `checkout_ipc.rs`, whose opening tests now write the list
  with `set_applications` where they used #501's write. The rulings are in
  `docs/archive/2026-10-unattended-rulings.md` under `## #1481`.
