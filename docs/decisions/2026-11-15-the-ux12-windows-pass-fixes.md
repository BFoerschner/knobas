# 2026-11-15: What the ux12 Windows pass found, fixed: the sync's quota, Standup, a repository's first open, first start

**Asked (Björn, on PR #2479):** fix what the Windows pass on v3.2.9-ux12 found, in his own window on his PC (Ryzen 5
5600X, 240 Hz, his scale data): the Tickets read about 1.5 times the Mac's, a repository's first open in three rounds
of reads, Standup's first open at 6x, a Gitea sync that stopped on knobas' own quota, `initdb` at 4.1 to 4.9 s of a
first start, and a server log in German. His rule: directness, performance and responsiveness over data scarcity; a
decision goes to one Fable agent.

**How it was measured.** Headless Chrome on the Mac (M2 Pro) at CPU 1x, 4x and 6x, through release builds of the
windowless backend on APFS clones of the scale home (39,770 items, 23,176 tickets), the front end built as the probes
build it. Before and after are the head before (`e55c4364`) and this one, two backends on two clones, alternated, two
runs each; statements split from the server's log and with `EXPLAIN (ANALYZE, BUFFERS)`. The machine carried other
agents' work, so single numbers run 10 to 20 ms either side.

## 1. The sync stopped on knobas' own quota

**Why.** A sync request standing aside for a person's reads spent its 45 s `SEND_BUDGET` doing it. Worse, the floor
that promises the sync about one request a second did not hold: past it a sync request only raced every waiting person's
request for each refill, and with a hundred waiting, ten sync requests got none through in five seconds. And a
background request past the floor slept until a floor already behind it, which ends at once: it spun a core until the
quota refilled (on tokio's paused clock, for ever).

**Taken** (`knobas-http/src/limiter.rs`, `lib.rs`; frozen surface). The time a background request waits while a
person's requests hold the origin is not counted against its allowance or the client's budget, for at most
`STAND_ASIDE_CEILING` (five minutes) more; a gone forge fails the person's requests too, so it never holds the sync, and
the ceiling ends a wait behind requests that never stop. Past the floor, a person's request lets the overdue sync take
the next request the quota lets through; both lanes read one origin-wide floor. The spin is gone. Tests on the paused
clock: `standing_aside_for_a_person_does_not_count_against_the_wait` (red before: all ten ended on the quota at 5 s; it
hung before the spin was fixed) and `a_sync_request_stands_aside_for_at_most_the_ceiling`; on the wall clock,
`a_sync_request_standing_aside_keeps_its_budget` (red with the budget change taken out).

## 2. A repository's first open in three rounds

**Why.** The tabs waited for the branch's name, from the repository's row or its branch list, before Files mounted, and
the README waited for the tree: 219 ms at 4x and 391 at 6x on the PC.

**Taken** (`TfRepo.svelte`). On a repository no branch was picked on this session, Files reads at the repository itself,
which is its default branch whatever either names, so it mounts at once and names the branch when one answers: every
read of the landing goes out in one round. The tabs are keyed by repository, so another repository still mounts them
afresh (#2248). Tried and left: the page's own reads as a pre-effect, which broke the page.

| Drawn, ms (two runs) | 1x | 4x | 6x |
|---|---|---|---|
| before | 40, 65 | 95, 106 | 159, 146 |
| after | 40, 56 | 87, 85 | 114, 141 |

## 3. Standup's first open

**Why.** The digest took 90 to 150 ms in the backend, every read in series; 61 ms of it was the blockers' statement,
which read every live ticket of a kind with a blocked set, with its stored assignee and status, before keeping the
user's 1,959 of 20,007. The page then built the 200 lines of *yesterday* in the frame the answer landed in: 74 ms of
script at 6x on the Mac, 208 on the PC.

**Taken** (`standup.rs`, `TfStandup.svelte`). Today, the blockers, and yesterday followed by its alerts are read at once;
the blockers' two statements side by side. Where the declarations' assignee is stored for a source and kind
(`stored_under!`), the user's items come off `item_field_value_idx`; payloads are walked only for kinds whose assignee is
not stored (61 ms to 14). Each list draws its first 24 lines in the frame its answer lands in and the rest the frame
after, as the board's columns do. Held by `the_blocked_status_read_finds_the_users_items_as_the_scan_did` (stored and
walked, an assignee in another case, a withdrawn item; a mutant without the exact-case test fails it) and the page's
frame test.

| | before | after |
|---|---|---|
| digest, backend alone (median of five) | 100 ms | 31 ms |
| first open drawn at 1x (two runs) | 43, 43 | 43, 39 |
| at 4x | 99, 97 | 48, 42 |
| at 6x | 116, 119 | 82, 53 |

## 4. A Tickets key

**Why it is 1.5 times the Mac's on the PC.** Not JIT: none of the PostgreSQL archives knobas bundles (macOS, Windows,
Linux 18.6.0) carries `llvmjit`, though every board plan costs 20 to 60 million against a JIT threshold of 100,000. The
Windows build is MSVC's, whose expression interpreter has no computed-goto dispatch, and a key is per-row expression
work: on the Mac a letter's read is 50 to 55 ms, the board's statement 37 to 41 of it, of which `scoped` (23,176
tickets joined to their cards, each row's columns and the filter worked out) is 26 ms and the per-column cut 8; the
statuses beside it 10 ms on a second connection; signals 3 ms after; the rest small reads and the answer.

**Tried and left, each measured.**

- The settings: `random_page_cost` 1.1 and `work_mem` 32 MB changed the statement by under 1 ms (it is all in memory);
  `jit` changes nothing where there is no JIT. None set.
- A partial index of the withdrawn entities: the check went 1.4 ms to 0.06, the statement 2 ms, and migration `0078`
  records that with one the planner probed it once per ticket on analyzed tables, which `read_plans.rs` refuses.
- Each column's newest by a bounded sort: 3 ms of 42, the same rows; it rescans `scoped` once a column, which
  `read_plans.rs` refuses and which a mirror with many statuses would pay for.

**Left, with what it would take.** Carrying the ticket's title, kind and dates on its stored card, so `scoped` reads
cards alone where every ticket has one: bounded on the Mac at 24 ms to 8 for that step, about 40% of the statement. It
needs a migration, the card triggers on title and date updates (a re-synced item rewrites its card row), the board
statement and the search's narrowing pieces rewritten together, and the board's equivalence suites. Cancelling a stale
key's statement helps typed words, not single keys, and needs a cancel the pool does not offer. The statuses beside the
cards could be kept between keys.

## 5. First start

**Why.** `initdb` took 4.1 to 4.9 s on the PC, 2.0 s of it its own sync, file after file; bring-up began only after the
WebView2 window was made (launch to `knobas starting` 0.3 to 0.75 s).

**Fable's ruling** (the one decision agent, xhigh), asked with the facts and options A (keep the sync), B (`--no-sync`,
accept the risk), C (`--no-sync` and knobas flushes on threads before marking), D (flush in the background beside the
start): **C, not D.** D hands roughly forty callers of `EmbeddedDb::start` a directory the next start would remove, so a
kill in the first seconds goes from harmless to a deleted cluster; gated inside `start` it overlaps only 0.65 s; and an
orphan or a stale lock on a torn directory needs branches of its own. C can take most of the 2 s away, since the drive
answers many waiting flushes with one.

