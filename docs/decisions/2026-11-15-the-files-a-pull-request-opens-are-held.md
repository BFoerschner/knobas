# 2026-11-15: The files a pull request opens are held before it is opened

**Asked (Björn, on PR #2479, through the orchestrator):** carry out Fable's ruling of the forge-rounds pass
(`2026-11-15-first-opens-without-the-forge.md`, *Asked of Fable*, option 1): hold each open pull request's *Files*
view during the sync, so *Files changed* opens from the store the first time, checked against the forge behind under
the held line and replaced in place with nothing moving. Its conditions: only open pull requests whose head or merge
base moved; a request cap of its own a run, behind a person's reads, within the per-server quota; these rows evicted
first; reviewer threads replaced in place when they change; the held answer pinned byte-equal to the forge's own read on
data that would catch a difference. Measure the first open before and after at 1x, 4x and 6x, and the sync's cost on
the scale estate and on a pull request of 3,000 changed files. His rule: directness, performance and responsiveness over
data scarcity.

## What is held, and when

- **After each run of a source**, on the run's task, so in the background lane (every request stands aside for a
  person's on the same server and passes the per-server quota), between the pipelines' logs and the stored trees:
  `knobas_sync::held_sections`. The adapter names what it holds (`Source::held_shape`, an optional SPI method,
  `None` by default): Gitea names its pull requests' `files`, where a record says it is open, and where the record
  keeps the two commits the diff stands at (`merge_base`, `head.sha`) and its count of line comments
  (`review_comments`).
- **The page's own read.** The pass reads the section through `Source::view`, as a page does, and holds the adapter's
  answer in `sync.held_read` under the page's key (`view:files`, the recipe the app reads it through). A page's first
  open of *Files changed* finds it as it finds any read a person made before (ADR-0042): drawn at once, *As knobas read
  it 4 min ago. Checking Tidewater Git…*, and read behind. The app's own work on an answer (ADR-0020 rule 7) is done on
  every draw, as for every held read.
- **Only what moved.** A pull request is read again when its merge base or head is not what the held copy stands at,
  or its count of line comments moved (Fable, below), or nothing is held. Its note, `sync.held_section` (migration
  `0091`), keeps the commits and count it was read at, so a run with nothing moved reads no held answer: **0 requests and
  no write**, 27 to 46 ms of database time on the scale estate's 143 open pull requests. A pull request a person opened
  since the pass read it (the row's own diff stands at the record's commits) is noted with no request.
- **In order, capped.** Most recently updated first; at most 200 requests inside 45 s a run (`REQUESTS_PER_RUN`,
  `BUDGET`), about forty pull requests. A pull request the forge fails on as a whole (a 5xx, a refused credential, no
  answer, or a read that had the whole budget and did not end) stops the run and goes last in the runs after, so one
  that always fails holds up no other; one the forge answers 404 for is noted at its commits and not asked again until
  they move. Each pass that sent a request logs what it did and which bound stopped it.
- **Evicted first.** A row the sync held is `speculative` until a page draws it; when the store is over its 64 MiB the
  speculative rows go first, oldest first, before any row a person read. The pass fills at most half the store and no
  more than a person's reads leave, so it never writes a row the bound would drop at once; rows over 2 MiB take at most
  half of that share between them, so a few large pull requests never push the ordinary ones out. A pull request no
  longer open drops its speculative row and its note at the next pass; a new credential drops them with the source's
  held reads; removing the source cascades.
- **Threads replaced in place.** What the count misses (an edited comment, a resolved thread, a comment deleted and
  another added) the page's read behind brings. *Files* now lands a read again in place (`landInPlace` over the diff's
  rows): a thread the source brings above the line the reader is on moves the page by as much, so the line stays
  where it was (`TfPrFiles.held.test.svelte.ts`; without the change the reader's line moved down a thread's height).

## Same answer

