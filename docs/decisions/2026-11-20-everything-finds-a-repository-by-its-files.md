# 2026-11-20: Everything finds a repository by its files, and a row says where it matched

**Asked (Björn, on PR #2479, through the orchestrator):** "another feature I'd like to land in ux25 is having proper
'everything' search for all the sources. Like for tickets, I'd like to be able to search their body fulltext just like
the docs. For repositories, not just search pullrequest bodies but also search like the filescontent inside. Ofcourse,
only if it's like in everything search. Maybe even add a setting on what you want inside the everything search, for
example you might not want to match filenames etc. I think the ranking is getting important here." His example: a
renamed repository he could not find, because he still had the old name in his head and only the source files still
had it. Asked whether knobas should remember old names: "no it shouldn't. that was an example. Its old name was still
in the classnames etc. but you shouldn't remember it yourself." His standing rule: directness, performance and
responsiveness over data scarcity. Decided by Fable under his delegation on 2026-11-20, built by Opus from the
decision list.

## What was already there, and what was missing

A ticket's description and comments have been in the index since #2455, and *Everything* matched them. What was
missing was any sign of it. A row drew its title and nothing else, where the Docs search draws a snippet under every
hit. The words' places were in the preview pane, for the one row selected.

A repository was found by its name and its description. Nothing of its files was in the index.

## Decided

1. **A row found by its text says where.** One line under its title: the place (*Description*, *Comment by Jonas
   Becker*, a page's section, *Log line 212*, a file's name and line), then the passage with the words marked. A row
   its name explains stays one line. Enter opens it at the match: a file at its line, a ticket or page with the find
   bar on the passage.
2. **Under *Everything* a repository is also found by its files**: their names and their text, on the default
   branch's head. It answers with the repository's row, never with a row a file. Not under *Titles*, not under
   another kind's letter.
3. **Every mirrored repository's files are read, not only the cloned ones'.** A file's bytes come from a checkout on
   this machine where its object store holds the blob. Else from the forge: one archive a repository, or single reads
   for 24 files and fewer. After each sync, behind the app, only what the stored tree says changed.
4. **Which files.** Text up to 512 KiB. Not binaries, lock files, minified or generated files, vendored folders, or
   what `.gitattributes` marks generated or vendored. Those are still found by name.
5. **A file's words are its identifiers' parts.** `ADProxyServer` is found by `adproxy`, `proxy`, `proxyserver` and
   `server`. So are `ad_proxy` and `ad-proxy`. A typed word of four letters and more is read as a word's start, of
   three as a whole word, and so is one that more than 4,096 word lists would be read for.
6. **The index is tantivy**, pinned at 0.27.0, in a folder of the profile. PostgreSQL keeps which commit each
   repository is read at and which blob each file. Tantivy commits first and PostgreSQL records after. Bring-up
   compares the two and reads again what differs. ADR-0048.
7. **Where a repository found by its files stands.** One whose files or folders are named by the words goes in as a
   name of rank 0.5 does: after the repositories named by them, above one whose description mentions them once. One
   whose files only say the words goes last. More files first, a favourite before another. Nothing PostgreSQL
   returned moves.
8. **The pane lists up to eight files** of the selected repository, each with its line and the lines around it, each
   opening the file there.
9. **Settings › Search says what *Everything* matches**: ticket text, pull request text, Docs text, build logs, file
   names, file contents. A box filters and removes nothing. *Keep at most* bounds the file text kept, 2 GB unless
   changed.
10. **No past name is stored.** A renamed repository is found because its code still holds the old name.

## What was weighed

- **PostgreSQL for the files.** The same words under GIN. A word few files hold costs the same. A broad word still
  being typed costs 84 to 255 ms on his PC against tantivy's 3 to 7, a cut-off count names repositories by the order
  their rows were written in, the planner picks a sequential scan of 1.2 s for a broad word under a limit unless told
  not to, and the table is 1,541 MB for 1,473 MB of text against 573 MB.
- **Trigrams.** Any substring, at many times the postings, each candidate checked by reading the file.
- **Cloned repositories only.** It answers for the repositories he already knows.
- **A row a file.** 500 files of one repository would be its ten rows.
- **File rows when the typing pauses.** Rows that land under a list being read, or a wait. The read is 1.5 ms a key.
- **A title above every body.** Story 26 stands. What a body hit lacked was a way to say it is one.
- **Remembering old names.** Björn: no.
- **Commit messages.** The mirror holds the newest commits only, and a rename's old name is in the files.

## What it costs

As built, on the Mac (M-series, niced neighbours under the machine queue):

- **The read a key.** An index of the Mac's own registry crates (1,168 repositories, 60,588 files, 519 MB of text)
  built in 35.0 s to 224 MB. 163 keys of 13 texts typed a letter at a time: 1.51 ms at the median, 5.60 at the 90th,
  11.82 at the worst (`files_corpus keys`).
- **The scale copy given files.** 62 repositories, 2,530 files and 10.6 MB of text by `files_fill` through the pass's
  own write path: 26 archives and 498 single reads, 3.1 to 3.4 s, a peak footprint of 57 MB, an index of 5.1 MB.
- **The backend's time a key** on that copy under *Everything*, the median of nine `search` calls over the windowless
  backend's HTTP, the same build with the index and with an empty one: a word only one repository's files hold
  (`settlebridge`, `dashmap`, `packetline`) 10 to 11 ms both ways; a broad word costs more, `sett` 45 to 63 ms, `conf`
  63 to 95, `config` 63 to 87, `test` 65 to 87, `payout` 55 to 77, `proxy` 32 to 45, `postmortem` 53 to 66. The rows
  with an evidence line and every other row are the same both ways (row 20).
- **The key probes** (`just keys-budget --real`, on that copy, the pane and the evidence lines drawn): a word only
  files hold 27 ms at 1x, 36 at 4x, 46 to 50 at 6x; a broad file word 36, 72 to 75, 70 to 87; a word only bodies hold
  43, 64 to 73, 88 to 90; at 1440x868 and 2560x1440 at 125 %, nothing moved.

On his PC (`files_corpus.exe`, a scratch folder in his Temp on NTFS, through WSL's interop, nothing installed):

- **The scale copy's files** (65 folders, 5,546 files, 10 MB of text): built in 1.2 s to 6 MB; 50 keys 0.41 ms at
  the median, 1.44 at the worst.
- **The WSL box's registry crates** (705 repositories, 32,454 files, 208 MB of text, unpacked by Windows' own
  `tar.exe`): the first build 163 s, the files just written and read cold under the virus scanner; the same build
  again 12.8 s. 89 MB on disk. 163 keys 0.51 ms at the median, 2.79 at the 90th, 6.19 at the worst.
- **Keys while another index fills beside them:** 0.50, 2.86 and 6.16 ms: the fill does not reach the read.
- **A write beside a reader that holds the segments mapped** (NTFS will not delete a mapped file): both exit 0, the
  index answers after, and the folder holds 19 files, no stray segments.
- **An index under a path of 319 characters:** built and read as any other.
- **Gitea's archive**, live (`the_archive_hands_over_every_blob_of_the_tree` against the WSL box's Gitea): the tree's
  2 files in 1 request.

