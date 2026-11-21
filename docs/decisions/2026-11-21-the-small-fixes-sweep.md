# 2026-11-21: The small fixes sweep (for ux27)

Four small things left over from the renderers-columns pass and the ux26 Windows boot check, each fixed or explained in
its own commit.

## *Add a default* stands on the table's columns

**Found.** Settings › Renderers' *Add a default* row was a settings row: its label on the left, and on the right the
pattern, two dropdowns and *Add*. A dropdown is as wide as its longest option (`flex: none`), so with a long pipeline or
renderer name the two ran to about 1,370 px, past the default renderers' table above and past the window. The table's
own dropdowns stay in their columns (`max-width: 100%`).

**Taken.** The row is the table's next row. Its label goes above, as every settings row's does below 720 px, and its
controls stand on the table's column template (`.tf-rnd-addcols`: the rest, 24 %, 24 %, 100 px at the UI size, inside the
table's 1 px border and its cells' 10 px). The pattern fills *Files*, each dropdown starts under the table's dropdowns
and is no wider than them, in their type size, and *Add* stands where *Remove* does. A long name cuts in its box and says
itself whole in the dropdown's title. Two 24 % dropdowns could not fit the right half of a settings row (about 418 px at
the section's 804 px), so the label had to move for them to have the table's widths.

**Held by.** `testenv/ui-checks/long-paths.mjs` gains `under`: the case `settings-renderers-add-default` holds each
control of the row to the table's column at its place, at the four sizes. Before: the pattern 138 px out of *Files*, the
dropdowns 433 and 749 px out of theirs, *Add* 700 px.

## A folder renderer saved in Settings already marks the open tree

**Found (the ux26 Windows boot check).** "After a page reload its glyph stood on `docs`, because the page had read the
renderers before I saved this one." The Files tree reads the app's one copy of the folder renderers
(`render/folders.svelte.ts`), and every save in Settings › Renderers goes through that copy and holds what the backend
stored. The tree's glyphs and its marks (for a renderer that holds a file glob) follow the copy, so they follow every
save. In the built app under `?fake-ipc`, a folder renderer added in Settings while the Files tab was kept had its glyph
on the tab at once. The boot check drove the app with JavaScript in the page and most likely called
`set_folder_renderers` itself, past the copy, which no person can do.

