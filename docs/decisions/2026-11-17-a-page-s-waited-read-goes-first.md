# 2026-11-17: A page's waited read goes first: reads behind in the background lane, Compare's standing and a commit's view behind, the README at the press

**Asked (Björn, through the orchestrator, from the ux16 Windows pass):** five reads still over target on his PC
(CPU 1x / 4x / 6x; a page within 100 / 200 / 300 ms): (1) Compare's first open on a repository without a clone,
485 / 669 / 430 ms, `entity_range_files` about 450 ms in the page and 80 to 170 called alone, unexplained; (2) a
repository's first open at window size, first read of the session, 115 / 239 / 356; (3) forge reads without a clone,
a deep file, a large file, a commit, 108 to 304; (4) *Files changed* of a 4,200-file pull request, first open, 383 to
589 at 4x and 6x; (5) the big compare's first *From* swap at 6x, 328 ms. His rule: directness, performance and
responsiveness over data scarcity.

## How it was measured

On Björn's PC, in knobas's own WebView2 window, under the PC's timing lock, Björn idle throughout (the idle guard before
every step). A scratch profile restored from the saved scale dump, synced four times so it stores trees and READMEs
as his does, then copied: every run starts from that copy, so each first open is a first read. Its Gitea source points
at a **timing proxy** in WSL in front of the live Gitea (`tproxy.py`, scratch), which logs every request's arrival,
first byte and last byte on the epoch clock; the probe (`open-win.mjs`, the ux16 pass's) records each IPC call on the
same clock, so a page's calls and the forge's requests line up per hop. A Gitea sync every 45 s through every run, as
in the ux16 pass. *Before* is v3.2.9-ux16 from its MSI; *after* is this pass's head built for Windows on the Mac
(`cargo xwin`, the release profile, the front end embedded) and copied beside the MSI's PostgreSQL. The Mac reproduced
item 1 against the same Gitea through the windowless backend and the same proxy.

## 1. Compare's first open: why 450 ms

**The cause.** The page's own read was not slow: from the IPC call to its two requests reaching Gitea took 5 to 8 ms,
and from Gitea's last byte to the answer in the page 5 to 7 ms. Gitea took the time, because the page put it on the
server beside up to eight requests nobody waited on. Compare draws the branch's standing (`entity_view` of the branch:
the pull request open from it, its commits, whether a pull request may be opened). Where knobas held it, it answered in
9 to 14 ms and the backend read it again behind, at the same moment as the compare; where it held none, the page read it
then, also at the same moment. Either way the standing's read is the repository record, then the branch, the open pull
requests, the branch's compare of every commit with its files, the behind count, the statuses and all branches; and the
repository's branch list, held, was read behind as well. On the PC at 6x the compare's list request took 600 ms on
Gitea beside them; at 1x 98 to 168. The Mac, held standings, five opens each: `entity_range_files` 282 / 502 / 802 /
660 / 943 ms, and 113 to 149 with the standing out of the way. Called directly, as the ux16 pass did, nothing went
beside it: 80 to 170. Not the cause, by the same log: the lane's quota (the compare's requests never waited on it), the
sync's requests (two in flight at most, standing aside), the IPC queue, the database pool, the stored trees.

**Taken.** Ruled by Fable (xhigh), option A of four, with four conditions:
- **A read behind runs in the background lane** (`knobas_http::behind`, `LiveSources::read_behind` and
  `read_again_behind`): its requests stand aside while a person's to the same server wait or are out, as the sync's
  do. Nobody waits on it; the page drew what knobas held and is told if the source moved.
- Condition 1: a pull request's first *Files* read made behind still keeps its diff for the next open
  (`knobas_http::is_behind`, Gitea's `keep_behind`); the sync's, in the same lane, does not.
- Condition 2: for 30 s after a person's write lands, the reads behind its repository's or item's views go as a
  person's (`live::note_write` from the write queue's `FollowLandings`), since they show the write took.
- Condition 3, measured: a branch standing read behind Compare during a scale-estate sync drew 0.9 to 1.1 s after the
  press (0.6 to 0.8 before); Fable's line is 2 s.
- Condition 4: the lane's docs, the limiter's module docs and ADR-0042 (amended).
- **A view a page draws beside its waited read is asked behind it** (`entity_view`'s `behind`, additive;
  `entityViewBehind`, `RepoPorts.behindView`): Compare's branch standing, and a commit's view, which only adds the parent
  and *New branch from here…* to the commit's diff. What knobas holds of either is answered at once as before.

| Compare, first open (drawn ms) | 1x | 4x | 6x |
|---|---|---|---|
| ux16 record | 485 | 669 | 430 |
| ux16, this pass's first run | 218 | 331 | 829 |
| ux16, from the copy | 284 | 223 | 510 |
| after, from the copy | **118** | **214** | **301** |