`crates/knobas-app/tests/it/held_files.rs` runs the pass with the real Gitea adapter against a fake Gitea, then the
page's read through `sources::crud::shown`, and pins the held draw byte-equal on the wire (all but `held_at`) to the page's
read of the forge made now: a rename with changes and one without, a binary file, a name outside ASCII, two hundred files,
line comments on lines that stayed and on lines a later push moved (outdated, drawn apart with their hunks), and a
*Files* of 1,505 files over the ordinary 2 MiB cap. Beside it: an unchanged run sends nothing and moves no row
version; a moved head or merge base, and a new line comment, read again; a closed pull request drops what only the sync
held; the request cap and the share; a too-large *Files* read once; a 5xx and a slow read go last; a 404 is not asked
again; a person's read is noted with no request. `held_store.rs` pins the eviction order.

## How it was measured

Headless Chrome on the Mac (M2 Pro) at CPU 1x, 4x and 6x, on AC power, `just open-budget --real <port> --inner
--no-big --only pr-files` (*Files changed* of a pull request and of the larger one, each throttle's own items), each run
on a fresh APFS copy of the saved scale home (`scale-db/final-home`) with every source but Gitea off and Gitea at the
stand-in forge (`standin-forge.mjs --repos --pulls --delay 30`, the scale estate's repositories built again from the
seed, `standin-repos.py`), through a release build of the windowless backend with debug assertions. *Before* is
`ffb89ed2` on a copy as saved; *after* is this pass's head on a copy whose held store `held_files_fill` filled.
Four runs a side, alternated; the machine carried other agents' work (a load of 5 to 15 in the runs counted; a set run
under a load of 46 to 68 is left out of the table and told below). The pass's cost by `examples/held_files_fill.rs`,
the pass alone on the background lane at the adapter's quota.

## Before and after

*Files changed*, first open, ms from the press: the first useful frame / the frame it is drawn in, medians of four runs:

| Place | 1x before → after | 4x before → after | 6x before → after |
|---|---|---|---|
| *Files changed* | 96 / 112 → 28 / 45 | 91 / 188 → 41 / 170 | 107 / 265 → 64 / 249 |
| the larger pull request's | 76 / 93 → 30 / 47 | 98 / 181 → 55 / 164 | 106 / 256 → 72 / 248 |

The backend's time on the *Files* read: 63 ms median before (56 to 312; one round of the forge, four requests side by
side), 9 ms after (3 to 39; the held row). What is left at 4x and 6x is the page's own: the frame that lays out and
paints the diff's rows, 100 to 180 ms at 6x, the same before and after. Under the heavy load (left out above) the
difference is wider: before waited 0.3 to 1 s for the stand-in, after drew in 40 to 140 ms (first useful).