The first fill against the live forge and a push's update on it are owed with the live script.

From the spike, on a corpus of 1,849 repositories, 194,653 files and 1,473 MB of text: the index 573 MB, built in
40 s on the Mac and 39 s on his PC. A key 1.5 ms at the median and 9.5 at the worst of 163 on his PC. A push of 100
files found by the next key 60 ms later.

- **The contract.** `SearchHit.files`, `Excerpt.file`, the commands `search_files`, `search_settings`,
  `set_search_everything`, `set_search_files_kept` and `reread_files`, the event `code:files`, `Source::read_archive`,
  migration `0101` (`sync.code_repo`, `sync.code_file`, `sync.code_gone`).
- **A new crate.** tantivy 0.27.0, MIT, with `mmap` and `lz4-compression` only: 35 crates new to the lockfile and
  about 3.7 MB of a Windows binary by the spike. As built, the windowless backend (`examples/headless_ipc`, the
  app's crates without its window) in release on the Mac is 105.2 MB against 98.1 MB at the head before, 77.9 against
  72.8 MB stripped: 5.1 MB for tantivy, the new crate, the pass and Gitea's archive reader together.

## Same answers

72 Rust tests and 49 front tests are new; two of the Rust ones are ignored by default, the read on a corpus of its
own and the live Gitea clause.

- **The ranking table** (D9's twenty rows) is `knobas-search/tests/it/files.rs`, one test a row or a pair of rows
  (1, 3 to 17), with row 2 in `everything.rs`, row 18 in the launcher's `cells.test.ts` (the key is its own reason)
  and row 19 in story 26's own tests, unchanged. Row 20 is `no_index_and_an_empty_one_answer_as_before`: a searcher
  with no index, and one with an empty index, answers byte for byte as the head before.
- **The boxes** (D13): `everything.rs` and `files.rs`: a cleared box leaves a ticket or a page to its names and the
  logs out, and the file boxes cleared, *Titles* and `#t` each say what the files answer.
- **The tokenizer and the reader**: `knobas_core::code_text` (what is read and what is left out, the query, the
  best line) and `knobas-files` (writes, reads, the bound, a removed file and repository found no more, a folder
  holding garbage opening empty, two writers refused).
- **The pass**: `knobas-sync`'s `code_files` against a fake forge: a second pass writes nothing, a push reads only
  what changed, more than 24 files are one archive, a checkout spares the forge, the bounds cut a repository and the
  next pass finishes it, the bound on kept text, a withdrawn repository, a crash between tantivy's commit and
  PostgreSQL's record, bring-up's reconcile, a failing forge, and the tree read a level at a time by its key.
- **The archive**: Gitea's `read_archive` against a tarball the test serves, and live against the WSL box's Gitea.

Mutants, each run against the tests of its step: the evidence line 6 of 6 caught, the boxes 4 of 4, the tokenizer
11 of 11, the index 10 of 10, the pass 5 of 5, the archive 3 of 3, the ranking 4 caught and 4 equivalent. The four
equivalent ones change the rank of a row only text found (row 7 is placed by its partition, not its rank), drop a
guard against a repository named twice (the merge already keys by id), set the loose flag where `in_files` is set
(the flag is read only where `in_files` is not), and drop the source filter on the file read (the repositories it
is handed are already the filter's).

## Left, with what it would take

- D16's checked-in fixture files and the mock source reading them: the mock source answers no tree today, so the
  pass does not run in `just demo`. The browser's fake holds the two repositories' files as a table
  (`FAKE_FILE_REPOS`) instead. With them, `testenv/seed-gitea.sh` pushing them and `testenv/live-everything.sh`
  finding `tidewater/ledger-gateway` on Gitea and Forgejo after a sync.
- Commit messages finding their repository: a corpus like the pipeline log's.
- Typos in files: tantivy's Levenshtein automaton under the same bound.
- Other branches.
- Mine first and recently opened first: a record of what was opened, and PostgreSQL's order re-cut.
- Jira's custom text fields: a list in the source's settings and one full sync.

## Reversing it

`commands::search` building its searcher without `with_files` is the launcher as it was, and the pass skipped leaves
the folder as it is. Removing the folder and the three tables removes every trace. The evidence line is the row
without its second span.
