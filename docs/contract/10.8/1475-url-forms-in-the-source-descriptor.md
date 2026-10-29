- **`crates/knobas-source/src/**` and the IPC schema — issue #1475 (2026-10-21): `SourceDescriptor`
  gains `url_forms`, and the paste resolver reads a pasted address as each declared form.** Spec
  #491's own next step (*"Per-adapter URL parsers are not built; if soak shows misses they are the
  next step, as a descriptor slot with its own entry"*); the 2026-10-20 journey audit is the soak.
  No migration, no event, no command added, removed or renamed, no argument changed, no `WriteOp`,
  nothing under `crates/knobas-http/**`, no new module in `commands/` or `ipc/`. The core crate's
  web-URL normalisation (`knobas_core::web_url`) and migration `0023`'s index are unchanged.
  Reserved for Björn, ruled by the implementer on Opus: the sign-off; the rulings are in
  `docs/archive/2026-10-unattended-rulings.md` under `## #1475`.

  **`crates/knobas-source/src/**` — additive.** A new module `url_forms` with one declaration type,
  exported to the generated `sources.ts` beside `SourceDescriptor`:

  ```rust
  pub struct UrlForm { pub kind: String, pub template: String, pub fields: Vec<PayloadPath> }
  ```

  and `SourceDescriptor` gains, last, `#[serde(default)] pub url_forms: Vec<UrlForm>`. `template` is
  the address after the source's base URL: literal text starting with `/`, `{}` for one value
  (ending at the next `/`, `?`, `&` or `#`, form-decoded), at most one `{*}`, last, for the rest of
  the address compared as written, or `{*}` alone for the whole of it. `fields[i]` is where the
  item's payload holds the `i`-th placeholder's value. After the template a pasted address may go
  on only with more query or a trailing `/`. `UrlForm::read(url)` answers every reading of an
  address as the form (`Reading { base, kind, fields: Vec<FieldValue { path, value }> }`, one per
  base the template fits after); `url_forms::check` holds a descriptor to the rules: a declared
  kind, the leading `/`, at least one placeholder, one field per placeholder, `{*}` last, no two
  placeholders side by side, no empty path. The contract battery runs it as **clause 12**. Every
  adapter in the tree declares none but Confluence.

  **External adapters.** The descriptor crosses the WIT boundary as its serde JSON (`descriptor:
  func() -> json`), so `adapter.wit` and `knobas:adapter@1.3.0` are unchanged: a component that
  sends no `url_forms` decodes as one declaring none, and one that sends them is read by this host
  and ignored by an older one.

  **The IPC schema — additive.** Every `SourceDescriptor` on the wire (`list_adapters`, and an
  `InstalledAdapter`'s `template`) carries `url_forms: Array<UrlForm>`; `app/src/lib/ipc/generated/sources.ts` gains `UrlForm` and the
  field. Nothing in the shell reads it. `resolve_url` keeps its name, its one argument and its
  answer (`UrlMatch | null`).

  **`RESOLVE_URL`** (`commands/entity/lookups.rs`) takes a second parameter, `$2::jsonb`: the
  pasted address read as every form every registered adapter declares (`form_readings`), each
  `{adapter_kind, base, kind, fields: [{path, value}]}`. One statement, two arms under `union all`:
  the stored address exactly as before (still the expression index's `Index Cond`), then each
  reading joined to `knobas.source_config` on its adapter kind with
  `web_url_normalized!(s.base_url) = web_url_normalized!(r.base)` and to that source's `sync.item`
  rows of the reading's kind whose `payload #>> path` equals every value; a reading with no fields
  is skipped. `order by via_form, entity_id limit 1`: the stored address wins over a form. The three
  commit-hint reads in `sources/crud.rs` bind `[]` and resolve the stored address alone, as before.
  Still one read on the local mirror, never a fetch (spec #491 story 16).

  **The Confluence adapter** (not frozen; recorded because it fills the slot). Three forms of kind
  `page`: `/pages/viewpage.action?pageId={}` at `id`, `/display/{}/{}` at `space.key` and `title`,
  and `{*}` at `_links.tinyui`, the tiny link Confluence keeps on every content record. No cursor
  change: the payload already holds all four values.

  **The launcher** says a miss as *"Not in the mirror as this address; it may be stored under
  another form."*; *Open in browser* stays. The fake's `resolve_url` answers the SEPA page's
  `pageId=` and `/display/` forms under `docs.ts`'s host.

  Witnessed by `knobas-source`'s `url_forms` unit tests and
  `url_forms_round_trip_and_a_descriptor_without_them_still_decodes` (a descriptor JSON with no
  `url_forms` key decodes as one declaring none), the Confluence crate's
  `descriptor::tests::a_page_is_named_by_its_id_its_space_and_title_and_its_tiny_link` and
  `another_pages_address_does_not_name_this_one`, `knobas-app`'s `tests/it/url_resolve.rs`
  (`a_confluence_page_opens_from_each_form_confluence_serves_it_under`,
  `a_form_misses_another_page_and_another_instance`, `the_stored_address_wins_over_a_form`,
  `a_reading_with_no_values_names_nothing`,
  `the_resolvers_statement_reaches_the_expression_index`), and, against the seeded Confluence Data
  Center, `tests/confluence_live.rs`'s
  `a_pasted_address_opens_its_page_in_every_form_confluence_serves`, which fetches each form from
  the server before it pastes it.
