- **`crates/knobas-source/src/**` — issue #665 (2026-10-12): `ViewRow` gains an optional commit
  hint, resolved by the app.** Spec #660 story 23: a build's *Changes* lists the commits it
  built, each opening the mirrored commit when knobas has it. TeamCity knows a change's sha and
  its VCS root's URL and cannot see a Gitea mirror, so the ticket puts the resolution in the
  app, and names this field as its §10.8 entry (signed off by Björn's 2026-10-12 delegation on
  #660, as the vocabulary itself was).

  ```rust
  pub struct ViewRow {
      /* #661's six fields, unchanged */
      #[serde(default, skip_serializing_if = "Option::is_none")]
      pub commit: Option<CommitHint>,
  }
  pub struct CommitHint { pub repo_url: String, pub sha: String }   // in knobas_source::view
  ```

  **Additive and invisible to the shell.** `#[serde(default)]`, so a row sent without it
  decodes; skipped when `None`, so a row without one serializes exactly as #661 froze it
  (`view::tests::the_wire_shape_is_a_union_on_type` pins both). `sources::crud::view` resolves
  every hint before the rule-7 check and clears it either way, so the field never reaches the
  shell; the TypeScript `ViewRow` is unchanged and its doc comment says why.

  **The resolution.** `crud::commit_page` composes `<repo_url minus .git and trailing
  slash>/commit/<sha>` -- Gitea's (and GitHub's) commit page -- and runs the paste resolver's own
  statement, `commands::entity::RESOLVE_URL`, on it; `commands::entity::is_web_url` becomes
  `pub(crate)` so the composition refuses what the resolver refuses. A hit sets `entity_id`; an
  `ssh://` or scp-like remote, a non-hex sha, or a page the mirror does not hold leaves the row
  plain text (rule 7's failure direction). No new statement, command, event or migration.
  Pinned by `knobas-app`'s `entity_view::a_commit_named_by_repository_and_sha_opens_the_mirrored_commit`.

  **What does not move.** This entry adds no block, tone or `Block` field. TeamCity's build view
  uses the vocabulary as it stands: its *Tests* tab is an entity row per failure followed by a text block
  of its details; its *Log* tab is one log block; its *Parameters* is a settings block whose
  `write_op` is empty, because nothing changes a build's parameters. The shell's `LogBlock` now
  opens scrolled to its end; that is drawing, not vocabulary.

  **Witness gap, said once.** The seeded failed build is its configuration's first, and TeamCity
  lists no change for a first build, so `just teamcity-live-seeded` witnesses *Changes* as an
  empty list only; the change shape (`vcsRootInstance(properties(...))`) was read off the same
  server's `/app/rest/changes` on 2026-10-12. The seeded VCS roots point at `http://gitea:3000`
  (the compose network's name for Gitea) while a knobas Gitea source points at `127.0.0.1:3000`,
  so in that environment a change would stay plain text: the resolver matches the URL as
  written and does not guess that two hosts are one.

  *Closed by #683 (2026-10-13).* The roots still say `http://gitea:3000`. The resolver learns
  nothing about hosts; a source says it, in its own config (the entry below). The seed's change
  witness gives one build a change, and a build's overview gains a *Revision* entity row carrying
  the same commit hint (no vocabulary change). `teamcity_seeded_live.rs`'s
  `a_builds_change_and_its_revision_open_the_mirrored_gitea_commit` syncs both products in one
  app and opens both rows.