**The pull request of 3,000 changed files** (3,000 added files of twelve lines, 36,000 lines, made in the stand-in's
`scale-commerce/cart-service` and its record put in the mirror), one fresh backend a throttle, drawn ms: 241 → 157 at 1x,
403 → 292 at 4x, 466 → 432 at 6x; the backend's part 192 to 225 ms before (four requests and a 36,000-line diff parsed),
101 to 151 ms after (a 4.6 MB answer from the store, 417 KB on disk, 13 ms of it the database's). Held, it is still over
the lines by 57, 92 and 132 ms at 1x, 4x and 6x: the rest is decoding, the rule-7 check and sending 4.6 MB, then the
page parsing it and drawing 3,000 files' list. A read of the held answer as text straight into the view, skipping the
JSON tree between, measured no gain (100 to 132 ms) and was left out.

## What the sync costs

On the scale estate (62 repositories, 1,707 pull requests in the mirror, **143 of them open**: 142 of the saved
estate's and the 3,000-file one), the stand-in at 30 ms a request:

| Run | requests | time | written |
|---|---|---|---|
| First fill | 707 in 4 runs (202, 200, 200, 105), about 5 a pull request | 19 to 20 s a run, 70 s in all | 139 *Files* held (6.65 MB of answers), 4 noted as not on the forge |
| Nothing changed | 0 | 27 to 46 ms of database time | nothing |
| One pull request pushed | one *Files* read: 4 requests, and one a review with line comments | the forge's round | its row and note replaced |

The four noted are `tidewater/*`, which the scale seed does not build, so the stand-in answers them 404; on the live
forge they are held like the rest. **Disk:** 1.6 MiB for `sync.held_read` and 144 KiB for `sync.held_section` after
the fill, the 3,000-file answer 417 KB of it; answers are 1.5 KB to 78 KB as JSON (median 11 KB) but that one's 4.6 MB.
**First start:** migration `0091` (a column with a constant default, one empty table) took 40 ms on a copy of the scale
database, `psql`'s own start included. At the five-minute interval the first fill is four runs, about 20 minutes, the
most recently updated first: the forty or so pull requests someone is likely to open are held after the first run.

## Asked of Fable (xhigh)

Three decisions, with the measurements above:

- **The row cap.** ADR-0042 held no read over 2 MiB, so the 3,000-file pull request was read every push and never held.
  Ruling: **8 MiB for the *Files* ask, whoever writes it** (the sync's row and a page's read behind of the same row, so
  the page can replace the sync's copy), 2 MiB for every other ask; rows over 2 MiB take at most half the pass's share,
  and one that does not fit is noted, not held; measure the held big open against the lines and, if it misses, hold it
  anyway and say by how much (above). `held_read::FILES_ROW_CAP`, `row_cap`.
- **Thread freshness.** Ruling: **read again when the record's `review_comments` moved** as well as its commits, whole,
  through `Source::view` (no splicing, so the byte-equal pin holds); the note keeps the count, and a null on either
  side takes the record's with no read, so the upgrade and a forge that sends no count read nothing; the module says
  what the count misses.
- **The caps.** Ruling: **keep 200 requests in 45 s and half the store**; a read that had the whole budget and did not
  end goes last as a 5xx does; log which bound stopped each pass, and look at the caps again once that log exists on
  the live forge.

## Found on the way

- **A record that always fails would starve the rest.** The first version stopped the pass at a 5xx, as the stored
  trees do, and the record stayed first in the next run's order. A pull request whose diff the forge cannot answer
  would have stopped every run at the same place. Fixed as above, with a test.
- **The stand-in asked `version` on every read.** It had no `/api/v1/version`, so the adapter, which keeps the answer
  for the process, asked again on every *Files* read, one request in five of the first fill. It answers 1.27.3 now.
- **The request counter ships.** `knobas_http::counted` was behind `test-util`; the pass holds its reads to an exact
  cap with it. Outside a counted task a request costs one failed task-local lookup.

## What still waits

- **The 3,000-file pull request**, as above: the page's own decoding and drawing of 4.6 MB, over the lines by 57 to 132
  ms.
- **A pull request pushed since the last run**, until the run after it: its held copy stands at the older head, drawn
  under the held line and replaced when the read behind lands (the diff view remounts for a new head).
- **The first fill** of a new source or credential: four runs here, newest first.
- **A thread edited or resolved with no new comment**, until the page's read behind: the count does not move.

## Owed on Windows

1. In knobas's own window on ux13 at 6x, *Files changed* and the larger pull request's first open after a sync has run
   (`open-win.mjs --set inner --throttle 6`), and the held line's words on it.
2. A thread added on the forge while *Files changed* is open and scrolled: the page stays on the line it was on when
   the read behind lands.

## Owed on WSL

1. `just gitea-live` and `just gitea-live forgejo`: the held-files pass against the live forges (a forge read
   unchanged, but now made after each run).
2. `held_files_fill --secrets` on the live Gitea and Forgejo: the fill's requests and time, which bound stops a pass
   (the log), and a byte check of a held *Files* against the page's live read (`entity_view` on a fresh backend).
3. `just open-budget --real <port> --inner` on the live Gitea at 1x, 4x and 6x on this head, after a run has filled
   the held store.

## Reversing it

The pass is one call in the scheduler; removing it holds nothing more, and a page reads as before. The rows can be
deleted at any time (`speculative` ones and `sync.held_section`); `0091` stays.
