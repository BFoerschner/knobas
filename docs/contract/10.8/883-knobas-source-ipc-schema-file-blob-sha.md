- **`crates/knobas-source/src/**` and the IPC schema — issue #883 (2026-10-16): `Block::File`
  carries the file's blob sha.** Spec #808, milestone TF3; #883 asks for this entry. One optional
  field on an existing block. No command, no new DTO, no migration, no `WriteOp`, no new
  commands/IPC module.

  1. `Block::File` gains `sha: Option<String>` (wire `"sha"`, `null` where the source has none),
     `#[serde(default)]`, so a file block without it reads as `None`. It is the file's blob sha
     at the ref drawn: the version *Edit* (#845) sends as `WriteOp::CommitFile`'s `base_sha`
     (#822), so a commit is made against the blob the reader was shown and holds on a mismatch
     as #822 already rules.

  **Gitea** reads the file's own `GET /repos/{o}/{r}/contents-ext/{path}?ref=` entry (no
  bytes, no history walk) before its `raw` read, on every file read, and draws the entry's `sha`.
  A Gitea without `contents-ext` (before 1.24) answers its route 404, and the file's
  `contents/{path}?ref=` entry is read instead. The entry comes first so that a push between the
  two reads leaves an older blob beside newer text, which #822 holds as moved, never the reverse.
  The same entry gives the size of a cut answer without a length (the #711 fallback). A failure
  of either read fails the section, as a failed `raw` read already did.

  Mirrored in `app/src/lib/ipc/entity.ts` (`ViewBlock`'s `file` arm, `sha: string | null`).
  Pinned by `knobas-source`'s `every_block_round_trips` (the file probe carries a sha) and
  `a_tree_and_a_file_have_the_wire_shape_the_shell_reads` (`"sha": null` on the wire, and a
  block without `sha` reads as `None`); `knobas-source-gitea`'s `tests/it/view_files.rs`
  (`a_file_is_read_raw_at_the_ref`, `a_branch_file_carries_the_blob_at_the_branch`,
  `a_cut_file_without_a_length_asks_gitea_its_size`,
  `a_gitea_without_contents_ext_draws_the_blob_from_contents`). Witnessed against the real server
  by `just gitea-live`'s `the_file_view_draws_the_blob_a_commit_is_made_against`: the drawn sha
  of the seeded `README.md` equals the contents API's, and a `CommitFile` made against it lands.
