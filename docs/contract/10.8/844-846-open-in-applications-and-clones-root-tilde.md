- **The IPC command schema and the Rust handler barrel — issues #844 and #846 (2026-10-16): two new
  commands, `checkout_settings` and `set_checkout_applications`; `open_checkout` takes an
  application, a file, a line or a command, and answers the argv it started; `set_clones_root`
  expands a leading `~`.** Spec #808, stories 104 to 108, requirements R-8.21 to R-8.25 and
  R-13a.3; milestone TF3. The amendment to #501's entry that its three templates become a named
  list (spec #808, Impl "Code", first bullet).

  **One settings row, no migration.** The list and its default are one `knobas.setting` row,
  `checkout.applications`, a JSON object `{ "default": <id>, "applications": [{ id, name,
  command, file_command }] }`, under `0002`'s comment 6 store, for the clones root's reason.
  `knobas-db/migrations/**` gains nothing. Until somebody saves the list there is no row, and the
  list is this platform's shipped one (VS Code, Terminal, Fork and IntelliJ IDEA on macOS, as
  `open -a <application>` commands; none elsewhere) with #501's stored templates
  (`checkout.command.vscode`, `.jetbrains`, `.terminal`) over the entries of the same id: today's
  templates become the first entries. A row that does not parse reads as no row.

  **The IPC schema.** In the **existing** `commands/entity.rs`, beside #501's three:

  ```rust
  #[tauri::command] pub async fn checkout_settings(..)
      -> Result<knobas_app::checkout::CheckoutSettings, IpcError>;
  #[tauri::command] pub async fn set_checkout_applications(..,
      applications: Vec<knobas_core::checkout::open::Draft>, default_application: Option<String>)
      -> Result<knobas_app::checkout::CheckoutSettings, IpcError>;
  #[tauri::command] pub async fn open_checkout(.., entity_id: String,
      application: Option<String>, file: Option<String>, line: Option<u32>,
      command: Option<String>) -> Result<Vec<String>, IpcError>;
  ```

  - `CheckoutSettings { clones_root, git: GitFound, default_application: Option<String>,
    applications: Vec<Application> }`; `Application { id, name, command, file_command:
    Option<String> }`; `Draft` is the same with `id: Option<String>` (a new entry has none and
    gets one made from its name, unique in the list). `GitFound` is tagged on `status`: `found {
    path }`, `no_git`, `failed { reason }`, from `working_tree`'s own `locate`: where the git on
    `PATH` is, and not its version, since knobas runs git only for `status`, `diff` and `clone`.
  - `set_checkout_applications` saves the whole list in order and the default (a default naming
    no entry is the first), refusing with `invalid` an entry with no name or a command or file
    command that does not parse, naming the entry.
  - `open_checkout`'s `action` argument is renamed `application`: an id from the list, the default
    when `null`. `command`, when given, runs that unsaved command instead (*Try it* in *Add an
    application*). `file` is a path relative to the checkout and must be a file inside it on disk
    (by its words, and by its canonical path, which catches a symlink out); with one, the
    application's file command runs. The answer was `()` and is now the argument vector started,
    which the toast prints.
  - `checkout_commands` and `set_checkout_command` keep their shapes and become a view over the
    list: one `OpenCommandView` per application (`action` its id, `label` "Open in <name>",
    `is_default` whether the command is the one shipped for that id), so the rooms UI's panel and
    settings keep working (the panel now passes its action as `application`), and ADR-0016's
    desktop witness that drives them runs with no change to its driver.
  - `set_clones_root` stores a leading `~` as `$HOME` (*Where should clones go?* offers `~/code`,
    and nothing knobas runs is a shell that would expand it).

  Mirrored in `app/src/lib/ipc/entity.ts` (`CheckoutApplication`, `CheckoutApplicationDraft`,
  `GitFound`, `CheckoutSettings`, `checkoutSettings`, `setCheckoutApplications`, `OpenAt`,
  `openCheckout(entityId, at)`) and in the fake IPC (`fake/checkouts.ts`).

  **The placeholders (ADR-0016).** `{path}` the checkout, `{file}` the file inside it, `{line}` the
  line or 1, `{name}` the checkout folder's own name, all from this disk
  (`knobas_core::checkout::open::Fill` holds two paths and a number). Any other placeholder is
  refused by name. The command is split into words, quotes grouping only, and run without a
  shell. `{path}` is no longer required: its absence, `{file}` in the checkout command and shell
  characters are the dialog's warnings (R-8.23), not refusals.

  **What is not touched.** No migration, no event, no `WriteOp`, nothing under
  `crates/knobas-source/src/**`, no new module in either barrel or in `commands/`: the code is
  `knobas_core::checkout::open` (replacing `OpenAction`, `expand` and `check_template`) and
  `knobas_app::checkout::open`, child modules of `checkout`. The handler list grows by two lines at
  its foot.

  Pinned by `checkout_ipc.rs` (the default and a named application run with the scanned path; an
  override with a space and a `;` is one argument and `{name}` its folder; a branch opens its
  repository's checkout; a file opens with the file command and its line; `..`, a root, a symlink
  out, a missing file and a directory are refused and nothing runs; unusable entries are refused
  and store nothing; a program that will not start names the command; *Try it* runs unsaved; the
  shipped list with #501's template until a list is saved, and a gone default; #501's write over
  the list; a tilde root under `$HOME`), `knobas_core::checkout::open`'s tests, and the
  `knobas_app::checkout::open` mirror pins.
