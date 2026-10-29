- **One migration and one IPC read — issue #496 (2026-09-07): the URL a paste names.**

  A Jira, Confluence, Gitea or Uptime Kuma link pasted into the launcher opens the entity it
  names (spec #491, v1.5 stream 2, stories 9–11 and 15–17). Two frozen surfaces, both of them
  additive, and the stream map in #491 names both in advance: *"one index on the mirror's
  `web_url`; one IPC read (`resolve_url`)"*. **Ratified by the orchestrator under this issue's
  own third criterion, which asks for the entry by name and for the normalisation rule to be
  stated here. Björn keeps the gate for frozen contracts and this entry is flagged for his
  review — ratified in his absence by the deputy's ruling of 2026-09-08 on #496
  (`docs/archive/2026-09-v1-5-unattended-rulings.md`), which exercised that gate on both
  surfaces and found the entry sufficient as written.**

  **`crates/knobas-db/migrations/**` — one migration, `0023_the_url_a_paste_names.sql`, and it
  creates one index.** No table, no column, no constraint, no view:

  ```sql
  create index item_web_url_norm_idx on sync.item ((<the normalised web_url>))
   where web_url is not null;
  ```

  `sync.item.web_url` has been there since `0002` and **no query has ever read it** — it is what
  *Open in browser* hands to the OS. Reading it by equality per keystroke is a sequential scan of
  the corpus, which is the one thing the launcher's 100 ms budget cannot afford, so the column
  gets the index this read needs and nothing else. Partial, because `web_url` is null for every
  note, every context and every kind whose adapter reported no page (interfaces §8 P5) — rows a
  paste can never name. **Not unique**: nothing says two mirrored items cannot report one address,
  and a unique index would turn that into a failed sync rather than an ambiguous paste.

  **The normalisation rule, which this section is asked to state.** Matching is **exact on the
  stored `web_url` after the same normalisation is applied to both sides** — the stored column and
  the pasted parameter — and it is four rules and no fifth:

  1. the **fragment** is dropped (`#comment-42` addresses a place inside a page, not another page);
  2. the **path's trailing slash** is dropped (`…/PAY-231/` and `…/PAY-231` are one ticket);
  3. **scheme and host are down-cased**, and nothing after the authority is — a Jira key is
     upper-case (`/browse/PAY-231`) and a Gitea path is case-sensitive, so folding the path would
     make two different pages one;
  4. the **query is kept verbatim**. This is the half that is deliberately *not* doing something:
     a Confluence `viewpage.action?pageId=42` link's identity *is* its query, and dropping it
     would resolve every page of an instance to whichever one was indexed first.

  No percent-decoding, no default-port removal, no query reordering, no `www.` folding: each is a
  claim about what two URLs mean, and none is asked for. Two sources on two hosts never confuse
  each other because the stored URL carries the source's own base — the rule compares whole URLs
  and never keys (story 17).

  The rule has **one implementation**: the SQL that `knobas_core::web_url_normalized!` expands,
  documented in `crates/knobas-core/src/web_url.rs`. It is applied to the pasted URL by binding it
  as a parameter and putting it through the same macro, so the two sides cannot disagree — the
  reasoning `ancestor_path_read!` records, and `crates/knobas-core/tests/web_url.rs` is written
  against the database for it exactly as `tests/ancestor_path.rs` is. The migration carries a
  **third** copy, because an index definition cannot expand a Rust macro; that copy is load-bearing
  for *speed* and not for correctness — a drifted copy answers identically and merely stops using
  the index — and two tests hold it: `the_migration_carries_the_macros_expression_verbatim`
  compares the characters, and `the_resolvers_statement_reaches_the_expression_index` asks the
  planner, which is what `the_view_still_reaches_the_fts_index` does for the launcher's GIN index.

  **The failure direction is absence.** A stored value that is not an absolute URL normalises to
  SQL `null`, `null` equals nothing, and such a row is unreachable by paste rather than reachable
  by accident. This is ADR-0007's first requirement *borrowed*, and it is worth saying which:
  `web_url` is a plain column and not a payload read, so that ADR does not govern this and no
  interim exception is owed — the hazard is simply the same shape, and the discipline is the one
  that answers it.

  **The IPC schema — one command, one DTO, no argument change anywhere else:**

  * `resolve_url(url: String) -> Option<UrlMatch>`, on the **entity** command module, appended to
    both barrels (`crates/knobas-app/src/lib.rs`'s handler list, `app/src/lib/ipc/entity.ts`
    through `index.ts`'s existing `export * from "./entity"`). One read on the local mirror, and
    **never a fetch from a source** (story 16, ADR-0007's direction): a paste while the tunnel is
    down is a miss, not a hang. It reads `sync.item` rather than `sync.live_item`, the way
    `get_entity`'s `DETAIL` does and for the same two reasons — a withdrawn entity resolves so its
    detail can say it is gone (story 15), and a disabled source's items still open.
  * `UrlMatch { entity_id, kind }` — two keys. Deliberately not an `EntityRow`: a paste answers
    *which entity is this link*, and the frontend then opens that entity through the read that
    already exists. `kind` rides along because an entity id names a namespace and a key, not a
    kind, and an address is built from both.
  * A value that is not an absolute `http`/`https` URL is refused with `invalid` rather than
    answered as a miss. A miss is *the mirror does not hold this link* and is the only outcome
    that earns *Open in browser*, which allows those two schemes and no others
    (`app/src/lib/shell/open-external.ts`).

  **What is not touched.** No adapter: `crates/knobas-source/src/**` is unchanged, no `WriteOp`
  grows, no descriptor gains a slot. Per-adapter URL parsers are **not** built (spec #491: if soak
  shows misses they are the next step, as a descriptor slot with its own entry). Nothing in
  `crates/knobas-http/**`; `crates/knobas-app/src/{error,profile}.rs` untouched; the `commands/` +
  `ipc/` module layout unchanged — `resolve_url` lands in the existing `commands/entity.rs` and
  `ipc/entity.ts`; no event; the keychain envelope unchanged at version 2. The launcher's
  **grammar** gains no URL branch: the frontend recognises an absolute URL before the query is
  sent (`app/src/lib/launcher/url.ts`), which is the same latitude `rows.ts`' `modeOf` already
  takes for the empty box, and ruling P2 is otherwise untouched.

  Pinned by: `knobas-core`'s six tests in `tests/web_url.rs` for the rule and its miss direction;
  eleven in `crates/knobas-app/tests/url_resolve.rs` for the four sources, the three spellings,
  the two misses, the tombstone, two Jira instances, the refusal, the index's shape and the plan
  that reaches it; `entity_mirror.rs`'s `the_url_match_shape_matches_its_typescript_mirror`;
  `wiring.rs`'s `every_command_is_in_the_handler_list`; and, on the frontend, five in
  `launcher/url.test.ts` for what is a paste and what is still a query, ten in
  `session.test.svelte.ts` for which backend read a keystroke reaches, what a close leaves behind
  and how the answers are sequenced in both directions, and five in `Launcher.test.svelte.ts` for
  the two outcomes on screen.

