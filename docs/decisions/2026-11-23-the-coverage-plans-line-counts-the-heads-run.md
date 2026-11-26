# The coverage plans' line counts the head's withdrawn run (2026-11-23)

**Found.** The ux30 end gate's mutants baseline on GitHub's ubuntu-24.04 runner (run 45227865249, job knobas-search)
failed `coverage_plans::a_probe_never_reads_a_table_to_prove_a_source_has_nothing`: `under:`, never analyzed, Custom,
read 258 child rows of `gitea-swept-last` against 257. The plan was the one meant. The look at the far end read its 256
rows, all withdrawn, and stopped at its `limit`. The read from the head then read 2 rows: `e_1` returned 0.50 rows over
2 loops, so one withdrawn row came ahead of the first live one.

**Why the fixture moved again.** The ux28 fix (2026-11-21, *rows that moved in the ux28 gate*) turned autovacuum off,
which closed one way for a later row to come ahead of an earlier one. A second way needs no vacuum. A backend writes a
row into the last page it wrote to while that page has room, and the test's pool hands each statement any of its
connections. So when another connection has meanwhile added new pages, a later statement can fill an earlier page.
Shown on the tests' PostgreSQL 18.6 with autovacuum off: two connections taking turns, 20 live rows written on one and
then 100 withdrawn rows on the other, and 15 of the withdrawn rows came ahead of the first live one in the index. The
fixture cannot choose where its rows land, so it cannot promise a live head.

**The probe is bounded as promised.** `UNDER_COVERAGE_SQL`'s doc already says it: a run at each end costs the head's run
and the far look, and the far run is not read past its 256 rows. Merge-manager rulings on #2505 accepted that cost
(#2504). Here the head's run was one row, so the probe read 256 + 1 + 1. No query or index change.

**Taken.** The line was off by the case it named. "256 rows, and one more from the head" holds only when the head's
first row is live. Now the test reads the head's withdrawn run from the table (rows ahead of the first live one, by
place, which is the index's order for equal keys) and holds each source to the test's own arithmetic:

- `gitea-swept`, live rows at the far end: 257 as before. The look stops at a live row within its 256, and the one more
  covers a layout that put every live row at the head instead.
- `gitea-swept-last`: 256 + its head's withdrawn run + 1, which is 257 when the head is live.
- New `gitea-swept-both-ends`: the same rows, with the entities of the first 6 child rows in index order withdrawn,
  chosen by place and not by write order. That forces the heap order the runner hit. With the old line it fails every
  time (256 + at least 6 + 1), and with the new line it passes.
