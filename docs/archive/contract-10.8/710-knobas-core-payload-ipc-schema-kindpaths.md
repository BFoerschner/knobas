- **`knobas_core::payload` and the IPC schema — issue #710 (2026-10-13): `KindPaths.commit`
  and `KindPaths.revision`, and `EntityDetail.builds`.** Spec #701 workflow 5: a pull request,
  a branch or a commit says whether its head commit's builds are green. Signed off by #701's
  delegation (Björn, 2026-10-13), which names "any new IPC for … checks"; #710 asks for this
  entry.

  **The declarations** (`knobas_core::payload`, re-exported by `knobas_source`). `KindPaths`
  gains two fields, both `#[serde(default)]` so an older peer's descriptor decodes as declaring
  neither, both in `field::ALL` (`field::COMMIT`, `field::REVISION`):

  1. `commit: Vec<PayloadPath>` — the commit a record of code stands at, by full object id.
     Gitea declares it on `pr` at `head.sha`, on `branch` at `commit.id` (the `/branches`
     record's spelling), on `commit` at `sha`. Weighed by the contract battery as text, like
     the other scalars.
  2. `revision: Vec<ListPath>` — the commits a build ran on, one per VCS root. TeamCity declares
     it on `build` at `revisions.revision` with entry `version`. Weighed by the battery as a list,
     under the rule `reviewers` already had (the array and its entries each in their own
     evidence slot).

  Two fields and not one because the two ends differ in shape and in meaning: a pull request
  stands at one commit (a scalar), a build ran on several (a list), and a single field read both
  ways would let a pull request's own branch and commit rows count as its checks. A kind
  declares at most one of the two; no built-in kind declares both. Mirrored in
  `app/src/lib/ipc/sources.ts`.

  **The read the sync makes.** TeamCity's `BUILD_FIELDS` (the builds walk) gains
  `revisions(revision(version))`: the sha and nothing else, since the declared path is the only
  reader. mockd serves `"revisions": {"revision": []}` on every build (the dataset gives no build
  a commit), and its two golden builds snapshots grow by that key. Cost, measured by `just
  teamcity-live-seeded` (`a_synced_build_carries_the_revisions_it_ran_on`) on the seeded
  2026.1.3 server, 2026-10-13: the four seeded builds' page grows from 1,735 to 2,063 bytes
  (about 80 bytes a build with one VCS root) and answers in about 3.4 ms either way, best of
  three. The seeded 1187 ran on `179eb1fb…`, the tip the seeded Gitea's
  `feature/PAY-231-sepa-retry` stands at.

  **The row shape.** `EntityDetail` gains `builds: Vec<EntityRow>`: the live rows whose kind's
  declared `revision` lists the sha the detail's own kind declares as its `commit`
  (`declared_string!` on the entity, read by the detail's own row statement; `declares_field!`
  and `declared_list!` on the builds in `DETAIL_BUILDS_AT`, ADR-0007: no payload key and no
  adapter named in either), ordered `err`, `warn`, `info`, `ok`, `muted`, none, then newest
  first, at most 20. The entity is read from `sync.item` (a withdrawn pull request still
  opens); the builds from `sync.live_item`. Empty for every kind that declares no `commit`,
  and for those the builds statement is not sent at all: it scans the live corpus. `EntityRow` is unchanged. Mirrored in
  `app/src/lib/ipc/entity.ts`.

  **What is not touched.** No command, event, migration, `WriteOp`, `Block` or `ViewRow` field.

  **The adapter's view (not a frozen surface, recorded for the shape it draws).** Gitea's
  commit, pull request and branch *Overview* draws the commit's combined status
  (`GET /repos/{o}/{r}/commits/{ref}/status`, the head's sha for a pull request, the branch's
  escaped name for a branch so the read goes beside the others) as an `Entities` block titled
  *Checks*, right after the facts: one row per status, `title` its context, `meta` its
  description, `chip` its state word, `tone` `err` for `failure` and `error`, `warn` for
  `warning`, `info` for `pending`, `ok` for `success`, `url` its target; failing first, Gitea's
  order within one tone. No status draws no block. A status read that fails does not fail the
  view (deputy ruling on #710, 2026-10-13): in the block's place goes an untitled `Entities`
  block of one row, `title` *Checks could not be read: <reason>*, `tone` `muted`, no chip, meta
  or url -- the shape a list's own muted note already has (#678), so nothing new crosses to the
  shell.

  **The shell.** `detail/BuildsPanel.svelte`, first of the detail's panels (above the checkout and
  the links): the builds as room lines (`EntityLine`, no source column), each opening its build,
  its header counting them and, in the error colour, how many failed. Absent when there are none.
  A panel and not a block inside *Overview*: the rows are the mirror's, so they draw whether the
  source's view answered or not and on every tab, and a source's section stays the source's
  (ADR-0020).

  Pinned by: `knobas-app`'s `tests/it/entity.rs`
  (`a_commits_detail_carries_the_builds_that_ran_on_it_failing_first`,
  `a_commit_no_build_ran_on_carries_none`), `entity_mirror.rs`'s
  `the_entity_detail_shape_matches_its_typescript_mirror` and `sources_mirror.rs`'s `KindPaths`
  keys; `knobas-core`'s `the_field_names_the_sql_macros_use_are_the_serialized_ones`;
  `knobas-source-teamcity`'s `a_build_ran_on_each_of_its_revisions` and
  `the_field_selectors_cover_everything_the_mapping_reads`; `knobas-source-gitea`'s
  `tests/it/view.rs`, `view_pr.rs` and `view_branch.rs` Checks tests (the failed read by
  each view's `…_cannot_be_read_still_draws_and_says_so`); the frontend's
  `BuildsPanel.test.svelte.ts` and `Detail.builds.test.svelte.ts`. Witnessed against the real
  servers by `just gitea-live`'s `a_commits_statuses_are_drawn_as_its_checks` (statuses posted
  on a commit of the test's own branch, read back through the commit's, the branch's and a pull
  request's views) and the contract battery against the container (the declared `commit` resolves
  on real records), and `just teamcity-live-seeded`'s
  `a_synced_build_carries_the_revisions_it_ran_on`.