**Taken (Fable's ruling, option B of three).** No change to the app. A backend event for this one list would leave the
other three saved lists (pipelines, command steps, defaults) as stale under the same call, and nothing else writes it.
A pin holds the behaviour: `TfRepoFiles.folders.test.svelte.ts` saves a renderer by a folder's name and then one by a
file glob while the tab is open, and each glyph comes at once. It passed before; a mutant that stops the marks from
following the list fails it. A boot check that saves through a command call reloads before it looks at the page.

## The launcher's first `Esc` takes the words

**Found (the ux26 Windows boot check).** After typing, the first `Esc` left the launcher open and the second closed it.

**Not changed.** It is the rule: the design system says `Esc` takes the words and then closes, so an opening with no
words closes in one press, and that `Esc` never loses typed words in one press (*Keys*, `filterKeys`).
`Launcher.svelte`'s `Escape` does that (the chip stays with the words gone, and the next press closes).

## The browser's fake reads the fixture's files

**Found (the Everything search finish).** The browser's fake kept a table of its own of two repositories' files
(`FAKE_FILE_REPOS` in `shell/dev/fake/launcher.ts`), beside `fixtures/tidewater/files/`, which the mock source compiles
in and `seed-gitea.sh` pushes. It had drifted: no `payout-service`, and Settings › Search counted two files left out
where the fixture has five.

**Taken.** The fake reads the fixture with an eager `import.meta.glob` as text (`exhaustive`, so the fixture's
`node_modules` folder is read too; the dev server may serve the folder, `vite.config.ts`). The engine's rules on what
text is kept are mirrored in miniature (`fakeLeftOut`: a folder, a lock file, a made file, empty, a NUL byte, a line over
2,000 bytes, a generated head), so a left-out file is found by its name only and counted by its rule.
`payout-service` is in the corpus too: its files' evidence joins its own row instead of drawing a second.
`fake-files.test.ts` holds the fake's table to the folder on disk.

## An archive the deadline cuts holds up nothing

**Found (the Everything search finish).** The files pass reads a repository of many small files as one archive. Gitea's
archive is downloaded whole into a spool before it is read, so one that the run's 120 s deadline cut kept nothing. Its
repository stayed owed, came first again in the next run's queue, asked for the same archive and was cut again, and every
repository after it waited. The 1 GiB cap on an archive narrowed that, and did not close it.

**Taken.** A cut goes last. A repository whose archive the deadline cut is read a file at a time from then on
(`ARCHIVE_CUT`, the process's), and single reads keep what each one wrote, so it finishes over the next runs. Any
repository whose first read the bounds cut (its row read and at no commit) is read after every other the next run, so
after a restart the cut one still waits for the rest; it may ask for its archive once more with what that run leaves.
`an_archive_the_deadline_cuts_is_not_asked_again_and_holds_up_nothing`: the newest repository's archive outlasts the run.
The next run reads the other one, then the cut one a file at a time. Before, the second run asked the same archive again
and read nothing else.

Also fixed: a run the deadline stopped counted as owed only the repositories it had not reached, dropping those it had
cut on the way (`passed.owed` was set where it is added to).

## A slow `entity_view` after a clone was Compare's read ahead

**Found (the ux26 Windows boot check).** "One log line looks odd: `slow ipc command command=entity_view
duration_ms=11294`, around the first Files open just after the clone." The tree had drawn and the README opened.

**Measured** on Linux, with the windowless backend on a restored scale copy and the live Gitea: a sync, a clone of
`scale-platform/runbooks` three seconds into it, then the repository's first Files open in the built front end, every
command timed and, for that run only, every request's wait on the limiter and its send printed. The tree drew from the
clone 135 ms after the open; `commits` answered in 23 ms, `branches` in 17, the README in 257 and `files` in 811 (the
copy had no stored tree, so a read of the forge whose first request waited 754 ms on the quota). The one long call was
`entity_view` with `behind` on the branch Compare reads ahead (`2026-11-20-compare-is-built-before-its-press.md`):
3,543 ms. A read behind goes in the background lane, and the sync's requests there waited 1 to 3.8 s each beside it.
Not the cause: the database (its queries took 1 to 3 ms, a pool connection under 2 ms), the runtime (a 50 ms ticker
never ran late), the clone. The boot check's log is gone, so the 11.3 s call's arguments are inferred: the read ahead
fires on the Files open, and a first start's tree pass keeps the sync's requests waiting the whole time.

**Taken (Fable's rulings).** The Files page owed nothing: it draws at once and fills in, and nobody waits on a read
behind. What the boot check flagged was the warning line, so that is what changed: `entity_view` with `behind` marks
its command, and the slow-command line for it is at info with `behind=true`, not a warning
(`a_slow_read_behind_is_logged_at_info_and_marked`, red before). A boot check's log now tells a read nobody waits on
from one a person does.

**Tried, not shown: a read behind ahead of the sync.** The origin's background requests have two places on the server,
and a place given back goes to whichever waiter is polled first, so the read ahead's requests drew lots with the sync's.
A rule that gave a waiting read behind the next place (and a sync holding none the one after) was built and timed,
the old and the new binary alternated on the same copy. The read ahead took 4.3, 3.6, 3.6, 3.2, 7.7 and 2.7 s before,
and 2.3, 2.7, 16.3, 5.2, 3.1, 2.8, 12.8, 2.7 and 2.6 s after: no improvement a rank test shows, and a heavier tail.
Every section a page draws from what knobas held is read again behind as a task of its own, so the rule mostly made
reads behind share one place among themselves. It was taken out. Reopen it only with a per-request trace that splits
the read ahead's eight requests into the wait for a place, the wait on the quota, the standing aside and the send.

**Owed on Windows:** the clone-then-open on the scale copy again, to see the boot check's call marked `behind=true`
and shorter.