`entity_range_files` in the page after: 84 / 94 / 112 ms (before 252 / 85 / 296). What is left at 6x is the page's
own work: the list's draw and a second round for the first files' lines (`entity_range_file_pages`, 60 ms at 6x).

## 2. A repository's first open

**The cause.** Not the forge: the landing's reads came from the store. At 4x and 6x the page's build is one task of 100
to 170 ms right after the press; the press's reads answered during it but were handled after it, and only then did the
landing ask its README, by the section its listing named: a second round, handled after the listing had been drawn,
130 ms more at 6x. The landing's own guess at the README (`file:README.md`, asked beside the listing since the inner-6x
pass) never went out: the press's read of the listing was in before the guess could be.

**Taken.** The press asks the README guess too (`readLandingAhead`), once the record it already reads names how the
source names a file, which comes back before the page's build; the landing takes that read wherever its listing came
first and names that README (`landingGuess`, `TfRepoFiles`).

| A repository's first open, first read of the session (drawn ms) | 1x | 4x | 6x |
|---|---|---|---|
| ux16 record | 115 | 239 | 356 |
| ux16, from the copy | 60 | 249 | 411 |
| after, from the copy | **43** | **203** | **311** |

What is left is the page's build (a task of 100 at 4x, 151 at 6x); at 4x and 6x the open is 3 and 11 ms over its line.

## 3. Files and a commit from the forge

**The cause**, by the proxy: a file is one round of two requests at the commit the mirror holds (the file's entry and
its bytes), 30 to 50 ms each alone; in the page they shared Gitea with the reads behind of the folders and views drawn
from what knobas held (the repository, `branches/main`, the folders' trees, two of them twice), and the deep file's entry
took 230 ms at 6x. A commit's diff shared Gitea with the commit's own view (its statuses, its record, the repository).
The reads behind now stand aside (item 1), and the commit's view is asked behind its diff.

| Drawn ms, ux16 from the copy → after | 1x | 4x | 6x |
|---|---|---|---|
| a deep file | 131 → 109 | 137 → 156 | 338 → **162** |
| a large file | 93 → 83 | 107 → 104 | 182 → 125 |
| a commit | 54 → 57 | 168 → 160 | 200 → 196 |
| ux16 record | deep 216, large 177, commit 108 | commit 211 | deep 304 |

One run a cell; at 1x and 4x the differences are within a run's spread (Gitea answers a lone request in 25 to 90 ms on
this box). The deep file at 1x is 9 ms over its line, the forge's two requests.

## 4. *Files changed* of a 4,200-file pull request

**The cause.** Not the forge (the sync holds it) and not git: the source's *Files* is the forge's whole diff with its
threads, 5.2 MB for 4,201 files cut at the 50,000-line ceiling. Read from the store, sent over IPC and parsed on the
main thread it took 220 ms at 4x and 330 at 6x, and the clone's first lines, asked beside it at 85 to 140 ms, waited
behind that same main-thread work (the pages answered at 313 and 511). Drawn: ux16 395 / 489 ms at 4x / 6x, the same
after this pass (379 / 449).

**Tried, not taken.** The source's *Files* read once the clone's diff and its first lines had drawn (the trial's diff is
kept in the pass's scratch): drawn 300 ms at 4x and 391 at 6x, still over, and when the source's answer then replaced
the clone's diff every row moved 21 px, because the forge's diff is cut at the ceiling and says so above the files,
where the clone's is not. Drawing the clone's diff with the forge's threads placed on it, file by file, is the change
that fixes this; the big-compare pass named it a pass of its own, and it is.

## 5. The big compare's first *From* swap at 6x

**The cause**, by the probe's IPC log and CPU profile: a pair never read waits for the clone's names (one in-process
read) and then draws the list; at 6x the pick's own task, the handling of its answers and the list's draw were three
long tasks of 59, 76 and 92 ms, and the IPC calls' own main-thread time (`fetch`, 66 ms in the profile) between them.
The 39 range reads are the files read ahead after it landed, not on its path. Measured landed 280 ms on ux16 this pass
(record 328). **Not changed** in this pass: the reads behind and the standing behind the compare take its forge
requests off the server, but its time is the main thread's.

## Not taken

- **A third lane** between a person's and the sync's (Fable's option B): more limiter code for a cost nobody measured,
  and it would put reads behind on the server beside the sync's two requests, the load the ux13 cap removed.
- **Compare reading the standing only after its list** (option C): a held standing's *PR already open* line would then
  draw after the form, a layout change.
- **A file's entry from the raw read's `ETag`** (its blob sha) instead of a second request: one request a file, but its
  page link would be built from knobas's address of the server rather than Gitea's own; with the reads behind out of
  the way the two requests are 30 to 50 ms each.

## Owed

- On the PC: the first sync after an upgrade and a write's echo (condition 2) in the window; a scale-estate sync's
  length with reads behind in its lane (Fable's last measure).
- Items 4 and 5 as above.

## Gates

On the final head, in a clean clone: listed in the PR comment of this pass.
