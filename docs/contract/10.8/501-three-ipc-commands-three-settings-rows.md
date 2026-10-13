- **Three IPC commands and three settings rows — issue #501 (2026-09-08): the command templates
  and the spawn behind *Open in VS Code*, *Open in JetBrains* and *Open terminal here*.**

  The other half of #499's entry above, and **the first feature in knobas that starts a process on
  the user's machine**. Ratified in advance by spec #491 (§5, §11, story 29) and by ADR-0016, *a
  spawned command takes no argument from the mirror*, whose consequences this entry implements.
  **Björn keeps the gate for frozen contracts and this entry is flagged for his review.**

  **No migration.** The three templates are three `knobas.setting` rows —
  `checkout.command.vscode`, `checkout.command.jetbrains`, `checkout.command.terminal` — under
  `0002`, comment 6's store, for the clones root's reason: three strings a person types once are
  not a relation. `0024` is untouched and `knobas-db/migrations/**` gains nothing.
  `knobas_app::checkout::command_key` builds the key and
  `every_action_stores_its_template_under_its_own_key` pins all three spellings.

  **The IPC schema — three commands and one DTO, all additive:**

  * `checkout_commands() -> Vec<OpenCommandView>` — every action with the command it would run
    here: the stored template, else this platform's default, else `null`, which is *not
    configured*. Read by the settings section **and** by the checkout panel, because a button with
    no template is drawn disabled rather than pressed into a refusal.
  * `set_checkout_command(action, template: Option<String>) -> Vec<OpenCommandView>` — set or
    clear, answering the fresh list so a field draws what is now stored. The template is checked
    **here**, where somebody is typing it: a refusal that arrived days later at an editor which
    did not open is one nobody can act on.
  * `open_checkout(entity_id, action) -> ()` — resolves the checkout through `entity_checkout`'s
    own path, expands the template, and starts the program. It returns when the program has
    *started*; an editor drawing a window is not something an IPC call can wait for.
  * `OpenCommandView { action, label, template, is_default }`, mirrored in
    `app/src/lib/ipc/entity.ts`. The **label is on the wire**, so the panel draws one button per
    row and no component carries a second spelling of "Open in JetBrains" to drift.
    `app/src/lib/shell/dev/fake-tauri.ts` restates the three labels and the three macOS
    templates, as a fixture that answers this command has to; that is the `?fake-ipc` corpus's
    standing licence and not a second implementation of anything.

  **Where the argument rule is enforced.** `knobas_core::checkout::expand(template, path)` takes
  **one `&Path`**, so no field of a mirrored repo is in scope to substitute and widening that
  would mean changing a public signature — the visible act ADR-0016 wants it to be. `{path}` is
  the only placeholder; every other `{...}` is refused **by name**, so a person who typed
  `{repo_url}` reads why it is not there. The expansion answers an **argv**, spawned with no
  shell: a checkout path holding a space, a `$` or a `;` is one argument and stays one, and a
  template cannot pipe, redirect or chain. macOS starts with `open -a "Visual Studio Code"
  {path}`, `open -a "IntelliJ IDEA" {path}` and `open -a Terminal {path}`; every other platform
  starts with none, and the buttons there read *not configured* until one is set, because knobas
  has never been run off macOS by anyone and a guess that spawns a process is the wrong kind of
  guess. **The platform is an argument, not a `cfg!`**, in all three deciders --
  `OpenAction::default_template_on`, `command_view` and `template_or_refusal` -- because the gate
  runs on one operating system and a rule written as `cfg!(target_os = ...)` has exactly one half
  of itself under test. *Not configured* is a state no Mac can reach through the database, and
  those three signatures are what let a test reach it at all.

  **What is not touched.** **`crates/knobas-app/capabilities/default.json` is unchanged** — the
  opener plugin's scope stays `http` and `https`, which is the point: the reason this feature is
  a command template at all is that a URL scheme handler is exactly the argument a mirrored
  `web_url` could carry (ADR-0016's first rejected option). No `vscode://`, no `file://`, no
  Tauri shell plugin. The `commands/` + `ipc/` module layout is **unchanged**: all three live on
  the `entity` pair beside #499's four. **Neither append-only barrel grows a module line**
  (`app/src/lib/ipc/index.ts` re-exports `./entity` already); `crates/knobas-app/src/lib.rs`'s
  handler list grows the three commands, which is what that list is append-only for. No event.
  No migration. Nothing in `crates/knobas-source/src/**` — no adapter learns that a checkout can
  be opened — nothing in `crates/knobas-http/**`, and `crates/knobas-app/src/{error,profile}.rs`
  are untouched: `TemplateError` is `knobas_core::checkout`'s own enum and reaches the window as
  an `IpcError::invalid` message, so `CoreError` gains no variant. No `WriteOp`: nothing here
  writes to a source, and nothing here writes to a working tree either — no clone, no checkout,
  no fetch. The keychain is not involved. The share export's part list is unchanged and the three
  rows ride in `knobas.setting`, which is in no part: a command naming `/Applications` means
  nothing on a colleague's machine.

  Pinned by: `knobas_core::checkout`'s `the_path_is_substituted_once_inside_its_own_word`,
  `quotes_group_a_word_and_are_not_passed_on`, `a_path_with_a_space_stays_one_argument`,
  `a_placeholder_that_is_not_the_path_is_refused_by_name` (six placeholder names, ADR-0016's own
  test), `an_unusable_template_says_what_is_wrong_with_it` (six refusals),
  `the_three_macos_defaults_expand_to_the_documented_commands`,
  `only_macos_starts_with_a_template` (both halves, on any platform) and
  `every_action_is_found_by_its_own_id`; `knobas_app::checkout`'s
  `every_action_stores_its_template_under_its_own_key`,
  `the_open_command_serialises_the_keys_the_mirror_declares`,
  `an_action_that_does_not_exist_is_refused_by_name`,
  `an_action_with_no_template_on_this_platform_is_not_configured` (the arm the gate's machine
  cannot reach through a database) and
  `a_stored_template_wins_and_the_platforms_answers_when_nothing_is`; `commands::entity`'s
  `the_mirror_invokes_the_commands_by_their_registered_names` and `tests/wiring.rs`'s
  `every_command_is_in_the_handler_list`, which is what makes the three reachable from the window,
  beside `the_window_may_open_http_urls_and_only_http_urls`, which is the standing guard on the
  opener scope this entry leaves alone; eight in `crates/knobas-app/tests/checkout_ipc.rs` over
  scratch databases and a **recording stub** —
  `opening_a_checkout_runs_the_template_with_the_scanned_path` and
  `an_override_is_what_the_command_is_given` assert the argument list a real process was handed,
  `a_branch_opens_its_repositorys_checkout`, `a_repo_with_no_checkout_refuses_and_names_it`,
  `a_template_that_cannot_be_run_is_refused_where_it_is_typed`,
  `a_program_that_will_not_start_names_the_template`,
  `a_stored_template_wins_over_the_platform_default_and_clearing_gives_it_back` and
  `clearing_a_template_hands_this_platforms_default_back`; and, on the rendered side, seven in
  `CheckoutPanel.test.svelte.ts` (the failure message names the template, which the panel
  composes itself) and six in `CheckoutsSection.test.svelte.ts`.

  **One criterion of #501 is open and disclosed**, per the deputy's ruling of 2026-09-08 on #500:
  the desktop witness has not run. `testenv/README.md`'s *What is not witnessed yet* carries it,
  and it is owed to **#525**.

  *The sentence above was true when it was written and was superseded on 2026-09-08: the screen
  was unlocked and the harness ran — see **Five IPC commands, one event, one plugin and a second
  capability — issue #503** below, which carries the new truth. This criterion is still open, and
  now for the demo corpus alone: `open-in-editor` reaches the step where a repo detail should
  open and `--demo` carries no repo entity.*

  *And that amendment was itself superseded the same day, by **#537**: the corpus does carry a repo
  entity now — `mock:payout-service`, plus two more repositories and three branches — so what is
  left open on this criterion is the **run**, and nothing else. `just desktop-witness
  open-in-editor` has still never been driven past the harness's readiness check on this Mac, and
  the run is owed to **#525**. The #503 entry below carries the same correction at length.*
