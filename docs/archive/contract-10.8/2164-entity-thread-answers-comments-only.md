- **The IPC command schema (`entity_thread`'s answer) — issue #2164 (2026-10-26): the
  conversation is the ticket's comments, and nothing else.** Spec #2163 (R-5.1 and R-5.2 as
  amended there; `docs/decisions/2026-10-26-conversation-comments-only.md`). No migration, no
  grammar key, no `WriteOp`, no new command, no `commands/` or `ipc/` module. **Amends #819**:
  `entity_thread` answers `Thread { comments: Vec<ThreadComment>, me: Vec<String> }`;
  `ThreadComment`, the comment read (`conversation::COMMENTS`), *mentions me*, `me` and
  `visit_entity` stand as #819 wrote them. **Retires #929's change lines and #819's `changes`**:
  `Thread.changes`, `ThreadChange` and `ChangeKind` leave `knobas_app::conversation`, the IPC
  mirror export (`ipc_mirror.rs`), the generated `app/src/lib/ipc/generated/entity.ts` and the
  `app/src/lib/ipc/entity.ts` barrel; the backend no longer reads the stored changelog
  (`HISTORIES`), the write queue's sent `transition` writes (`MOVES`, `SKEW`) or the
  confirm-linked pull requests (`PULLS`) for the conversation, so `conversation::thread` takes no
  adapter registry. The Conversation tab loses *Include changes* and its change lines. What #929
  added outside the conversation stands until its own entry retires it: Gitea's `opened` and
  `merged_at` declarations (`KindPaths`, `field::OPENED`, `field::MERGED_AT`) keep their
  contract-battery weighing with no reader in the app, and the Jira adapter's `expand=changelog`
  is #2165's to drop. Pull-request links, the PR signals and the write queue are unchanged.
  Witnessed by `tests/it/conversation_ipc.rs`'s `the_thread_is_the_comments_and_nothing_else` (a
  ticket with a stored changelog, a sent status move and a confirm-linked merged pull request
  answers its one comment, and the answer's keys are `comments` and `me`).
