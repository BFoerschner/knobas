- **The IPC command schema and Gitea's view declarations — issues #984 and #985 (2026-10-17):
  `submit_write` takes an optional `link_to`, and Gitea's file section declares its own
  `commit_file` action.** Spec #808, milestone TF3; requirements R-8.6, R-8.8, R-8.11; stories 94
  and 97. No new command, no migration, no grammar key, no `WriteOp` or `WriteOp` field, no new DTO,
  no `KindPaths` field, no `commands/` or `ipc/` module.

  1. **`submit_write(payload, link_to: Option<String>)`** (#984). `link_to` is a ticket's entity id.
     Absent (or `null`), the command is what it was (`submit_in_background`). Present, it is
     refused before anything is queued unless the payload is a `CreateBranch` or a
     `CreatePullRequest` (`invalid`), `link_to` is an entity id (`invalid`) and the mirror holds it
     (`not_found`) (`sources::write_queue::arrival_linkable`). Then the write is queued and sent in
     the background as before, the source re-read (`send_in_background_then`, #779), and
     `sources::write_queue::link_on_arrival` draws a confirmed `related` link from what the write
     made to the ticket: the branch a sent `CreateBranch` named, looked up by its name in the
     repository, or the pull request open on a sent `CreatePullRequest`'s head (start work's own
     look-ups, `start_work::queue::branch_named` and `pull_request_on_head`). A write that is not
     sent on that send (waiting, held, refused, later discarded) links nothing; a link or proposal
     already on the pair is confirmed, never doubled (`protocol::draw_link`). Nothing is stored:
     a write that waits and lands on a later tick is not linked (the #862 *Link it to <job>*
     shape). Mirrored in `app/src/lib/ipc/entity.ts` (`submitWrite(payload, linkTo?)`, sent as
     `linkTo` only when given). Pinned by `knobas-app`'s `tests/it/link_on_arrival_ipc.rs`.

  2. **Gitea's `file:{path}` section carries a `Block::Actions` after its `Block::File`** (#985),
     with one `ViewAction` `commit_file` (*Commit change…*, no fields). It is disabled with the
     sentence the flush refuses a commit onto the branch with (`write::PushGuard::refusal`, shared
     by both: `<branch> is a protected branch that this account may not push to: commit to a new
     branch and open a pull request instead`) where `GET /repos/{o}/{r}/branches/{branch}` says
     `protected && !user_can_push`, #822's own rule; enabled otherwise. One more read per file
     section, of an endpoint the adapter already reads. A branch Gitea does not answer for declares
     no block: the flush stays the backstop. A directory declares nothing. The flush's own refusal
     reads `gitea: <owner>/<repo>: <that sentence>`. The new shell's commit dialog disables *Commit
     to <branch>* with the reason and opens on *Commit to a new branch*; the old shell has no
     payload for `commit_file`, so `offeredActions` draws no button for it. Pinned by
     `knobas-source-gitea`'s `tests/it/view_files.rs` (protected and not, a branch that does not
     answer, a directory) and witnessed against the real server by `just gitea-live`'s
     `a_file_is_committed_onto_a_branch_a_new_branch_held_when_it_moved_and_refused_when_protected`
     (declared enabled before the protection, disabled after with the flush's reason).
