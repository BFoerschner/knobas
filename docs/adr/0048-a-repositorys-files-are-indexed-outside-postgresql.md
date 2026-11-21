---
status: accepted
---

# A repository's files are indexed outside PostgreSQL

Björn, on PR #2479: "For repositories, not just search pullrequest bodies but also search like the filescontent
inside." His case was a renamed repository he could not find, whose old name only its source files still held. The
mirror stored every branch's file list (ADR-0041, ADR-0042) and no file's text. Decided 2026-11-20 (Fable, xhigh,
under Björn's standing instruction; built by Opus; `docs/decisions/2026-11-20-everything-finds-a-repository-by-its-files.md` has the
measurements): **the names and the text of every mirrored repository's files, at its default branch's head, are kept
in a tantivy index in the profile, and PostgreSQL keeps what that index stands at.**

- **A third reader beside PostgreSQL's words and the picker.** ADR-0045 matches names in memory and bodies by
  PostgreSQL. File text is 10 to 25 times the mirror's bodies, and its words are identifiers typed by their starts.
  `knobas-files` holds the index. No other crate names a tantivy type. tantivy is pinned at 0.27.0, MIT, with `mmap`
  and `lz4-compression` and no C code.
- **One document a file**: the repository's number, the path, the path's tokens, the text's tokens, the text. The
  tokens are `knobas_core::code_text`'s, which also reads what was typed: an identifier's parts, each joined to the
  identifier's end, so a word is found from the start of any part.
- **A typed word is read as a word's start from four letters, whole at three, and whole where more than 4,096 word
  lists would be read for it.** The bound is on work, and what it changes is said.
- **The files answer with their repository.** Counted by repository, one row each, in the repositories' kind, under
  *Everything* only. One named by the words in its paths stands as a name of rank 0.5 does (`group::with_names`), one
  that only says them stands last. PostgreSQL's answer stands (ADR-0045).
- **PostgreSQL keeps the state**: `sync.code_repo` (the commit each repository is read at, its counts) and
  `sync.code_file` (each file's blob). A pass after each sync run reads what the stored tree says changed, from a
  checkout's object store where one holds the blob (ADR-0043), else from the forge, by archive or by file.
- **Two stores, one rule.** Tantivy commits first, PostgreSQL records after, and a write is a replace. Bring-up
  compares the files each holds of every repository and reads again what differs. A folder that cannot be opened is
  made again and filled again.
- **Not carried by a backup**, as nothing of `sync` is. The mirror re-syncs and the index is read again.

## Rejected

- **PostgreSQL, the same tokens under GIN.** One store. A broad word still being typed is where it loses: the lists of
  every word under the prefix are joined, then each row is fetched for its repository. The table is larger than the
  text it indexes, because a row's token vector is stored beside the index built from it.
- **`pg_trgm`.** Not built. Its index holds every three letters of every file, and each candidate is checked by
  reading the file.
- **Only cloned repositories.** The repository nobody can find is not cloned.
- **Every branch.** A feature branch repeats its base.
- **A hidden shallow clone a repository.** git runs with the person's credentials (ADR-0023), and a fetch nobody
  asked for can prompt or fail.
- **Remembering a repository's past names.** Björn ruled it out.

## Consequences

- `Searcher::with_files` is the one switch. A searcher without it, or with an index that holds nothing, answers as
  before, which is every earlier test and the way back.
- A profile has a folder knobas can rebuild and PostgreSQL does not know. Removing the folder costs one fill.
- The forge is asked for one archive a repository on the first fill, one at a time, inside a run's bound.
- A file's text is stored decoded. *Files* still reads bytes from the clone or the forge.
- tantivy is 35 crates new to the lockfile and about 3.7 MB of the binary by the spike. As built, the windowless
  backend in release on the Mac grew by 5.1 MB stripped (72.8 to 77.9 MB), for everything this build added. Its
  on-disk format is its own: a version that changes it gets a new folder name and a fill.
- The witnesses: `code_text`'s unit tests, `knobas-files/tests/it` (the read, the bound, the writes),
  `knobas-sync/tests/it/code_files.rs` (the pass, a crash between the two stores), `knobas-search/tests/it/files.rs`
  (the ranking table), the live clause `the_archive_hands_over_every_blob_of_the_tree` against Gitea, and
  `files::the_files_read_alone` over a corpus.