**Taken** (`knobas-db/src/embedded.rs`). `initdb --no-sync`, then every file (and on Unix each directory) is written to
disk on sixteen threads, a plain `fsync` on Unix (std's `sync_all` is `F_FULLFSYNC` on macOS) and `FlushFileBuffers` on
Windows, bounded by the command timeout; a failed flush fails the bring-up, so no server runs on an unflushed directory.
The marker is written and flushed after. A complete, unmarked directory no server ran on is now unfinished and redone:
that also closes the hole an OS crash during `initdb`'s own sync left. Mac: the flush is 13 ms where `initdb`'s own sync
was 36. `INITDB_NO_SYNC` is the switch back. Bring-up now starts before the main window is made, and the window's making
is timed in the log.

## 6. The server's words

**Why.** `initdb` records the environment's language as the cluster's `lc_messages`, so Björn's server wrote German.
Every error knobas tells apart is told by its SQLSTATE; the one reading of the server's words is a failed start's
`FATAL:` line, a label some translations change.

**Taken.** `-c lc_messages=C` at every start, `--lc-messages=C` and `LC_MESSAGES=C` for `initdb`; checked against
PostgreSQL 18's source (Windows checks it as `LC_CTYPE` in `initdb` and accepts it in the server). Held by
`the_server_speaks_c_whatever_language_it_was_started_under` (a cluster made under `de_DE.UTF-8`; red before) and
`build_settings_has_the_server_speak_c`.

## Owed on Windows

1. **The flush's go/no-go (Fable's).** On the PC, three times: `initdb --no-sync` into a scratch folder with the app's
   own flags (the `initdb` line of `run_initdb`), then time knobas's flush, or as a stand-in a first start of a scratch
   profile, reading `initdb made the data directory` and `the data directory is on disk` (ms) from `knobas.log`. At 0.7 s
   or less, keep it; over, set `INITDB_NO_SYNC` to `false`.
2. Launch to `knobas starting` and `the main window is made` (ms) in the log of two fresh profiles.
3. `show lc_messages` answers `C` on the scale profile's server, and its `start.log` is English after a restart.
4. Keys, Standup and a repository's first open at 1x, 4x and 6x in the window on ux13.
5. A sync of the scale Gitea while a page's reads run (the record's `x-syncnow-long.js`): no `local rate limit` stop.

## Owed on WSL

`just live-sources`, `just live-demo` on the live products (Gitea, Forgejo, Kuma), and the inner pages
(`just open-budget --real <port> --inner`) on this head.
