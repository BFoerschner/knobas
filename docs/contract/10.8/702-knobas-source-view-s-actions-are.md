- **`crates/knobas-source/src/**` — issue #702 (2026-10-13): a view's actions are a toolbar, and
  each action's dialog is declared by the source.** The tracer of spec #701 (*The view gets a
  toolbar, and actions get declared forms*), whose delegation (Björn, 2026-10-13) signs off
  `SourceView.actions` and `ViewAction.fields` by name. No `WriteOp` variant is added; the two
  actions that exist migrate. ADR-0020 rule 4 carries the matching amendment.

  1. `SourceView { sections, actions: Vec<ViewAction> }`, `actions` `#[serde(default)]` so a view
     sent without it decodes. The entity's actions, drawn as a **toolbar** between the detail's
     header and the tabs, on every tab; empty draws none. A section read (`view(entity,
     Some(section))`) answers the section alone and leaves `actions` empty: the toolbar comes with
     the whole view. `Block::Actions` stays in the vocabulary for an action that belongs to one
     section, and **an action is in one place or the other**; the shell drops from a block any
     action the toolbar already draws (same `write_op` and `label`). `SourceView::new(sections)` is a view
     with no toolbar.
  2. `ViewAction { write_op: String, label: String, fields: Vec<ActionField>, confirm:
     Option<String>, primary: bool, danger: bool, disabled: Option<String>, with_settings: bool }`
     — `fields`, `primary`, `danger`, `disabled` and `with_settings` `#[serde(default)]`.
     **`options: Vec<String>` is removed**; an op's choices are now a `choice` field. `primary`
     draws the button filled; `danger` draws it in the error tone and always opens the dialog;
     `disabled` is the source's reason the action cannot be taken right now, drawn as a disabled
     button with the reason as its tooltip and accessible description (*no longer absent*: an
     adapter now draws an action it does not offer, saying why, where it used to leave it out);
     `with_settings` makes the dialog offer the entity's own settings — the view's
     `Block::Settings` of the same entity — to change for this one write, sending only what changed
     (a run's parameter overrides, #669). `ViewAction::new(write_op, label)` is the plain action
     (no fields, no sentence, no tone), for struct-update syntax.
  3. `ActionField { name: String, label: String, kind: FieldKind, value: Option<String>, required:
     bool, placeholder: Option<String> }` — `value`, `required`, `placeholder` `#[serde(default)]`;
     `ActionField::new(name, label, kind)`. `name` is the key the reader's answer is handed on
     under; `value` the start (a text's prefill, a choice's preselected option — `None` the first —
     a flag's `"true"`); `required` refuses a blank text, a choice with nothing picked and an
     unchecked flag.
  4. `FieldKind`, internally tagged on `type` with snake_case words: `text { suggestions:
     Vec<String> }` (one line, suggestions offered and none picked; `suggestions`
     `#[serde(default)]`), `long_text` (Markdown, several lines, drawn as a plain text area and
     never rendered, ADR-0020 rule 5), `choice { options: Vec<ChoiceOption> }`, `flag`. `ChoiceOption
     { value: String, label: String }`: what the op takes and what the reader sees.
     The wire answers are text: every field's value goes to the shell's payload table as a string
     by `name`, a flag as `"true"` or `"false"`.

  Sized for the four tickets that wait on it (#705–#708): a comment (`long_text`), a review
  (`choice` verdict + `long_text`), an edit (`text` + `long_text` prefilled by `value`), close and
  delete branch (`danger` + `confirm`), merge (`choice` style + `flag` delete branch), update branch
  (`choice`), new branch (`text` name + `choice` from — a `select` past four options), open pull
  request (`text` title + `long_text` body + `choice` base).

  **The adapters.** Gitea's pull request draws *Merge…* in its toolbar, `primary`, one required
  `choice` field `style` (`merge` *Merge commit*, `rebase` *Rebase*, `squash` *Squash*, the SPI's
  `MergeStyle` words, `merge` first), the sentence *Merge {head} into {base}?*, and is **always
  drawn**: disabled with *Already merged*, *Closed: reopen it to merge*, *Not mergeable into
  {base}: a conflict, a work-in-progress title or a check still running* (Gitea's `mergeable:
  false` does not say which), or *Gitea has not said yet whether this can be merged* (no flag);
  enabled only when open, not merged and `mergeable: true`. The overview no longer carries an
  actions block. *Superseded in part by #705's entry below (2026-10-13, the #701 ruling on
  disabled and absent): a merged or closed pull request draws no *Merge…* at all, so *Already
  merged* and *Closed: reopen it to merge* are gone; the two `mergeable` reasons stay.* TeamCity's build configuration draws *Run…* in its toolbar, `primary`,
  `with_settings`, one `text` field `branch` whose suggestions are the branches its recent builds
  ran on (never the default branch by name, #669), placeholder *default branch*, always enabled.

  **The IPC schema:** no command, event, migration or barrel moves. `app/src/lib/ipc/entity.ts`
  mirrors the shapes: `SourceView.actions`, `ViewAction` with the new fields and without `options`,
  and the new `ActionField`, `FieldKind` (a union on `type`) and `ChoiceOption`. `entity_view`
  answers the view unchanged.

  **The shell.** `SourceView.svelte` draws the toolbar (`role="toolbar"`, *Actions*) above the tab
  strip, outside the tab panel, so it stays on every tab. `ActionButtons.svelte` (was
  `ActionsBlock.svelte`) draws a row of buttons — toolbar or block — and `ActionDialog.svelte` the
  declared dialog, generic over the field kinds: a text an input with a `datalist`, a long text a
  text area (Cmd/Ctrl+Enter sends), a choice a segmented radio group or a `select` past four
  options, a flag a checkbox; a blank required field stops the write, is marked `aria-invalid`
  with *Required* beside it, and the first is focused. The dialog opens on its first field (a
  choice on its picked option), or with no fields on its write, or on *Cancel* for a danger; Enter
  sends from any field but a text area, Escape cancels. A disabled action keeps focus
  (`aria-disabled`, not `disabled`) so its reason is reachable by keyboard. `detail/view/
  actions.ts` keeps **one table**, op id → `(entity, values, settings) => WriteOpPayload | null`;
  `FORMS` and `actionForm` are gone, and `initialValues`, `missingFields` and `opensDialog` are
  added. An op with no entry is still not drawn. `Detail` hands the values on as before.

  Pinned by: `knobas_source`'s `view::tests::every_block_round_trips` (every field kind, an action
  with every flag, the toolbar) and `the_wire_shape_is_a_union_on_type` (the JSON of an action and
  its fields, and what decodes when the defaulted parts are left out); `knobas-source-gitea`'s
  `tests/it/view_pr.rs` (`an_open_mergeable_pull_request_offers_a_merge_in_its_toolbar`,
  `a_pull_request_gitea_would_not_merge_offers_a_disabled_merge_saying_why`,
  `a_section_read_carries_no_toolbar`); `knobas-source-teamcity`'s `tests/it/view.rs`
  (`the_overview_offers_a_run_suggesting_its_recent_builds_branches`,
  `a_configuration_with_no_builds_still_offers_a_run`); the frontend's `actions.test.ts`,
  `SourceView.test.svelte.ts`, `Detail.action.test.svelte.ts` and
  `Detail.settings.test.svelte.ts`. Witnessed against the real servers by `just gitea-live`'s
  `a_mergeable_pull_request_is_merged_from_its_view_and_the_next_sync_reads_it_merged` (the
  toolbar's *Merge…* enabled once Gitea has computed it, merged through the op, then drawn
  disabled with *Already merged*) and `just teamcity-live-seeded`'s
  `a_run_with_an_override_is_built_on_its_branch_and_synced_next` (the toolbar's *Run…* suggests
  1187's branch and offers its overrides).

