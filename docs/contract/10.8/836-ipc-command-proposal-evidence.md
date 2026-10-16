- **The IPC command schema and the Rust handler barrel — issue #836 (2026-10-16): one new read
  command, `proposal_evidence`, what was said for and against one proposal.** Spec #808, story
  72 and R-6.4; milestone TF3. The spec's Impl "Signals, conversation and proposals" grows a
  proposal's one-line reason into a reason, evidence with match spans, supporting items and
  counter-evidence, with the tray's ranking unchanged; this is that payload.

  **A command, not a field on `SuggestionEntry`.** `room_suggestions` answers up to 500 rows to the
  Proposed links view and is read on every room's tray; an excerpt costs a text scan (and for
  shared wording a `ts_headline`) per proposal that only the open proposal needs. So the read is
  per proposal, made when it opens. In the **existing** `commands/entity.rs`, beside the four
  suggestion commands:

  ```rust
  #[tauri::command] pub async fn proposal_evidence(.., link_id: String)
      -> Result<knobas_core::evidence::ProposalEvidence, IpcError>;
  ```

  `ProposalEvidence { link_id: Uuid, reason: Option<String>, excerpts: Vec<Excerpt>,
  support: Vec<String>, against: Vec<String> }`, `Excerpt { entity_id: String, parts: Vec<Part> }`,
  `Part { text: String, marked: bool }`. The match spans are parts rather than offsets, so the
  shell never has to agree with Rust on what a character is. `against` is never empty
  ("Nothing found against it."). Errors as `accept_suggestion`'s: `invalid` for an id that is not
  a UUID, `not_found` for one no link carries, `internal` for a query. Read for a link in any
  state. Mirrored in `app/src/lib/ipc/entity.ts` as `EvidencePart`, `EvidenceExcerpt`,
  `ProposalEvidence` and `proposalEvidence`, and in the fake IPC.

  **What it reads.** `knobas_core::evidence::proposal_evidence`, a module of its own beside
  `suggest` (so the responsiveness probe's after-pass snapshot, keyed on `suggest.rs`, survives).
  Per rule: the four key rules quote the item's title and body around the ticket's key, marked as a
  whole word (the rules' `\m…\M`), list items the same rule found and the reader accepted for the
  same ticket, and name against it the other issue keys the text holds of a project some live
  ticket belongs to (so `UTF-8` is no key); `similar_text` quotes each
  end around the lexemes both `fts` share (the common words of #776 left out) through
  `ts_headline('english', …)`, lists the shared words, and says wording alone often matches
  unrelated items; `monitor_url_host` quotes the monitor's text around the host its reason names
  and counts other assets proposed for the same monitor. Every rule: proposals the same rule made
  at either end that were dismissed, and an end its source withdrew. No payload is read.

  **What is not touched.** No migration, no event, no `WriteOp`, nothing under
  `crates/knobas-source/src/**`, no new module in either barrel or in `commands/`. The detector,
  its stored reason, `suggest::proposals` and `room_suggestions` are unchanged, so the ranking is
  too. The handler list grows by one line at its foot.

  Pinned by `knobas-core`'s `tests/it/proposal_evidence.rs` (a key in a commit message marked as a
  whole word with other keys against it, accepted siblings as support, a dismissed sibling against,
  shared wording quoted from both ends, an unknown id), the module's own tests (word boundaries,
  windows, multibyte cuts, the headline split), and `knobas-app`'s `suggestions_ipc.rs`
  (`every_proposal_over_the_demo_corpus_has_evidence_with_the_match_marked`, which also holds the
  tray's page unchanged across the reads, and
  `evidence_for_a_bad_id_is_invalid_and_for_an_unknown_one_not_found`).
