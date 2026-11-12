// Trips across knobas under ?fake-ipc in headless Chrome: what a person does on a second press, on leaving and coming back, and on Esc.
// It presses things, from the table below, written by hand. testenv/README.md, *UI checks*, says how to run it.
//
// Usage:
//   testenv/ui-checks/build-app.sh . <folder>
//   testenv/ui-checks/with-built.sh <folder> <port> node testenv/ui-checks/trips.mjs <out dir> --port <port> [options]
// Options:
//   --only a,b      only trips whose id, kind, finding, page or route contains one of these
//   --workers n     parallel Chromes (default 2)
//   --gap n         ms between the two presses of a trip with `leaves` or `returns`, to prove one fails once its hold has let go
//   --strict        also run the trips marked `strict`, and hold esc-sweep to the scroller rule. They fail on a head without Round 14's
//                   fixes, so a plain run leaves them out and gives the same trips and verdicts as before. The whole-round check runs both.
//
// Writes <out dir>/trips.json and trips.md. Exits 1 if a trip fails or can't run: a trip that no longer finds its control says so.
//
// Kinds, and what fails:
//   enter-twice   focus a control, press Enter twice with real key events. Fails if the second Enter changes the address, opens a
//                 dialog or drawer, or sends a write, or if the press lands on a control with different words. A control that keeps
//                 focus and changes its words is a hold, not a landing. A second write is fine only once the pressed row has left
//                 the list and focus took the next row's same words (the keeper's rule), which needs the trip's `rows`.
//                 `leaves`: the first Enter is meant to change the address. The second must still write and open nothing, and stay there.
//                 `returns`: the press closes a dialog or an editor and focus goes back to the control with these words, as the spec asks.
//                 That landing is accepted; a second Enter that writes, reopens or moves still fails.
//                 `returnsCounting`: the control focus goes back to counts as it goes (*Cloning 3 of 9…*), so `returns` is its start.
//                 `opens`: the first Enter is meant to open a dialog (*Clone all…*, PR #2479). The second must leave it open and send nothing. These trips press again after `gap`
//                 ms (200 by default), inside the hold. --gap n presses after n ms instead, so --gap 1000 shows the trip can fail.
//   leave-back    press by key, which raises a toast and hides a row. While the toast is up, go to `via` and back. Fails if the rows
//                 differ from just after the press. Can't run if the press hides no row or the toast has gone by the return.
//                 `rowsStay`: the press isn't meant to hide one (*Save link set*), so the rows are only compared. `reopen`: what to press on the
//                 return, for rows in something a leave closes.
//   esc-sweep     focus each field on the page, or those in `fields`, and press Esc. Fails if focus falls to the body. For each of `typed` (a value with no
//                 Save, which is the rule in Settings), also type a character, press Esc, leave and come back, and fails if the typed
//                 value stayed.
//
//                 With --strict, also fails if Esc lands focus on a scroller: .tf-scroller, or any tabindex=-1 box that scrolls (F-649).
//
// Only with --strict:
//   enter-fast    Enter twice at gaps of 0 and 120 ms, with `rows`. Fails if the second one writes too, which is what happens when focus has
//                 moved to the next row's same control. Each press here writes once. Can't run if the first press writes nothing.
//   enter-walks   Enter, wait 600 ms, Enter. Fails if the second writes nothing: a person must still be able to walk down a list.
//                 Can't run unless the row left and focus took the next row's same words.
//   pointer-twice real mouse presses at 15% of a button's width, 120, 300 and 600 ms apart, a page each. Fails if a dialog opens, and with `once` if the second press writes.
//                 With `rows`, a first press that writes nothing acts by changing them, and the second fails if it changes them again or the address.
//                 `reveal`: the button is in a row's fading group, so the pointer rests on it first, which brings the group up.
//   undo-stale    make `changes` to one setting, press Undo, and read `value`. Fails unless it is the value from before the changes, if a
//                 toast still offers an Undo afterwards, or if the changes leave two toasts up. With two Undos up it tries each, newest first,
//                 on a fresh page. If no Undo is up and the value is back at the first, there is nothing to press.
//   rail-top      `scroller` scrolled to its end, press `press` by pointer and then by key: fails unless `scroller` reads 0. Can't run if it has nowhere to scroll.
//   aimed-after-close  by pointer, press `open`, then `close` in the dialog it opens; at once `Tab` and `Enter`. Fails unless that Enter acted: a press aimed
//                 at something new, straight after a dialog closed, must land.
//   column-filter on a table, type `filter` a letter at a time or press `pill`. Fails if a column moves over 1 px from where it stood.
//                 Can't run if the rows don't change, so it can't pass without having filtered. `sizes` are [w, h] pairs, 1440 x 900 if absent.
//   walk-done     by real pointer, press `press` in a row, glide to the next row's in 190 ms, wait 150 ms, press, then leave the list. Fails unless
//                 exactly those two rows finished. F-159's numbers, from Round 14's reviewer: a row that closes up mid-glide puts the wrong row under the pointer.
//   dialog-typing `prep` opens a dialog, then type `word` (or a word starting like another of a select's options) with real keys. Fails if a select's
//                 value changed, if the word landed in no field, or if focus isn't on `[data-arrive]` when the dialog names one. `stays`: the plan
//                 settled that this dialog starts on its select, so typing is free to change it, and it fails only if focus has moved off the select.
//   kept-opening  scroll `scroller` to 600, then by `then`. other: `via`, then the address `other`. launcher: `via`, then `other` by ⌘K and `words`. Both must read 0,
//                 and so must .tf-main. same: `via` and back. esc, back: `other`, then Esc or ⌘[. continue: `via` if any, Docs, then `press`. All four must read what was
//                 scrolled. landing: `via`, then `other`, or back and `press`. It must reach `other` with `to` in view, or within 80 px of the top with `top`.
//                 press: press `press` by pointer inside the page, with no hide. The address must change, and `scroller` must read what it was scrolled to (round 15's reconcile).
//                 `size` is a [w, h] for a window short enough to scroll. Can't run if the scroller has nowhere to scroll.
//   add-source-fold  Add a source at each of `sizes`, by pointer and by key: a press on a kind, twice, folds to it with focus on the address, and the second lands on
//                 nothing. Change, twice, opens the list on the chosen row, and Enter twice the same. An arrow picks and keeps it open, Tab folds and goes to the address. Esc from
//                 Change folds and stays. A typed token stays until another kind empties it, is in no storage, and a restored draft arrives folded with it empty.
//   strip-press   at each of `sizes`, press every row of `row`'s column by real pointer, down and back up. Fails if the column's x or the pane's left edge moves
//                 more than once in the walk (F-560: a holder's next level may fold the strip, once), or if a press selects nothing.
//   strip-walk    at each of `sizes`, focus `row`, walk its column by ↑ to the top and ↓ to the end. Fails if the column's x or the pane's left edge moves over 1 px,
//                 or if a press moves nothing. Then → on a row with a › must draw the next column and take focus into it.
//                 These four came in Round 15. On 53a9db9d, 10 of the 23 fail until its fixers land: inbox-walk-done, time-walk-dismiss, type-new-page-here, type-new-page,
//                 type-create-ticket, type-set-pattern, kept-docs-other, kept-docs-launcher, kept-asset-other, strip-walk-gitea. Also kept-docs-continue-away, which no fixer owns yet.
//
//   find-key      Round 16's. Scroll the page to 600, press ⌘F: fails unless the box named `box` holds focus, on screen. Then type, press ⌘F again and fail unless its words
//                 are selected, and press Esc until out of the box: fails if the address changed or focus fell to the body. `nothing`: the page has no box, or a dialog owns the
//                 key, so fails if focus or the address moved. `seed`: the box belongs to the launcher, which only checks the box. `words` with `count`: the Docs bar, which
//                 must read that count with the scroller at or below where it was as the words are typed, and go back to it on Esc (F-458).
//   find-ladder   Round 17. Scroll to 600 and press ⌘F: fails unless the box named `box` holds focus. With `chip`, the launcher must carry that section's chip. With `words`, the find bar: opening
//                 moves nothing, typing changes no file shown, `count` matches, a second press selects, Esc goes back and clears the marks, and after a step Esc stays. `fallback`: with no Highlight API, a step selects the match and a key typed next goes in the box.
//   nav-return    Round 16. At each of `sizes` (1440 x 560 and 1100 x 560 if absent): open by address, or with `enter` by a nav press, then `prep`. Scroll each of `scroll`, take `steps` ({ go }, { press },
//                 { scroll } or { eval }), press `via`'s nav entry (Docs if absent) and `section`'s by real pointer. Fails if a scroller differs from what it was scrolled to, if focus left the entry,
//                 or if Esc then moves the address or the scroll. `wheel`: a wheel 120 ms into the hold must stay, added to the place. `same`: a press on the section you're on, with no `via`.
//   hold-to-undo  Round 16. At each of `sizes`, by real pointer: press `press` in a row, glide to the toast's Undo in 300 ms, rest, press it. Fails if any of `rows` moves on the way, or before the press has landed.
//                 After it the rows must stand as they stood, with no row held. `aim`: glide to that instead, press it and require `opens` (a selector) to appear, with the hold closed once it has landed.
//   hold-gutter   Round 16. Press `press` by real pointer, drift right of `list` and down a row, wait for the toast to go (`wait` ms), and come back to the next row's control. Fails if a row moved or the pointer lands on another row's.
//   hold-leave    Round 16. Press `press` by real pointer, press `via`'s nav entry and `section`'s while the toast stands. Fails if the row came back or a hold outlived the leave. Undo must then bring it back.
//
//   row-room      PR #2479. At each of `sizes`, with a pointer that hovers and then with none (touch emulation, as Linux's headless Chrome on the release
//                 runner reads), every button in a row's `cell` must end inside `table`: at rest, with the pointer on the row, and with keyboard focus in its
//                 fading group (Tab into it). Fails naming the size, the row, the button and how far it runs past. `query` is added to the address, as for any trip.
//
//   comment-view  Round 16. Type in the composer, with *Comment* resting 6 px above the bottom of its scroller, and send by `how` (key or pointer). Fails unless the box and *Comment* end above the status bar. `size` is a [w, h] where the default doesn't scroll.
//
//   scroller-keys Round 17. `how`: text (a real press in the text of `scroller`), twice (the same press twice with a still pointer), address (just opened), tile (`tile` pressed by pointer), nav (a press in the text, then `via`'s and
//                 `section`'s nav entries by pointer). Then each of `keys`, on a fresh page. Fails if `scroller` doesn't move, if the press moved it, or if focus draws a ring. `size` is a [w, h] short enough to scroll.
//   scroller-select Round 17. Drag across the text of `scroller`, then PageDown. Fails if the drag selects nothing, if a press in it moves the scroller, or if the selection doesn't survive the key.
//   pane-keys     Round 17. `how`: text, blank, wheel, link (a real press on `link`, then the pane), row (`row` pressed by pointer), column (a wheel over a column). Then ↓ and ↑. `walks`: ↓ must change the address, as the tree's walk. Otherwise
//                 fails if the address changes or the pane doesn't move.
//
//   nav-reopen    Round 17. At each of `sizes`, by pointer and by key: open `route`, go to the item at `left`, scroll `scroller` to `at`, press `via` (Docs if absent, `via: null` for none) and `section`, which must land on
//                 `route`. Press `row` twice with a still pointer, or Enter twice. Fails unless that opens `left` at `at` on one history entry. Esc must go to `route`, and `row` again must open `bare` at 0, as it is used once.
//                 `far`: more entries on the way, which push the page out of keep-alive. `then: "other"`: `other`'s row first, which opens `otherBare` at 0 and ends the memory. `then: "launcher"`: ⌘K `words` from `via`, which opens `bare` at 0.
//                 `then: "tab"`: `tab`'s link, which opens `tabTo`, not `left`. `then: "back"`: ⌘[ from `via` to `left`, then the entry and `row`, which opens `bare` at 0. `keeps` is { sel, value } that must read `value` after the reopen.
//   nav-docs      Round 17. `then: "search"`: a search, Inbox, the entry, each pressed by pointer: it must stand with its *Clear*. A hit, the entry, Clear, Inbox, the entry goes to #/docs. `then: "folds"`: folds opened and closed on the home and on a space hold through Inbox and the entry.
//   gear-return   Round 17. Settings' section scrolled, Inbox, the gear by pointer and ⌘,: address and scroll as left, Esc back to Inbox. On the section, the gear pressed twice and Enter twice move nothing. A source's page goes to Sources; the status bar's *Sources* goes to #/settings.
//   walk-place    Round 17. `scroller` at `at`, `via` and `section`, then `j`: scroll stays, focus on a row in view. With no row focused `k` likewise. From scroll 0, `j` then `j` then `k` walk from the first row.
//
//   field-in-view Round 17. Press `open`, then type a line and Enter `times` times with real keys. Fails if `field` lost focus or ends under the status bar after any Enter. `size` is a [w, h].
//   press-holds   Round 17. Press `press` twice by pointer with a still pointer, and on a fresh page Enter twice by key, then `via`'s nav entry and back. Fails if the control moves from under the pointer or the pressed point stops being that control,
//                 or if `line` didn't draw below it.
//
//   scroller-tab  Round 17. `scroller` at `at`, a real press in its words, then Tab. Fails if the scroller moves, or if focus lands on nothing.
//   pane-esc      Round 17. A real press in the pane's words, then Esc. Fails unless the address moved, focus left the pane, and the next ↓ walks.
//
//   rows-still    The still-rows pass (PR #2479, 2026-11-11; Björn: "another repo just pops in where my cursor was"). Records the box of every
//                 `rows` element in view, each frame, from the first draw (3 rows, or all there are) until the page has settled, and fails if
//                 one of those rows moves or changes height, leaves, or a row comes in above one of them in its column. `query` holds `fake-late`, so each
//                 late read lands after the first draw. `ways` (all five if absent): first (from `via`), return (to `via` and back), sync (to `via`,
//                 `grow` adds rows to the fixture and every run ends having changed something, then back), shown (the same while on the page;
//                 `held` also needs *Show N new* up afterwards, and pressing it must bring the rows in; `rest`, a selector, rests the
//                 pointer on the first such element before the sync, and `pill` names the button the rows wait behind in place of the
//                 bar's *Show N new*: Builds' runs, which hold only under a resting pointer), launcher (from `via` by ⌘K and `words`),
//                 palette (⌘K on the page: the launcher's own rows). `prep` runs on the page first, for every way but first and launcher.
//                 `tail` is how long past quiet it still records (600 ms): longer for a page that reads again a while after a run ends.
//
// A first Enter that changes nothing can't run: the trip would pass without having pressed anything.
// Writes are counted by wrapping window.__TAURI_INTERNALS__.invoke, which @tauri-apps/api looks up on every call (shell/dev/fake-tauri.ts).
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { launch, screenOpener, sleep } from "./chrome.mjs";

const REPO = "repo/mock%3Atidewater%2Fpayout-service";
// A hidden monitor adds the fold's own row, so a Hide would otherwise leave the count alone.
const MONITOR_ROWS = ".tf-mon tbody tr:not(.tf-mon-foldrow)";
const RUN = "build/teamcity%3AbuildType%3APayout_IntegrationTests/teamcity%3Abuild%3APayout_IntegrationTests-1187";

// The still-rows trips' fixture: Björn's scale, each read landing late, and `window.__fake` to grow the corpus and end a run.
const STILL_Q = "&fake-scale&fake-sources-healthy&fake-hooks&fake-late=list_entities:250,code_facts:700,entity_checkouts:900,list_sources:150,filter_suggestions:400,mini_board:200,inbox_items:250";
// A repository pushed after everything else, with an open pull request asking the reader's review, as a sync brings them: both
// sort to the top, where the reader is.
const GROW_FORGE = `window.__fake.corpus.push(
  { entity_id: "mock:tidewater/still-new-service", kind: "repo", title: "tidewater/still-new-service", updated_at: "2026-12-01T00:00:00Z", body_text: "", author: "mara", web_url: null, deleted_at: null, payload: { full_name: "tidewater/still-new-service", default_branch: "main", teams: ["Payments"], language: "Rust" } },
  { entity_id: "mock:still-new-service#1", kind: "pr", title: "A pull request the sync brought", updated_at: "2026-12-01T00:00:00Z", body_text: "", author: "jonas", web_url: null, deleted_at: null, payload: { num: 1, repo: "still-new-service", state: "open", chip: { text: "open", tone: "info" }, path: "tidewater/still-new-service", requested_reviewers: ["mara"] } },
), 1`;

// A ticket updated after every other, in the first status, as a sync brings one.
const GROW_TICKET = `window.__fake.corpus.push({ entity_id: "mock:PAY-9001", kind: "ticket", title: "A ticket the sync brought", updated_at: "2026-12-01T00:00:00Z", body_text: "", author: "mara", web_url: null, deleted_at: null, payload: { key: "PAY-9001", fields: { project: { key: "PAY", name: "Payments Platform" } }, status: "To do", priority: "High", assignee: "mara", type: "Story", labels: [] } }), 1`;

// By page. `press` finds the first visible control with those words; `prep` runs first: { click, text | starts }, { fill, text }, { eval } or { wait }.
const PAGES = [
  // The still-rows pass (PR #2479, 2026-11-11): each late read lands after the first draw, and nothing drawn may move.
  { page: "Code, Repositories", route: "code", trips: [
    // *Show N new* itself: a sync brings a repository above the drawn rows, then the press, twice by key and twice by a still pointer.
    { id: "show-new-key", kind: "enter-twice", strict: true, finding: "still-rows", query: STILL_Q, prep: [{ wait: 800 }, { eval: GROW_FORGE }, { eval: "window.__fake.endRuns(), 1" }, { wait: 2500 }], press: { sel: ".tf-fheld", starts: "Show" }, rows: "tbody tr.tf-row" },
    { id: "show-new-pointer", kind: "pointer-twice", strict: true, finding: "still-rows", query: STILL_Q, prep: [{ wait: 800 }, { eval: GROW_FORGE }, { eval: "window.__fake.endRuns(), 1" }, { wait: 2500 }], press: { sel: ".tf-fheld", starts: "Show" }, rows: "tbody tr.tf-row" },
    { id: "code-rows-still", kind: "rows-still", strict: true, finding: "still-rows", via: "inbox", words: "code", rows: "tbody tr.tf-row", held: true, query: STILL_Q, grow: GROW_FORGE },
  ] },
  { page: "Code, Pull requests", route: "code/prs", trips: [
    { id: "code-prs-rows-still", kind: "rows-still", strict: true, finding: "still-rows", via: "inbox", ways: ["first", "return", "sync", "shown"], rows: "tbody tr.tf-row", held: true, query: STILL_Q, grow: GROW_FORGE },
  ] },
  { page: "Tickets, board", route: "tickets", trips: [
    { id: "board-rows-still", kind: "rows-still", strict: true, finding: "still-rows", via: "inbox", words: "tickets", rows: ".tf-tcard", held: true, query: STILL_Q, grow: GROW_TICKET },
  ] },
  { page: "Tickets, List", route: "tickets", trips: [
    { id: "list-rows-still", kind: "rows-still", strict: true, finding: "still-rows", via: "inbox", ways: ["return", "sync", "shown"], prep: [{ click: "button, a, [role=tab]", text: "List" }, { wait: 500 }], rows: ".tf-main tbody tr", held: true, query: STILL_Q, grow: GROW_TICKET },
  ] },
  { page: "Inbox", route: "inbox", trips: [
    { id: "inbox-show-new-key", kind: "enter-twice", strict: true, finding: "still-rows", query: STILL_Q, prep: [{ wait: 800 }, { eval: "window.__fake.inboxNew(), window.__fake.endRuns(), 1" }, { wait: 2000 }], press: { sel: ".tf-fheld", starts: "Show" }, rows: ".tf-ib" },
    { id: "inbox-show-new-pointer", kind: "pointer-twice", strict: true, finding: "still-rows", query: STILL_Q, prep: [{ wait: 800 }, { eval: "window.__fake.inboxNew(), window.__fake.endRuns(), 1" }, { wait: 2000 }], press: { sel: ".tf-fheld", starts: "Show" }, rows: ".tf-ib" },
    { id: "inbox-rows-still", kind: "rows-still", strict: true, finding: "still-rows", via: "code", words: "inbox", rows: ".tf-ib", held: true, query: STILL_Q, grow: "window.__fake.inboxNew(), 1" },
    { id: "launcher-rows-still", kind: "rows-still", strict: true, finding: "still-rows", ways: ["palette"], rows: "[role=option]", query: STILL_Q },
  ] },
  { page: "Builds", route: "builds", trips: [
    // Runs are newest first always (the monitors-builds pass, Björn on ux6): a run a sync brings waits only while the pointer rests on the
    // runs, behind the pill over their top; a first draw, a return and a sync away draw it on top.
    { id: "builds-rows-still", kind: "rows-still", strict: true, finding: "still-rows", via: "inbox", words: "builds", rows: ".tf-main tr.tf-row, .tf-main .tf-rail a", held: true, rest: ".tf-runs tr.tf-row", pill: ".tf-newruns button", query: STILL_Q, grow: "window.__fake.runNew(), 1" },
  ] },
  // The seeded roster, not the scale's bulk one: `monitorDown` takes a seeded monitor down.
  { page: "Monitors", route: "monitors", trips: [
    { id: "monitors-rows-still", kind: "rows-still", strict: true, finding: "still-rows", via: "inbox", words: "monitors", rows: ".tf-mon tbody tr", query: STILL_Q.replace("&fake-scale", ""), grow: "window.__fake.monitorDown(), 1", tail: 3000 },
  ] },
  { page: "Docs", route: "docs", trips: [
    { id: "docs-rows-still", kind: "rows-still", strict: true, finding: "still-rows", via: "inbox", words: "docs", rows: "a.tf-dh-row, [role=treeitem]", query: STILL_Q },
  ] },
  { page: "Docs, a space", route: "docs/space/ENG", trips: [
    { id: "space-rows-still", kind: "rows-still", strict: true, finding: "still-rows", via: "inbox", ways: ["first", "return", "sync", "shown"], rows: "a.tf-dh-row, [role=treeitem]", query: STILL_Q },
  ] },
  { page: "A repository, branches", route: `${REPO}/branches`, trips: [
    { id: "branches-rows-still", kind: "rows-still", strict: true, finding: "still-rows", via: "inbox", ways: ["first", "return", "sync", "shown"], rows: ".tf-main tbody tr", query: STILL_Q },
  ] },
  { page: "A repository, pull requests", route: `${REPO}/prs`, trips: [
    { id: "repo-prs-rows-still", kind: "rows-still", strict: true, finding: "still-rows", via: "inbox", ways: ["first", "return", "sync", "shown"], rows: ".tf-main tbody tr", query: STILL_Q },
  ] },
  { page: "A repository, commits", route: `${REPO}/commits`, trips: [
    { id: "commits-rows-still", kind: "rows-still", strict: true, finding: "still-rows", via: "inbox", ways: ["first", "return", "sync", "shown"], rows: ".tf-main tbody tr, .tf-main [data-place]", query: STILL_Q },
  ] },
  { page: "Standup", route: "time/standup", trips: [
    { id: "standup-rows-still", kind: "rows-still", strict: true, finding: "still-rows", via: "inbox", ways: ["first", "return", "sync", "shown"], rows: ".tf-main li, .tf-main tbody tr", query: STILL_Q },
  ] },
  { page: "Time, week", route: "time/week", trips: [
    { id: "week-rows-still", kind: "rows-still", strict: true, finding: "still-rows", via: "inbox", ways: ["first", "return", "sync", "shown"], rows: ".tf-main tbody tr", query: STILL_Q },
  ] },
  { page: "Time, today", route: "time", trips: [
    { id: "day-rows-still", kind: "rows-still", strict: true, finding: "still-rows", via: "inbox", words: "time", rows: ".tf-tm-row", query: STILL_Q },
  ] },
  { page: "Settings, Applications", route: "settings/checkouts", trips: [
    { id: "make-default", kind: "enter-twice", finding: "F-646", press: { sel: ".tf-main button", text: "Make default" } },
    { id: "default-undo", kind: "undo-stale", strict: true, finding: "F-648", changes: [{ sel: ".tf-stg-apps button", text: "Make default", row: "Terminal" }, { sel: ".tf-stg-apps button", text: "Make default", row: "Fork" }], value: "[...document.querySelectorAll('.tf-stg-apps tbody tr')].find((r) => [...r.querySelectorAll('.tf-chip')].some((c) => c.textContent.trim() === 'Default'))?.querySelector('b')?.textContent.trim() ?? null" },
  ] },
  { page: "Settings, Appearance", route: "settings/appearance", trips: [
    { id: "rail-top", kind: "rail-top", strict: true, finding: "F-650", scroller: ".tf-stg-scroll", press: { sel: ".tf-stg-nav a", text: "Keyboard" }, size: [1440, 620] },
  ] },
  { page: "Settings, Monitoring", route: "settings/monitoring", trips: [
    { id: "monitoring-fields", kind: "esc-sweep", finding: "F-641", typed: ["#tf-mon-slow", "#tf-mon-keep"] },
  ] },
  { page: "Settings, Performance", route: "settings/performance", trips: [
    { id: "performance-fields", kind: "esc-sweep", finding: "F-641", typed: ["#tf-perf-kept"] },
  ] },
  { page: "Settings, Backup", route: "settings/backup", trips: [
    { id: "backup-fields", kind: "esc-sweep", finding: "F-641", typed: ["#tf-bak-keep"] },
  ] },
  { page: "Settings, Renderers", route: "settings/renderers", trips: [
    { id: "renderers-fields", kind: "esc-sweep", strict: true, finding: "F-649" },
  ] },
  { page: "Tickets, List", route: "tickets", trips: [
    { id: "list-filter", kind: "column-filter", strict: true, finding: "F-151", prep: [{ click: "button, a, [role=tab]", text: "List" }, { wait: 500 }], filter: "payo", sizes: [[1440, 900], [1100, 800], [860, 760]] },
    { id: "list-pill", kind: "column-filter", strict: true, finding: "F-151", prep: [{ click: "button, a, [role=tab]", text: "List" }, { wait: 500 }], pill: { sel: ".tf-main button", text: "Bug" } },
  ] },
  { page: "A source", route: "settings/source/gitea", trips: [
    { id: "syncing-undo", kind: "undo-stale", strict: true, finding: "F-648", changes: [{ sel: ".tf-main button[aria-checked]" }, { sel: ".tf-main button[aria-checked]" }], value: "document.querySelector('.tf-main button[aria-checked]')?.getAttribute('aria-checked') ?? null" },
    // Owners waits for its Save, and its draft stays on Esc by design, so it is swept and not typed into.
    { id: "source-fields", kind: "esc-sweep", finding: "F-641" },
  ] },
  { page: "Compare", route: `${REPO}/compare/spike%2Fsepa-instant`, trips: [
    { id: "compare-fields", kind: "esc-sweep", finding: "F-255" },
    { id: "sent-link", kind: "enter-twice", finding: "F-264", prep: [{ wait: 900 }, { click: ".tf-cgo .tf-primary", text: "Create pull request" }, { wait: 800 }, { click: ".tf-tabs a", starts: "Branches" }, { wait: 1000 }], press: { sel: ".tf-sentlink" }, leaves: true },
  ] },
  { page: "Docs, SEPA", route: "page/confluence%3Asepa", trips: [
    { id: "unlink-still-reached", kind: "enter-twice", finding: "F-447", press: { sel: ".tf-rd-unlink", text: "Unlink" } },
    // Esc steps out to the composer's own box, which is what the rule asks of every field.
    { id: "docs-composer", kind: "esc-sweep", finding: "F-430", fields: ["textarea[aria-label='Comment on this page']"] },
    { id: "docs-comment", kind: "enter-twice", strict: true, finding: "F-266", prep: [{ fill: "textarea[aria-label='Comment on this page']", text: "Looks right to me." }, { wait: 150 }], press: { sel: ".tf-compose .tf-primary", text: "Comment" }, returns: "Comment on this page" },
    { id: "docs-section-link", kind: "enter-twice", strict: true, finding: "F-450", press: { sel: "#s-1 button[aria-label='Copy link to this section']" } },
    { id: "docs-create-page", kind: "enter-twice", strict: true, finding: "F-449", prep: [{ click: ".tf-main button", starts: "New page" }, { wait: 400 }, { fill: ".tf-dialog input[type=text], .tf-dialog input:not([type])", text: "Rollout checklist" }, { wait: 150 }], press: { sel: ".tf-dialog footer button", text: "Create page" }, returns: "New page…" },
    { id: "docs-sync-now", kind: "enter-twice", strict: true, finding: "F-448", prep: [{ click: ".tf-main button", text: "More" }, { wait: 250 }], press: { sel: "[role=menu] button, [role=menuitem]", text: "Sync now" }, returns: "More" },
    { id: "docs-for-you", kind: "enter-twice", strict: true, finding: "F-451", press: { sel: ".tf-dl-for a", starts: "SEPA payout retry design" }, leaves: true },
  ] },
  { page: "Docs, PSP codes", route: "page/confluence%3Apsp-codes", trips: [
    { id: "unlink-by-hand", kind: "enter-twice", finding: "F-433", prep: [{ click: ".tf-rd-head .tf-btn", text: "Link to PAY-231" }, { wait: 400 }], press: { sel: ".tf-rd-unlink", text: "Unlink" } },
  ] },
  { page: "Time, Day", route: "time", trips: [
    { id: "dismiss", kind: "enter-twice", finding: "F-340", press: { sel: ".tf-tm-row button", text: "Dismiss" }, rows: ".tf-tm-row" },
    { id: "add-to", kind: "enter-twice", finding: "F-340", press: { sel: ".tf-tm-row button", text: "Add to PAY-231" }, rows: ".tf-tm-row" },
    { id: "add-47m", kind: "enter-twice", finding: "F-340", press: { sel: ".tf-main button", starts: "Add 47m" } },
    { id: "delete-block", kind: "enter-twice", finding: "F-339", prep: [{ click: ".tf-tm-row button", text: "Edit…" }, { wait: 300 }], press: { sel: ".tf-dialog button", text: "Delete block" }, rows: ".tf-tm-row" },
    { id: "pointer-add-to", kind: "pointer-twice", strict: true, finding: "F-358", press: { sel: ".tf-tm-row button", text: "Add to PAY-231" } },
    { id: "pointer-assign-to", kind: "pointer-twice", strict: true, finding: "F-358", press: { sel: ".tf-tm-row button", text: "Assign to PAY-228" } },
    { id: "pointer-add-47m", kind: "pointer-twice", strict: true, finding: "F-358", press: { sel: ".tf-main button", starts: "Add 47m" } },
    { id: "aimed-after-close", kind: "aimed-after-close", strict: true, finding: "reconcile", open: { sel: ".tf-tm-row button", text: "Edit…" }, close: { sel: ".tf-dialog footer button", text: "Cancel" } },
    { id: "pointer-dismiss", kind: "pointer-twice", strict: true, finding: "F-358", press: { sel: ".tf-tm-row button", text: "Dismiss" } }, // Already holds its slot, so it shows the trip can pass.
    { id: "dismiss-week", kind: "leave-back", finding: "F-356", press: { sel: ".tf-tm-row button", text: "Dismiss" }, rows: ".tf-tm-row", via: "time/week" },
    { id: "dismiss-inbox", kind: "leave-back", finding: "F-356", press: { sel: ".tf-tm-row button", text: "Dismiss" }, rows: ".tf-tm-row", via: "inbox" },
    { id: "delete-block-week", kind: "leave-back", finding: "F-356", prep: [{ click: ".tf-tm-row button", text: "Edit…" }, { wait: 300 }], press: { sel: ".tf-dialog button", text: "Delete block" }, rows: ".tf-tm-row", via: "time/week" },
  ] },
  { page: "Inbox", route: "inbox", trips: [
    { id: "inbox-done", kind: "enter-twice", finding: "F-138", press: { sel: ".tf-inbox .tf-ib [data-act=done]" }, rows: ".tf-inbox .tf-ib" },
    { id: "inbox-ack", kind: "enter-twice", finding: "F-138", press: { sel: ".tf-inbox .tf-ib button", text: "Ack" }, rows: ".tf-inbox .tf-ib" },
    { id: "inbox-rerun", kind: "enter-twice", finding: "F-150", press: { sel: ".tf-inbox [data-act=rerun_build]" }, rows: ".tf-inbox .tf-ib" },
    { id: "inbox-done-fast", kind: "enter-fast", strict: true, finding: "F-156", press: { sel: ".tf-inbox .tf-ib [data-act=done]" }, rows: ".tf-inbox .tf-ib" },
    { id: "inbox-done-walks", kind: "enter-walks", strict: true, finding: "F-156", press: { sel: ".tf-inbox .tf-ib [data-act=done]" }, rows: ".tf-inbox .tf-ib" },
    { id: "inbox-done-monitors", kind: "leave-back", finding: "F-356", press: { sel: ".tf-inbox .tf-ib [data-act=done]" }, rows: ".tf-inbox .tf-ib", via: "monitors" },
    { id: "inbox-ack-monitors", kind: "leave-back", finding: "F-356", press: { sel: ".tf-inbox .tf-ib button", text: "Ack" }, rows: ".tf-inbox .tf-ib", via: "monitors" },
  ] },
  { page: "Monitors", route: "monitors", trips: [
    { id: "monitors-ack", kind: "enter-twice", finding: "F-351", press: { sel: ".tf-mon button.tf-mon-ack" }, rows: MONITOR_ROWS },
    { id: "monitors-hide", kind: "enter-twice", finding: "F-340", press: { sel: ".tf-mon tbody button", text: "Hide" }, rows: MONITOR_ROWS },
    { id: "monitors-pause", kind: "enter-twice", finding: "F-340", press: { sel: ".tf-mon tbody button", text: "Pause" }, rows: MONITOR_ROWS },
    { id: "monitors-hide-fast", kind: "enter-fast", strict: true, finding: "F-156", press: { sel: ".tf-mon tbody button", text: "Hide" }, rows: MONITOR_ROWS },
    { id: "monitors-hide-walks", kind: "enter-walks", strict: true, finding: "F-156", press: { sel: ".tf-mon tbody button", text: "Hide" }, rows: MONITOR_ROWS },
    { id: "monitors-hide-inbox", kind: "leave-back", finding: "F-356", press: { sel: ".tf-mon tbody button", text: "Hide" }, rows: MONITOR_ROWS, via: "inbox" },
    { id: "monitors-ack-pointer", kind: "pointer-twice", strict: true, once: true, finding: "F-351", press: { sel: ".tf-mon button.tf-mon-ack" } },
    { id: "monitors-pause-pointer", kind: "pointer-twice", strict: true, once: true, reveal: true, finding: "F-340", press: { sel: ".tf-mon tbody button", text: "Pause" } },
    // The README's picture: its query, and the widths the design works at.
    { id: "monitors-row-room", kind: "row-room", strict: true, finding: "PR #2479", query: "&fake-sources-healthy&fake-timer", table: ".tf-mon", cell: ".tf-mon-acts", sizes: [[1100, 900], [1240, 900], [1440, 900], [1920, 900]] },
  ] },
  { page: "Builds, a run", route: RUN, trips: [
    { id: "builds-rerun", kind: "enter-twice", finding: "F-338", press: { sel: ".tf-bhead .tf-primary" } },
    { id: "show-in-log", kind: "enter-twice", finding: "F-355", press: { sel: ".tf-problem a.tf-btn", text: "Show in log" }, leaves: true },
  ] },
  { page: "A repository's branches", route: `${REPO}/branches`, trips: [
    { id: "new-pr-branches", kind: "enter-twice", finding: "F-241", press: { sel: ".tf-main a.tf-btn", text: "New pull request" }, leaves: true },
  ] },
  { page: "A repository", route: `${REPO}/prs`, trips: [
    { id: "new-pr-repo", kind: "enter-twice", finding: "F-241", press: { sel: ".tf-main a.tf-btn", text: "New pull request" }, leaves: true },
  ] },
  { page: "A pull request", route: "pr/mock%3Apayout-service%23150", trips: [
    { id: "submit-review", kind: "enter-twice", finding: "F-254", prep: [{ click: ".tf-acts button", text: "Review…" }, { wait: 400 }, { click: ".tf-dialog input[value=approve]" }, { wait: 150 }], press: { sel: ".tf-dialog footer button", text: "Submit review" }, returns: "Review…" },
  ] },
  { page: "A pull request's conversation", route: "pr/mock%3Apayout-service%23142/conversation", trips: [
    { id: "pr-comment", kind: "enter-twice", finding: "F-266", prep: [{ fill: ".tf-compose textarea", text: "Looks right to me." }, { wait: 150 }], press: { sel: ".tf-compose .tf-primary", text: "Comment" }, returns: "Comment on PR #142" },
  ] },
  { page: "A pull request's diff", route: "pr/mock%3Apayout-service%23142/diff", trips: [
    { id: "reply-twice", kind: "enter-twice", strict: true, finding: "F-261", prep: [{ click: ".tf-thr-reply", text: "Reply…" }, { wait: 300 }, { fill: ".tf-reply textarea", text: "A cap at five tries." }, { wait: 150 }], press: { sel: ".tf-reply .tf-primary", text: "Reply" }, returns: "Reply…" },
    { id: "reply-opener-twice", kind: "enter-twice", strict: true, finding: "F-261", press: { sel: ".tf-thr-reply", text: "Reply…" } },
    { id: "resolve-twice", kind: "enter-twice", strict: true, finding: "F-261", press: { sel: ".tf-thread button", text: "Resolve" } },
    { id: "unresolve-twice", kind: "enter-twice", strict: true, finding: "F-261", press: { sel: ".tf-thread button", text: "Unresolve" } },
  ] },
  { page: "A ticket's connections", route: "ticket/mock%3APAY-231", trips: [
    { id: "ticket-comment", kind: "enter-twice", strict: true, finding: "F-266", prep: [{ fill: ".tf-compose textarea", text: "Looks right to me." }, { wait: 150 }], press: { sel: ".tf-compose .tf-primary", text: "Comment" }, returns: "Comment on PAY-231" },
    { id: "set-save", kind: "enter-twice", finding: "F-544", prep: [{ click: "button", starts: "Link something to it" }, { wait: 700 }, { click: "button", text: "Save as a link set…" }, { wait: 900 }], press: { sel: ".tf-dialog footer button", text: "Save link set" }, returns: "Save as a link set…" },
    { id: "set-save-leave", kind: "leave-back", strict: true, finding: "F-550", prep: [{ click: "button", starts: "Link something to it" }, { wait: 700 }, { click: "button", text: "Save as a link set…" }, { wait: 900 }], press: { sel: ".tf-dialog footer button", text: "Save link set" }, rows: ".tf-drawerhost .tf-lk", rowsStay: true, via: "inbox", reopen: { click: "button", starts: "Link something to it" } }, // The drawer closes on a leave, so its rows are read once it is open again.
    { id: "set-apply", kind: "enter-twice", finding: "F-536", prep: [{ click: "button", starts: "Link something to it" }, { wait: 700 }, { click: "button", text: "Apply a link set…" }, { wait: 700 }, { click: ".tf-dialog input[type=radio]:not(:disabled)" }, { wait: 150 }], press: { sel: ".tf-dialog footer .tf-primary" }, returns: "Apply a link set…" },
  ] },
  { page: "A link set", route: "sets/payout-stack", trips: [
    { id: "set-add", kind: "enter-twice", finding: "F-547", prep: [{ click: "article[aria-label='Pages'] .tf-set-add button", text: "Add…" }, { wait: 700 }, { click: ".tf-dialog .tf-picks input:not(:disabled)" }, { wait: 150 }], press: { sel: ".tf-dialog footer button", text: "Add" }, returns: "Add…" },
  ] },
  { page: "An asset", route: "asset/asset%3Aknobas-gitea", trips: [
    { id: "asset-move", kind: "enter-twice", finding: "F-545", prep: [{ click: "button", text: "Move…" }, { wait: 700 }, { click: ".tf-dialog .picker button" }, { wait: 300 }], press: { sel: ".tf-dialog footer .tf-primary", starts: "Move into" }, returns: "Move…" },
    { id: "asset-rename", kind: "enter-twice", finding: "F-545", prep: [{ click: "button", text: "Rename…" }, { wait: 400 }, { fill: "input[aria-label=Name]", text: "-renamed" }, { wait: 100 }], press: { sel: "input[aria-label=Name]" }, returns: "Rename…" },
    { id: "asset-add", kind: "enter-twice", finding: "F-547", prep: [{ click: ".col-h .plus" }, { wait: 600 }, { fill: ".tf-dialog input[type=text]", text: "New child" }, { eval: "(() => { const t = document.querySelector('.tf-dialog select'); t.value = [...t.options].find((o) => o.value && !o.disabled).value; t.dispatchEvent(new Event('change', { bubbles: true })); })()" }, { wait: 150 }], press: { sel: ".tf-dialog footer .tf-primary", text: "Create" }, leaves: true, returns: "New asset in Estate" }, // Passes with --gap too: Enter on this + opens nothing, even on a fresh page.,
  ] },
  { page: "Standup", route: "time/standup", trips: [
    { id: "publish", kind: "enter-twice", finding: "F-340", press: { sel: ".tf-su-proto .tf-primary" } },
    { id: "standup-notes", kind: "esc-sweep", finding: "F-255", fields: ["textarea[aria-label='Notes for Mara Lindqvist']"] },
    { id: "publish-again", kind: "enter-twice", finding: "F-340", prep: [{ click: ".tf-su-proto .tf-primary" }, { wait: 800 }, { fill: "textarea[aria-label='Notes for Mara Lindqvist']", text: "One more line." }, { wait: 300 }], press: { sel: ".tf-su-proto button", starts: "Publish again" } },
  ] },
  { page: "Inbox, by mouse", route: "inbox", trips: [
    { id: "inbox-walk-done", kind: "walk-done", strict: true, finding: "F-159", press: { sel: ".tf-inbox .tf-ib [data-act=done]" }, rows: ".tf-inbox .tf-ib" },
  ] },
  { page: "Time, Day, by mouse", route: "time", trips: [
    { id: "time-walk-dismiss", kind: "walk-done", strict: true, finding: "F-159", press: { sel: ".tf-tm-row button", text: "Dismiss" }, rows: ".tf-tm-row" },
  ] },
  { page: "Dialogs that start on a select", route: "page/confluence%3Asepa", trips: [
    { id: "type-new-page-here", kind: "dialog-typing", strict: true, finding: "F-453", prep: [{ click: ".tf-rd-head button", text: "New page…" }, { wait: 500 }], word: "Finance rota" }, // F-457: both Docs buttons now read New page…, and this one is the page header's.
    { id: "type-new-page", kind: "dialog-typing", strict: true, finding: "F-453", prep: [{ click: ".tf-main button", text: "New page…" }, { wait: 500 }], word: "Finance rota" },
  ] },
  { page: "Dialogs that start on a select", route: RUN, trips: [
    // A failed run fills the title in, so the dialog waits on its project, and it arrives there (reconcile, round 15). Typing toward a project is what's expected.
    { id: "type-create-ticket", kind: "dialog-typing", strict: true, finding: "F-453", stays: true, prep: [{ click: ".tf-main button", text: "Create ticket…" }, { wait: 500 }] },
  ] },
  { page: "Dialogs that start on a select", route: "sets/payout-stack", trips: [
    // The plan leaves this one to fixer 1. If it stays on its select, say so here with `stays: true`.
    { id: "type-set-pattern", kind: "dialog-typing", strict: true, finding: "F-453", prep: [{ click: ".tf-main button", text: "Add a pattern…" }, { wait: 500 }] },
  ] },
  { page: "Dialogs settled to start on a select", route: "build/teamcity%3AbuildType%3APayout_IntegrationTests", trips: [
    { id: "stays-run", kind: "dialog-typing", strict: true, finding: "F-453", stays: true, prep: [{ click: ".tf-main button", text: "Run…" }, { wait: 500 }] },
  ] },
  { page: "Dialogs settled to start on a select", route: "asset/asset%3Aknobas-gitea", trips: [
    { id: "stays-import", kind: "dialog-typing", strict: true, finding: "F-453", stays: true, prep: [{ click: ".tf-main button", text: "Import…" }, { wait: 500 }] },
  ] },
  { page: "Opening a kept page, Docs", route: "page/confluence%3Asepa", trips: [
    { id: "kept-docs-other", kind: "kept-opening", strict: true, finding: "F-454", then: "other", scroller: ".tf-rd-main", via: "inbox", other: "page/confluence%3Apsp-codes" },
    { id: "kept-docs-launcher", kind: "kept-opening", strict: true, finding: "F-067", then: "launcher", scroller: ".tf-rd-main", via: "tickets", other: "page/confluence%3Apsp-codes", words: "PSP error" },
    { id: "kept-docs-same", kind: "kept-opening", strict: true, finding: "F-454", then: "same", scroller: ".tf-rd-main", via: "inbox" },
    { id: "kept-docs-esc", kind: "kept-opening", strict: true, finding: "F-454", then: "esc", scroller: ".tf-rd-main", other: "page/confluence%3Apsp-codes" },
    { id: "kept-docs-back", kind: "kept-opening", strict: true, finding: "F-454", then: "back", scroller: ".tf-rd-main", other: "page/confluence%3Apsp-codes" },
    { id: "kept-docs-continue", kind: "kept-opening", strict: true, finding: "F-431", then: "continue", scroller: ".tf-rd-main", press: { sel: "section[aria-label='Continue reading'] a.tf-dh-row" } },
    { id: "kept-docs-continue-away", kind: "kept-opening", strict: true, finding: "F-431", then: "continue", scroller: ".tf-rd-main", via: "inbox", press: { sel: "section[aria-label='Continue reading'] a.tf-dh-row" } },
  ] },
  { page: "Opening a kept page, Docs", route: "page/confluence%3Apsp-codes", trips: [
    { id: "kept-docs-section", kind: "kept-opening", strict: true, finding: "F-454", then: "landing", scroller: ".tf-rd-main", via: "inbox", other: "page/confluence%3Asepa/2", to: "#s-2", top: true, at: 300 },
    { id: "kept-docs-mention", kind: "kept-opening", strict: true, finding: "F-431", then: "landing", scroller: ".tf-rd-main", via: "inbox", other: "page/confluence%3Asepa/mention", to: ".tf-msg.tf-mention", at: 300 },
    { id: "kept-docs-for-you", kind: "kept-opening", strict: true, finding: "F-431", then: "landing", scroller: ".tf-rd-main", via: "inbox", other: "page/confluence%3Asepa/mention", press: { sel: ".tf-dl-for a", starts: "SEPA payout retry design" }, to: ".tf-msg.tf-mention", at: 300 },
  ] },
  { page: "Opening a kept page, an asset", route: "asset/asset%3Aknobas-gitea", trips: [
    { id: "kept-asset-other", kind: "kept-opening", strict: true, finding: "F-454", then: "other", scroller: "aside.pane", via: "inbox", other: "asset/asset%3Aknobas-jira" },
    { id: "kept-asset-same", kind: "kept-opening", strict: true, finding: "F-454", then: "same", scroller: "aside.pane", via: "inbox" },
    { id: "kept-asset-esc", kind: "kept-opening", strict: true, finding: "F-454", then: "esc", scroller: "aside.pane", other: "ticket/mock%3APAY-231" }, // Esc on an asset goes up the tree, so this one comes back from what it opened.
    { id: "kept-asset-back", kind: "kept-opening", strict: true, finding: "F-454", then: "back", scroller: "aside.pane", other: "asset/asset%3Aknobas-jira" },
  ] },
  // Fixer 2's, moved here in the reconcile. `once`: a second press must not ask for a second run. A run the fake holds going keeps the control off.
  { page: "A source's Sync now", route: "settings/source/teamcity", trips: [
    { id: "source-sync-now", kind: "enter-twice", strict: true, finding: "F-652", press: { sel: ".tf-main button", text: "Sync now" } },
    { id: "source-sync-now-pointer", kind: "pointer-twice", strict: true, once: true, finding: "F-652", press: { sel: ".tf-main button", text: "Sync now" } },
  ] },
  { page: "Settings' Sync all", route: "settings/sources", trips: [
    { id: "sync-all", kind: "enter-twice", strict: true, finding: "F-652", press: { sel: ".tf-main button", text: "Sync all" } },
    { id: "sync-all-pointer", kind: "pointer-twice", strict: true, once: true, finding: "F-652", press: { sel: ".tf-main button", text: "Sync all" } },
  ] },
  // *Clone all…* (PR #2479): the opener says first in a dialog, its *Clone* starts one run however it is pressed, and the run's one
  // summary toast stands through a leave and a return. `once`: a second press must not ask for a second run.
  { page: "Code's Clone all", route: "code", trips: [
    { id: "clone-all-open", kind: "enter-twice", strict: true, opens: true, finding: "PR #2479", press: { sel: ".tf-fhead button", text: "Clone all…" } },
    { id: "clone-all-start", kind: "enter-twice", strict: true, finding: "PR #2479", prep: [{ click: ".tf-fhead button", text: "Clone all…" }, { wait: 400 }], press: { sel: ".tf-dialog footer .tf-primary", starts: "Clone" }, returns: "Cloning ", returnsCounting: true },
    { id: "clone-all-start-pointer", kind: "pointer-twice", strict: true, once: true, finding: "PR #2479", prep: [{ click: ".tf-fhead button", text: "Clone all…" }, { wait: 400 }], press: { sel: ".tf-dialog footer .tf-primary", starts: "Clone" } },
    { id: "clone-all-toast-back", kind: "leave-back", strict: true, finding: "PR #2479", prep: [{ click: ".tf-fhead button", text: "Clone all…" }, { wait: 400 }], press: { sel: ".tf-dialog footer .tf-primary", starts: "Clone" }, rows: ".tf-main tbody tr", rowsStay: true, via: "inbox", after: 4500 },
  ] },
  { page: "Settings' Clone all", route: "settings/checkouts", trips: [
    { id: "settings-clone-all-open", kind: "enter-twice", strict: true, opens: true, finding: "PR #2479", press: { sel: ".tf-main button", text: "Clone all…" } },
  ] },
  // A press inside one kept page isn't a hide: its rail or column stays where it was.
  { page: "A rail through a press", route: "builds/teamcity%3AbuildType%3APayout_Lint", trips: [
    { id: "kept-builds-rail", kind: "kept-opening", strict: true, finding: "F-454", then: "press", scroller: ".tf-pl-list", at: 600, press: { sel: ".tf-bmain a[href^='#/build/']" } },
  ] },
  { page: "A rail through a press", route: "page/confluence%3Asepa", trips: [
    { id: "kept-docs-tree", kind: "kept-opening", strict: true, finding: "F-454", then: "press", size: [1440, 420], scroller: ".tf-dl-body", at: 150, press: { sel: ".tf-dl-leaf", starts: "PSP error" } },
  ] },
  { page: "A rail through a press", route: "asset/asset%3Aknobas-gitea", trips: [
    { id: "kept-asset-column", kind: "kept-opening", strict: true, finding: "F-454", then: "press", size: [1440, 460], scroller: "ol.col[data-col='4']", at: 200, press: { sel: "ol.col[data-col='4'] button.row", starts: "Ct knobas-uptime-kuma" } },
  ] },
  // Round 17: the box you pressed in holds the keys. Each row is one place the promise is made, in the plan's table and the text.
  { page: "Keys, a Docs page", route: "page/confluence%3Asepa", trips: [
    { id: "scroller-keys-docs", kind: "scroller-keys", strict: true, finding: "F-463", how: "text", scroller: ".tf-rd-main" },
    { id: "scroller-keys-docs-twice", kind: "scroller-keys", strict: true, finding: "F-463", how: "twice", keys: ["PageDown"], scroller: ".tf-rd-main" },
    { id: "scroller-keys-docs-address", kind: "scroller-keys", strict: true, finding: "F-463", how: "address", scroller: ".tf-rd-main" },
    { id: "scroller-keys-docs-nav", kind: "scroller-keys", strict: true, finding: "F-463", how: "nav", section: "Docs", keys: ["PageDown"], scroller: ".tf-rd-main" },
    { id: "scroller-tab-docs", kind: "scroller-tab", strict: true, finding: "F-463", scroller: ".tf-rd-main" },
    { id: "scroller-select-docs", kind: "scroller-select", strict: true, finding: "F-463", scroller: ".tf-rd-main", in: ".tf-doc" },
  ] },
  { page: "Keys, a pipeline", route: "builds/teamcity%3AbuildType%3ADataPipeline_Build", trips: [
    { id: "scroller-keys-pipeline", kind: "scroller-keys", strict: true, finding: "F-463", how: "address", keys: ["PageDown", " "], scroller: ".tf-bmain", size: [1440, 460] },
  ] },
  { page: "Keys, a page with no inner scroller", route: "monitors", trips: [
    { id: "scroller-keys-page", kind: "scroller-keys", strict: true, finding: "F-463", how: "address", keys: ["PageDown"], scroller: ".tf-main", size: [1440, 400] },
  ] },
  { page: "Keys, a Docs tile", route: "docs", trips: [
    { id: "scroller-keys-docs-tile", kind: "scroller-keys", strict: true, finding: "F-463", how: "tile", tile: { sel: ".tf-main a[href^='#/page/']", starts: "SEPA" }, scroller: ".tf-rd-main" },
  ] },
  { page: "Keys, a run", route: RUN, trips: [
    { id: "scroller-keys-run", kind: "scroller-keys", strict: true, finding: "F-463", how: "text", scroller: ".tf-bmain" },
    { id: "scroller-keys-run-address", kind: "scroller-keys", strict: true, finding: "F-463", how: "address", keys: ["PageDown"], scroller: ".tf-bmain" },
    { id: "scroller-keys-run-nav", kind: "scroller-keys", strict: true, finding: "F-463", how: "nav", section: "Builds", keys: ["PageDown"], scroller: ".tf-bmain" },
    { id: "scroller-tab-run", kind: "scroller-tab", strict: true, finding: "F-463", scroller: ".tf-bmain" },
    { id: "scroller-select-run", kind: "scroller-select", strict: true, finding: "F-463", scroller: ".tf-bmain" },
  ] },
  { page: "Keys, Settings", route: "settings/appearance", trips: [
    { id: "scroller-keys-settings", kind: "scroller-keys", strict: true, finding: "F-463", how: "text", scroller: ".tf-stg-scroll", size: [1440, 420] },
    { id: "scroller-tab-settings", kind: "scroller-tab", strict: true, finding: "F-463", scroller: ".tf-stg-scroll" },
    { id: "scroller-keys-settings-address", kind: "scroller-keys", strict: true, finding: "F-463", how: "address", keys: ["PageDown"], scroller: ".tf-stg-scroll", size: [1440, 420] },
  ] },
  { page: "Keys, a ticket", route: "ticket/mock%3APAY-231", trips: [
    { id: "scroller-keys-ticket", kind: "scroller-keys", strict: true, finding: "F-463", how: "text", scroller: ".tf-work" },
    { id: "scroller-keys-ticket-address", kind: "scroller-keys", strict: true, finding: "F-463", how: "address", keys: ["PageDown"], scroller: ".tf-work" },
    { id: "scroller-tab-ticket", kind: "scroller-tab", strict: true, finding: "F-463", scroller: ".tf-work" },
    { id: "scroller-select-ticket", kind: "scroller-select", strict: true, finding: "F-463", scroller: ".tf-work" },
  ] },
  { page: "Keys, the asset pane", route: "asset/asset%3Aknobas-gitea", trips: [
    { id: "pane-keys-text", kind: "pane-keys", strict: true, finding: "F-564", how: "text" },
    { id: "pane-keys-blank", kind: "pane-keys", strict: true, finding: "F-564", how: "blank" },
    { id: "pane-keys-wheel", kind: "pane-keys", strict: true, finding: "F-564", how: "wheel" },
    { id: "pane-keys-narrow", kind: "pane-keys", strict: true, finding: "F-564", how: "text", size: [860, 460] },
    { id: "pane-keys-link", kind: "pane-keys", strict: true, finding: "F-564", how: "link", link: { sel: "aside.pane button.link" } },
    { id: "pane-keys-row", kind: "pane-keys", strict: true, finding: "F-564", how: "row", walks: true, size: [1440, 460], row: { sel: "ol.col[data-col='4'] button.row" } },
    { id: "pane-keys-column", kind: "pane-keys", strict: true, finding: "F-564", how: "column", walks: true },
    { id: "pane-esc", kind: "pane-esc", strict: true, finding: "F-564" },
    { id: "pane-esc-narrow", kind: "pane-esc", strict: true, finding: "F-564", size: [860, 460] },
    { id: "pane-keys-address", kind: "pane-keys", strict: true, finding: "F-562", how: "address", walks: true },
  ] },
  // Round 17's way back: a row brings the item back, Docs and Settings are sections with a place, and j keeps the place. The edges are rows too.
  { page: "Way back, a run", route: "builds/teamcity%3AbuildType%3APayout_IntegrationTests", trips: [
    { id: "nav-reopen-log", kind: "nav-reopen", strict: true, finding: "F-371", left: `${RUN}/log`, bare: RUN, section: "Builds", scroller: ".tf-bmain", at: 500, row: { sel: `.tf-bmain .tf-run-no a[href='#/${RUN}']` } },
    { id: "nav-reopen-log-tr", kind: "nav-reopen", strict: true, finding: "F-371", left: `${RUN}/log`, bare: RUN, section: "Builds", scroller: ".tf-bmain", at: 500, how: ["pointer"], row: { sel: `.tf-bmain tr:has(a[href='#/${RUN}']) .tf-run-dur` } },
    { id: "nav-reopen-log-entry", kind: "nav-reopen", strict: true, finding: "F-371", left: `${RUN}/log`, bare: RUN, via: null, section: "Builds", scroller: ".tf-bmain", at: 500, row: { sel: `.tf-bmain .tf-run-no a[href='#/${RUN}']` } },
    { id: "nav-reopen-overview", kind: "nav-reopen", strict: true, finding: "F-371", left: RUN, bare: RUN, section: "Builds", scroller: ".tf-bmain", at: 300, how: ["pointer"], row: { sel: `.tf-bmain .tf-run-no a[href='#/${RUN}']` } },
    { id: "nav-reopen-log-other", kind: "nav-reopen", strict: true, finding: "F-371", then: "other", left: `${RUN}/log`, bare: RUN, section: "Builds", scroller: ".tf-bmain", at: 500, how: ["pointer"], row: { sel: `.tf-bmain .tf-run-no a[href='#/${RUN}']` }, other: { sel: `.tf-bmain a[href^='#/build/']:not([href*='-1187'])` } },
    { id: "nav-reopen-log-tab", kind: "nav-reopen", strict: true, finding: "F-371", then: "tab", left: `${RUN}/tests`, bare: RUN, section: "Builds", scroller: ".tf-bmain", at: 200, how: ["pointer"], row: { sel: `.tf-bmain .tf-run-no a[href='#/${RUN}']` }, tab: { sel: `.tf-bmain a[href='#/${RUN}/log']` }, tabTo: `${RUN}/log` },
    { id: "nav-reopen-log-back", kind: "nav-reopen", strict: true, finding: "F-371", then: "back", left: `${RUN}/log`, bare: RUN, section: "Builds", scroller: ".tf-bmain", at: 500, how: ["pointer"], sizes: [[1440, 560]], row: { sel: `.tf-bmain .tf-run-no a[href='#/${RUN}']` } },
    { id: "nav-reopen-log-far", kind: "nav-reopen", strict: true, finding: "F-371", left: `${RUN}/log`, bare: RUN, section: "Builds", scroller: ".tf-bmain", at: 500, how: ["pointer"], sizes: [[1440, 560]], far: ["Tickets", "Inbox", "Code", "Docs", "Monitors", "Link sets", "Time"], via: null, row: { sel: `.tf-bmain .tf-run-no a[href='#/${RUN}']` } },
    { id: "nav-reopen-log-find", kind: "nav-reopen", strict: true, finding: "F-371", left: `${RUN}/log`, bare: RUN, section: "Builds", scroller: ".tf-bmain", at: 300, how: ["pointer"], sizes: [[1440, 560]], fill: { sel: "input[aria-label='Find in log']", text: "error" }, row: { sel: `.tf-bmain .tf-run-no a[href='#/${RUN}']` } },
  ] },
  { page: "Way back, a pull request", route: "code/prs", trips: [
    { id: "nav-reopen-pr-launcher", kind: "nav-reopen", strict: true, finding: "F-274", then: "launcher", left: "pr/mock%3Apayout-service%23142/diff", bare: "pr/mock%3Apayout-service%23142", section: "Code", scroller: ".tf-main", at: 600, how: ["pointer"], sizes: [[1440, 560]], words: "idempotent", row: { sel: ".tf-main a.tf-rowt[href='#/pr/mock%3Apayout-service%23142']" } },
    { id: "nav-reopen-pr", kind: "nav-reopen", strict: true, finding: "F-274", left: "pr/mock%3Apayout-service%23142/diff", bare: "pr/mock%3Apayout-service%23142", section: "Code", scroller: ".tf-main", at: 600, row: { sel: ".tf-main a.tf-rowt[href='#/pr/mock%3Apayout-service%23142']" } },
  ] },
  { page: "Way back, a repository", route: "code/repos", trips: [
    { id: "nav-reopen-repo", kind: "nav-reopen", strict: true, finding: "F-274", left: `${REPO}/files/README.md`, bare: REPO, section: "Code", scroller: ".tf-main", at: 600, row: { sel: `.tf-main tr a[href='#/${REPO}']` } },
  ] },
  { page: "Way back, Docs", route: "docs", trips: [
    { id: "nav-return-docs-folds", kind: "nav-docs", strict: true, finding: "F-464", then: "folds" },
  ] },
  { page: "Way back, a Docs search", route: "docs/search/retry", trips: [
    { id: "nav-return-docs-search", kind: "nav-docs", strict: true, finding: "F-464", then: "search" },
  ] },
  { page: "Way back, Settings", route: "settings/appearance", trips: [
    { id: "gear-return", kind: "gear-return", strict: true, finding: "F-661", source: "settings/source/jira" },
  ] },
  { page: "Way back, the walk", route: "inbox", trips: [
    { id: "walk-place-inbox", kind: "walk-place", strict: true, finding: "F-075", section: "Inbox", scroller: ".tf-main", at: 200, rows: ".tf-ib[data-row]" },
  ] },
  { page: "Way back, the Tickets walk", route: "tickets", trips: [
    // Only with nothing selected: a switch to List selects the first ticket, and a selected ticket is where the walk starts, even scrolled away.
    { id: "walk-place-board", kind: "walk-place", strict: true, finding: "F-075", section: "Tickets", scroller: ".tf-cols", at: 200, rows: ".tf-tickets [data-entity]" },
  ] },
  { page: "Keys, the next action item", route: "time/standup", trips: [
    { id: "field-in-view-item", kind: "field-in-view", strict: true, finding: "F-372", open: { sel: "button[data-su-add='item']", text: "Add an action item" }, field: "New action item", times: 5 },
    { id: "field-in-view-item-1100", kind: "field-in-view", strict: true, finding: "F-372", open: { sel: "button[data-su-add='item']", text: "Add an action item" }, field: "New action item", times: 5, size: [1100, 620] },
  ] },
  { page: "Keys, Test connection", route: "settings/source/gitea", trips: [
    { id: "press-holds-test", kind: "press-holds", strict: true, finding: "F-658", press: { sel: ".tf-stg-acts button", text: "Test connection" }, line: ".tf-stg-note" },
  ] },
  // Round 16's way back: every row of the plan's table, by address and by a nav press. A launcher jump, a link and Esc stay with the kept-* trips above.
  { page: "Nav return, a pipeline", route: "builds/teamcity%3AbuildType%3ADataPipeline_Build", trips: [
    { id: "nav-return-pipeline", kind: "nav-return", strict: true, finding: "F-370", section: "Builds", scroll: { ".tf-bmain": 300, ".tf-pl-list": 115 } },
    { id: "nav-return-run", kind: "nav-return", strict: true, finding: "F-370", section: "Builds", scroll: { ".tf-bmain": 300, ".tf-pl-list": 115 }, steps: [{ press: { sel: ".tf-bmain a[href^='#/build/']:not(svg a)" } }, { scroll: { ".tf-pl-list": 415 } }] },
    { id: "nav-return-same", kind: "nav-return", strict: true, finding: "F-370", section: "Builds", same: true, scroll: { ".tf-bmain": 300, ".tf-pl-list": 115 } },
    { id: "nav-return-card", kind: "nav-return", strict: true, finding: "F-370", section: "Builds", scroll: { ".tf-bmain": 300 }, steps: [{ go: "ticket/mock%3APAY-231" }, { eval: "[...document.querySelectorAll('.tf-tksecs a')].find((a) => a.textContent.trim().startsWith('Builds')).click()" }] },
  ] },
  { page: "Nav return, Builds", route: "builds", trips: [
    { id: "nav-return-builds", kind: "nav-return", strict: true, finding: "F-370", section: "Builds", scroll: { ".tf-bmain": 300, ".tf-pl-list": 115 } },
    // The entry from a run goes to the pipeline's own address, and bare #/builds is that same page (reconcile, round 16).
    { id: "nav-return-builds-run", kind: "nav-return", strict: true, finding: "F-370", section: "Builds", scroll: { ".tf-bmain": 300, ".tf-pl-list": 115 }, steps: [{ press: { sel: ".tf-bmain a[href^='#/build/']:not(svg a)" } }] },
    { id: "nav-return-builds-run-same", kind: "nav-return", strict: true, finding: "F-370", section: "Builds", same: true, scroll: { ".tf-bmain": 300, ".tf-pl-list": 115 }, steps: [{ press: { sel: ".tf-bmain a[href^='#/build/']:not(svg a)" } }] },
  ] },
  { page: "Nav return, entered by a nav press", route: "inbox", trips: [
    { id: "nav-return-builds-entered", kind: "nav-return", strict: true, finding: "F-370", section: "Builds", enter: true, scroll: { ".tf-bmain": 300, ".tf-pl-list": 115 } },
    { id: "nav-return-board-entered", kind: "nav-return", strict: true, finding: "F-071", section: "Tickets", enter: true, scroll: { ".tf-cols": 200 } },
    { id: "nav-return-docs-entered", kind: "nav-return", strict: true, finding: "F-071", section: "Docs", via: "Inbox", enter: true, scroll: { ".tf-rd-main": 227 } },
    { id: "nav-return-inbox-first", kind: "nav-return", strict: true, finding: "F-071", section: "Inbox", scroll: { ".tf-main": 200 } },
    { id: "nav-return-wheel", kind: "nav-return", strict: true, finding: "F-370", section: "Inbox", scroll: { ".tf-main": 150 }, wheel: 100 },
  ] },
  { page: "Nav return, Tickets", route: "tickets", trips: [
    { id: "nav-return-board", kind: "nav-return", strict: true, finding: "F-071", section: "Tickets", scroll: { ".tf-cols": 200 } },
    { id: "nav-return-list", kind: "nav-return", strict: true, finding: "F-071", section: "Tickets", prep: [{ click: "button, a, [role=tab]", text: "List" }, { wait: 500 }], scroll: { ".tf-board-wrap": 300 } },
    { id: "nav-return-ticket", kind: "nav-return", strict: true, finding: "F-071", section: "Tickets", scroll: { ".tf-cols": 200 }, steps: [{ go: "ticket/mock%3APAY-231" }] },
  ] },
  { page: "Nav return, Code", route: "code", trips: [
    { id: "nav-return-code", kind: "nav-return", strict: true, finding: "F-071", section: "Code", scroll: { ".tf-main": 150 } },
  ] },
  { page: "Nav return, Pull requests", route: "code/prs", trips: [
    { id: "nav-return-prs", kind: "nav-return", strict: true, finding: "F-071", section: "Code", scroll: { ".tf-main": 300 } },
    { id: "nav-return-pr", kind: "nav-return", strict: true, finding: "F-071", section: "Code", scroll: { ".tf-main": 300 }, steps: [{ press: { sel: ".tf-main a[href^='#/pr/']" } }] },
  ] },
  { page: "Nav return, Monitors", route: "monitors", trips: [
    { id: "nav-return-monitors", kind: "nav-return", strict: true, finding: "F-071", section: "Monitors", sizes: [[1440, 400]], scroll: { ".tf-main": 100 } },
  ] },
  { page: "Nav return, Day", route: "time", trips: [
    { id: "nav-return-day", kind: "nav-return", strict: true, finding: "F-071", section: "Time", scroll: { ".tf-main": 300 } },
  ] },
  { page: "Nav return, Week", route: "time/week", trips: [
    { id: "nav-return-week", kind: "nav-return", strict: true, finding: "F-071", section: "Time", scroll: { ".tf-main": 200 } },
  ] },
  { page: "Nav return, Docs", route: "docs", trips: [
    { id: "nav-return-docs", kind: "nav-return", strict: true, finding: "F-071", section: "Docs", via: "Inbox", scroll: { ".tf-rd-main": 227 } },
  ] },
  { page: "Nav return, Link sets", route: "sets", trips: [
    { id: "nav-return-sets", kind: "nav-return", strict: true, finding: "F-071", section: "Link sets", scroll: { ".tf-main": 150 } },
  ] },
  // Round 16's hold: the toast keeps it, and the gutter is the list.
  { page: "A hold, Inbox", route: "inbox", trips: [
    { id: "hold-to-undo-inbox", kind: "hold-to-undo", strict: true, finding: "F-163", press: { sel: ".tf-inbox .tf-ib [data-act=done]" }, rows: ".tf-inbox .tf-ib, .tf-ishelf > summary", sizes: [[1440, 900], [1100, 760]] },
    { id: "hold-to-snoozed-inbox", kind: "hold-to-undo", strict: true, finding: "F-163", press: { sel: ".tf-inbox .tf-ib [data-act=done]" }, rows: ".tf-inbox .tf-ib, .tf-ishelf > summary", aim: ".tf-ishelf > summary", opens: ".tf-ishelf[open]", sizes: [[1440, 900]] },
    { id: "hold-gutter-inbox", kind: "hold-gutter", strict: true, finding: "F-164", press: { sel: ".tf-inbox .tf-ib [data-act=done]" }, rows: ".tf-inbox .tf-ib", list: ".tf-inbox" },
    { id: "hold-leave-inbox", kind: "hold-leave", strict: true, finding: "F-163", press: { sel: ".tf-inbox .tf-ib [data-act=done]" }, rows: ".tf-inbox .tf-ib", section: "Inbox", via: "Monitors" },
  ] },
  { page: "A hold, Day", route: "time", trips: [
    { id: "hold-to-undo-day", kind: "hold-to-undo", strict: true, finding: "F-163", press: { sel: ".tf-tm-row button", text: "Dismiss" }, rows: ".tf-tm-list > .tf-tm-row", sizes: [[1440, 900], [1100, 760]] },
    { id: "hold-gutter-day", kind: "hold-gutter", strict: true, finding: "F-164", press: { sel: ".tf-tm-row button", text: "Dismiss" }, rows: ".tf-tm-list > .tf-tm-row", list: ".tf-tm-list" },
    { id: "hold-leave-day", kind: "hold-leave", strict: true, finding: "F-163", press: { sel: ".tf-tm-row button", text: "Dismiss" }, rows: ".tf-tm-list > .tf-tm-row", section: "Time", via: "Inbox" },
  ] },
  { page: "A ticket's composer", route: "ticket/mock%3APAY-231", trips: [
    { id: "comment-view-key", kind: "comment-view", strict: true, finding: "F-162", how: "key" },
    { id: "comment-view-pointer", kind: "comment-view", strict: true, finding: "F-162", how: "pointer" },
  ] },
  { page: "A Docs page's composer", route: "page/confluence%3Asepa", trips: [
    { id: "comment-view-docs-key", kind: "comment-view", strict: true, finding: "F-162", how: "key" },
    { id: "comment-view-docs-pointer", kind: "comment-view", strict: true, finding: "F-162", how: "pointer" },
  ] },
  { page: "A pull request's composer", route: "pr/mock%3Apayout-service%23142/conversation", trips: [
    { id: "comment-view-pr-key", kind: "comment-view", strict: true, finding: "F-162", how: "key", size: [1100, 620] },
    { id: "comment-view-pr-pointer", kind: "comment-view", strict: true, finding: "F-162", how: "pointer", size: [1100, 620] },
  ] },
  { page: "An asset's column", route: "asset/asset%3Aknobas-gitea", trips: [
    { id: "strip-walk-gitea", kind: "strip-walk", strict: true, finding: "F-556", row: "knobas-gitea", sizes: [[1440, 900], [1241, 800], [1100, 800]] },
    { id: "strip-press-jira", kind: "strip-press", strict: true, finding: "F-560", row: "knobas-jira", sizes: [[1440, 900], [1241, 800], [1100, 800]] },
  ] },
  { page: "Add a source", route: "settings/sources", trips: [
    { id: "add-source-fold", kind: "add-source-fold", strict: true, finding: "F-656", prep: [{ click: "button", starts: "Add a source" }, { wait: 800 }], sizes: [[1440, 900], [1100, 620], [860, 700]] },
  ] },
  { page: "Find key, a run's Log", route: `${RUN}/log`, trips: [
    { id: "find-key-log", kind: "find-key", strict: true, finding: "F-072", box: "Find in log" },
  ] },
  { page: "Find key, a file", route: `${REPO}/files/src/lib.rs`, trips: [
    { id: "find-key-file", kind: "find-key", strict: true, finding: "F-267", box: "Find in file" },
  ] },
  { page: "Find key, Tickets", route: "tickets", trips: [
    { id: "find-key-tickets", kind: "find-key", strict: true, finding: "F-072", box: "Filter tickets" },
    { id: "find-key-dialog", kind: "find-key", strict: true, finding: "F-072", nothing: true, prep: [{ click: "button", text: "New view…" }, { wait: 400 }] },
  ] },
  { page: "Find key, Docs home", route: "docs", trips: [
    { id: "find-key-docs-home", kind: "find-key", strict: true, finding: "F-460", box: "Search or act", seed: true },
  ] },
  { page: "Find key, a Docs space", route: "docs/space/ENG", trips: [
    { id: "find-key-docs-space", kind: "find-key", strict: true, finding: "F-460", box: "Search or act", seed: true },
  ] },
  { page: "Find key, a Docs page", route: "page/confluence%3Asepa", trips: [
    { id: "find-key-docs-page", kind: "find-key", strict: true, finding: "F-458", box: "Find in page", words: "retry", count: "3 of 4" },
  ] },
  { page: "Find key, no box", route: "inbox", trips: [
    { id: "find-key-none", kind: "find-key", strict: true, finding: "F-074", box: "Search or act", seed: true },
  ] },
  { page: "Find ladder, a diff", route: "pr/mock%3Apayout-service%23142/diff", trips: [
    { id: "find-ladder-diff", kind: "find-ladder", strict: true, finding: "F-272", box: "Find in page", words: "JITTER", count: / in open files$/ },
    { id: "find-ladder-fallback", kind: "find-ladder", strict: true, finding: "F-272", box: "Find in page", words: "retr", count: / in open files$/, fallback: true },
  ] },
  { page: "Find ladder, Compare", route: `${REPO}/compare`, trips: [
    { id: "find-ladder-compare", kind: "find-ladder", strict: true, finding: "F-272", box: "Find in page", words: "jitter", count: / in open files$/ },
  ] },
  { page: "Find ladder, a ticket", route: "ticket/mock%3APAY-231", trips: [
    { id: "find-ladder-ticket", kind: "find-ladder", strict: true, finding: "F-169", box: "Find in page", words: "retry", count: / in this column$/ },
  ] },
  { page: "Find ladder, a Conversation", route: "pr/mock%3Apayout-service%23142/conversation", trips: [
    { id: "find-ladder-conversation", kind: "find-ladder", strict: true, finding: "F-074", box: "Find in page", words: "jitter", count: /^\d+ of \d+$/ },
  ] },
  { page: "Find ladder, a pull request's Overview", route: "pr/mock%3Apayout-service%23142", trips: [
    { id: "find-ladder-overview", kind: "find-ladder", strict: true, finding: "F-074", box: "Find in page", words: "retry", count: /^\d+ of \d+$/ },
  ] },
  { page: "Find ladder, a run's Log", route: `${RUN}/log`, trips: [
    { id: "find-ladder-log", kind: "find-ladder", strict: true, finding: "F-072", box: "Find in log" },
  ] },
  { page: "Find ladder, a file", route: `${REPO}/files/src/lib.rs`, trips: [
    { id: "find-ladder-file", kind: "find-ladder", strict: true, finding: "F-267", box: "Find in file" },
  ] },
  { page: "Find ladder, Tickets", route: "tickets", trips: [
    { id: "find-ladder-tickets", kind: "find-ladder", strict: true, finding: "F-072", box: "Filter tickets" },
  ] },
  { page: "Find ladder, Inbox", route: "inbox", trips: [
    { id: "find-ladder-inbox", kind: "find-ladder", strict: true, finding: "F-074", box: "Search or act", chip: "Inbox" },
  ] },
  { page: "Find ladder, Time", route: "time", trips: [
    { id: "find-ladder-time", kind: "find-ladder", strict: true, finding: "F-074", box: "Search or act" },
  ] },
  { page: "Find ladder, Settings", route: "settings/appearance", trips: [
    { id: "find-ladder-settings", kind: "find-ladder", strict: true, finding: "F-074", box: "Search or act" },
  ] },
  { page: "The find bar, a ticket", route: "ticket/mock%3APAY-231", trips: [
    { id: "find-meet-ticket", kind: "find-meet", strict: true, finding: "reconcile", how: "box", scroller: ".tf-work", words: "re" },
  ] },
  { page: "The find bar, a Docs page", route: "page/confluence%3Asepa", trips: [
    { id: "find-meet-docs", kind: "find-meet", strict: true, finding: "reconcile", how: "box", scroller: ".tf-rd-main", words: "re" },
  ] },
  { page: "The find bar, a pull request's tabs", route: "pr/mock%3Apayout-service%23142/conversation", trips: [
    { id: "find-meet-tabs", kind: "find-meet", strict: true, finding: "reconcile", how: "tab", words: "retry", size: [1440, 700], tabs: [{ text: "Commits", stays: true }, { text: "Files changed" }] },
  ] },
  { page: "The find bar, a diff's tabs", route: "pr/mock%3Apayout-service%23142/diff", trips: [
    { id: "find-meet-diff-tabs", kind: "find-meet", strict: true, finding: "reconcile", how: "tab", words: "retry", size: [1440, 700], tabs: [{ text: "Overview" }] },
  ] },
];

// Commands that change something. shell/dev/fake's reads, syncs' progress and heartbeats are left out.
const WRITE = /^(set_|delete_|create_|save_|add_|remove_|rename_|update_|install_|uninstall_|apply_|publish_|start_timer$|stop_timer$|start_work_(amend|retry|run|skip)$|submit_write$|amend_write$|discard_write$|flush_writes$|ack_alert$|unack_alert$|complete_inbox_item$|snooze_inbox_item$|dismiss_|accept_suggestion$|ad_hoc_block$|update_block$|log_work$|unlink$|move_|edit_|clone_repo$|hide_|order_dep_layers$|place_dep_kind$|restore_backup$|backup_now$|bind_|clear_tone_overrides$|docs_create_page$|docs_edit_section$|follow_merges$|complete_first_run$|sync_now$|sync_all$|clone_all$)/;

// Runs in the page, once per document, before the app.
const LIB = String.raw`(() => {
  const WRITE = ${WRITE};
  const seen = (e) => {
    if (!e.getClientRects().length || e.closest(".tf-kept[hidden], [hidden]")) return false;
    return getComputedStyle(e).visibility === "visible";
  };
  const say = (s) => (s ?? "").replace(/\s+/g, " ").trim();
  const words = (e) => say(e.textContent) || say(e.getAttribute("aria-label")) || say(e.value);
  const labelOf = (e) => {
    const l = e.id && document.querySelector('label[for="' + e.id + '"]');
    const own = l ? (l.firstChild?.nodeType === 3 ? l.firstChild.textContent : l.textContent) : "";
    return say(own) || say(e.getAttribute("aria-label")) || say(e.placeholder) || e.id || e.tagName.toLowerCase();
  };
  const rowOf = (e) => {
    const host = e.closest("tr, li, [class*=row], [class*=-app]") ?? e.parentElement;
    return host && host !== e ? say(host.textContent).slice(0, 40) : "";
  };
  const CONTROL = "a[href], button, summary, [role=button], [role=menuitem], [role=tab]";
  const FIELD = "input:not([type=hidden], [type=checkbox], [type=radio], [type=file], [type=range], [type=color]), select, textarea";
  window.__trip = {
    wrap() {
      // Through the app's own `invoke` (`app/src/lib/ipc/invoke.ts`): Tauri's bridge `invoke` is read-only.
      const i = window.__TAURI_INTERNALS__;
      if (!i || window.__knobasIpcTap?.__counted) return !!i;
      const prev = window.__knobasIpcTap;
      window.__writes = [];
      const tap = function (cmd, args, options, next) {
        if (WRITE.test(cmd)) window.__writes.push(cmd === "submit_write" ? Object.keys(args?.payload ?? {})[0] ?? cmd : cmd);
        return prev ? prev(cmd, args, options, next) : next(cmd, args, options);
      };
      tap.__counted = true;
      window.__knobasIpcTap = tap;
      return true;
    },
    find(p) {
      return [...document.querySelectorAll(p.sel)].find((e) => seen(e) && !e.disabled && (p.text === undefined || words(e) === p.text) && (p.starts === undefined || words(e).startsWith(p.starts)) && (p.row === undefined || say(e.closest("tr, li")?.textContent).startsWith(p.row))) ?? null;
    },
    click(p) { const e = this.find(p); if (!e) return false; e.focus(); e.click(); return true; }, // Focus first, as a real press does, so a dialog has its opener to go back to.
    fill(sel, text) {
      const e = document.querySelector(sel);
      if (!e) return false;
      e.focus();
      e.value = (e.value ? e.value + "\n" : "") + text;
      e.dispatchEvent(new Event("input", { bubbles: true }));
      return true;
    },
    focusPress(p) {
      const e = this.find(p);
      if (!e) return null;
      e.scrollIntoView({ block: "nearest" });
      e.focus();
      if (document.activeElement !== e) return null;
      window.__pressed = e;
      return JSON.stringify({ words: words(e), row: rowOf(e) });
    },
    point(p) {
      const e = this.find(p);
      if (!e) return null;
      e.scrollIntoView({ block: "nearest" });
      window.__pressed = e;
      const r = e.getBoundingClientRect();
      const x = r.left + r.width * 0.15;
      const y = r.top + r.height / 2;
      const under = document.elementFromPoint(x, y);
      return JSON.stringify({ x, y, words: words(e), under: !!under && (under === e || e.contains(under)) });
    },
    under(x, y) { const e = document.elementFromPoint(x, y)?.closest(CONTROL); return e ? words(e) : ""; },
    undos() { return [...document.querySelectorAll(".tf-toast")].filter((t) => seen(t) && [...t.querySelectorAll("button")].some((b) => words(b) === "Undo")).map((t) => say(t.querySelector("span")?.textContent ?? t.textContent)); },
    pressUndo(i) { const t = [...document.querySelectorAll(".tf-toast")].filter((t) => seen(t) && [...t.querySelectorAll("button")].some((b) => words(b) === "Undo"))[i]; if (!t) return false; const b = [...t.querySelectorAll("button")].find((b) => words(b) === "Undo"); b.focus(); b.click(); return true; },
    columns() {
      const t = [...document.querySelectorAll(".tf-main thead")].find((x) => seen(x));
      if (!t) return null;
      const left = t.getBoundingClientRect().left;
      return JSON.stringify([...t.querySelectorAll("th")].filter(seen).map((h) => ({ name: say(h.textContent) || "(no words)", left: +(h.getBoundingClientRect().left - left).toFixed(1) })));
    },
    tableRows() { const t = [...document.querySelectorAll(".tf-main tbody")].find((x) => seen(x)); return t ? [...t.querySelectorAll("tr")].filter(seen).length : 0; },
    snap(rows) {
      const a = document.activeElement;
      const body = !a || a === document.body || a === document.documentElement;
      const off = !body && (a.disabled || a.getAttribute("aria-disabled") === "true");
      const open = [...document.querySelectorAll(".tf-dialog, dialog[open], [role=dialog], .tf-drawer")].filter(seen);
      return JSON.stringify({
        hash: location.hash,
        dialogs: open.length,
        dialog: say(open.at(-1)?.querySelector("h2, h3")?.textContent),
        writes: window.__writes ?? [],
        toasts: [...document.querySelectorAll(".tf-toast")].filter(seen).map((t) => say(t.textContent)),
        rows: rows ? [...document.querySelectorAll(rows)].filter(seen).map((r) => say(r.textContent)) : [],
        active: { body, pressed: !body && a === window.__pressed, control: !body && a.matches(CONTROL) && !off, words: body ? "" : words(a), row: body ? "" : rowOf(a), tag: body ? "body" : a.tagName.toLowerCase(), scroller: !body && (a.matches(".tf-scroller") || (a.tabIndex === -1 && a.tagName !== "TEXTAREA" && /auto|scroll/.test(getComputedStyle(a).overflowY))) ? String(a.className) : "" },
      });
    },
    fields() { return [...document.querySelectorAll(".tf-main " + FIELD.split(", ").join(", .tf-main "))].filter(seen).length; },
    focusField(sel, i) {
      const e = sel ? document.querySelector(sel) : [...document.querySelectorAll(".tf-main " + FIELD.split(", ").join(", .tf-main "))].filter(seen)[i];
      if (!e) return null;
      e.focus();
      if (document.activeElement !== e) return null;
      window.__field = e;
      return JSON.stringify({ label: labelOf(e), value: e.value, type: e.type });
    },
    valueOf(sel) { return document.querySelector(sel)?.value ?? null; },
    onBody() { const a = document.activeElement; return !a || a === document.body || a === document.documentElement; },
    walkPair(p, rows) {
      const list = [...document.querySelectorAll(rows)].filter(seen);
      const inRow = (r) => [...r.querySelectorAll(p.sel)].find((e) => seen(e) && !e.disabled && (p.text === undefined || words(e) === p.text));
      const mid = (e) => { const r = e.getBoundingClientRect(); return { x: r.left + r.width / 2, y: r.top + r.height / 2 }; };
      for (let i = 0; i + 1 < list.length; i++) {
        const a = inRow(list[i]), b = inRow(list[i + 1]);
        if (!a || !b) continue;
        b.scrollIntoView({ block: "nearest" }); // Only if it must: when the list shrinks, a page scrolled to its end clamps, and that can put the right row back by luck.
        const from = mid(a), to = mid(b);
        return JSON.stringify({ from, to, shown: to.y < innerHeight && from.y > 0, words: words(a), row: say(list[i].textContent), aimed: say(list[i + 1].textContent) });
      }
      return null;
    },
    rowAt(x, y, rows) { return say(document.elementFromPoint(x, y)?.closest(rows)?.textContent); },
    dialog() {
      const d = [...document.querySelectorAll(".tf-dialog")].filter(seen).at(-1);
      if (!d) return null;
      const a = document.activeElement;
      const arrive = d.querySelector("[data-arrive]");
      return JSON.stringify({
        title: say(d.querySelector("h2, h3")?.textContent),
        active: a && d.contains(a) ? { tag: a.tagName.toLowerCase(), label: labelOf(a), arrive: a === arrive } : { tag: a ? a.tagName.toLowerCase() : "body", label: "", arrive: false },
        arrive: arrive ? labelOf(arrive) : null,
        selects: [...d.querySelectorAll("select")].filter(seen).map((s) => ({ label: labelOf(s), value: s.value, shown: say(s.selectedOptions[0]?.text), others: [...s.options].filter((o) => !o.selected && !o.disabled).map((o) => say(o.text)) })),
        fields: [...d.querySelectorAll("input[type=text], input:not([type]), textarea")].filter(seen).map((e) => ({ label: labelOf(e), value: e.value })),
      });
    },
    scrollOf(sel) { return this.find({ sel })?.scrollTop ?? null; },
    scrollTo(sel, top) { const e = this.find({ sel }); if (!e) return null; e.scrollTop = top; return e.scrollTop; },
    landing(sel, to) {
      const s = this.find({ sel });
      const e = [...document.querySelectorAll(to)].find(seen);
      if (!s || !e) return null;
      const box = s.getBoundingClientRect(), r = e.getBoundingClientRect();
      return JSON.stringify({ gap: Math.round(r.top - box.top), inView: r.top >= box.top - 2 && r.top < box.bottom, atEnd: s.scrollTop + s.clientHeight >= s.scrollHeight - 2, top: Math.round(s.scrollTop), height: Math.round(box.height) });
    },
    strip(col) {
      const list = [...document.querySelectorAll('ol.col[data-col="' + col + '"]')].find(seen);
      const pane = [...document.querySelectorAll("aside.pane")].find(seen);
      const a = document.activeElement;
      return JSON.stringify({ x: list ? +list.getBoundingClientRect().left.toFixed(1) : null, pane: pane ? +pane.getBoundingClientRect().left.toFixed(1) : null, paneWidth: pane ? Math.round(pane.getBoundingClientRect().width) : null, row: say(a?.querySelector?.(".lbl")?.textContent) || say(a?.textContent), holder: !!a && say(a.textContent).endsWith("›"), col: a?.closest?.("ol.col")?.dataset.col ?? null });
    },
    focusRow(name) {
      const e = [...document.querySelectorAll("button.row[data-asset]")].find((b) => seen(b) && say(b.querySelector(".lbl")?.textContent) === name);
      if (!e) return null;
      e.scrollIntoView({ block: "nearest" });
      e.focus();
      return document.activeElement === e ? e.closest("ol.col")?.dataset.col ?? null : null;
    },
  };
})()`;

// The rows-still recorder, put in the page when a trip arms it. Rows are matched across frames by element, else by `data-key`, the row's link or its words.
const STILL = String.raw`(() => {
  if (window.__still) return;
  const w = (window.__still = { frames: null, sel: "" });
  const keyOf = (el) => el.getAttribute("data-key") || (el.matches("a[href]") ? el.getAttribute("href") : el.querySelector("a[href]")?.getAttribute("href")) || (el.textContent || "").trim().slice(0, 60);
  const snapshot = () => {
    if (w.frames) {
      const rows = [...document.querySelectorAll(w.sel)].filter((el) => !el.closest(".tf-skel")).map((el) => { const b = el.getBoundingClientRect(); return { el, key: keyOf(el), top: Math.round(b.top), h: Math.round(b.height), left: Math.round(b.left), w: Math.round(b.width) }; }).filter((r) => r.h > 0 && r.key !== "" && r.top < innerHeight);
      const sig = rows.map((r) => r.key + "@" + r.left + "," + r.top + "/" + r.h).join("|");
      if (w.frames.at(-1)?.sig !== sig) w.frames.push({ t: Math.round(performance.now() - w.t0), sig, rows });
    }
  };
  const tick = () => {
    snapshot();
    requestAnimationFrame(tick);
  };
  requestAnimationFrame(tick);
  // On the page already, the first frame is taken as it arms, so a read landing before the next paint still shows as a change.
  w.arm = (sel, now) => { w.sel = sel; w.t0 = performance.now(); w.frames = []; if (now) snapshot(); return 1; };
  w.verdict = () => {
    const frames = w.frames ?? [];
    w.frames = null;
    const most = Math.max(0, ...frames.map((f) => f.rows.length));
    const at = frames.findIndex((f) => f.rows.length > 0 && f.rows.length >= Math.min(3, most));
    if (at < 0) return JSON.stringify({ drew: false, frames: frames.length });
    const first = frames[at].rows.filter((r) => r.top + r.h > 0);
    // A row that comes in above a drawn row in the same column pushes it; one filling an empty pane beside them does not.
    const over = (a, b) => a.left < b.left + b.w && b.left < a.left + a.w;
    const say = (r) => JSON.stringify(r.key.slice(-42));
    for (const f of frames.slice(at + 1)) {
      const used = new Set();
      for (const r of first) {
        const now = f.rows.find((x) => x.el === r.el && !used.has(x)) ?? f.rows.find((x) => x.key === r.key && !used.has(x));
        if (!now) return JSON.stringify({ drew: true, firstAt: frames[at].t, problem: "at " + f.t + " ms the row " + say(r) + " (drawn at " + r.top + " px) left the list" });
        used.add(now);
        if (now.top !== r.top || now.h !== r.h || now.left !== r.left) return JSON.stringify({ drew: true, firstAt: frames[at].t, problem: "at " + f.t + " ms the row " + say(r) + " moved from " + r.left + "," + r.top + " px, " + r.h + " high, to " + now.left + "," + now.top + " px, " + now.h + " high" });
      }
      const came = f.rows.find((x) => !used.has(x) && first.some((r) => over(x, r) && x.top <= r.top));
      if (came) return JSON.stringify({ drew: true, firstAt: frames[at].t, problem: "at " + f.t + " ms the row " + say(came) + " came in at " + came.top + " px, among rows drawn at " + frames[at].t + " ms" });
    }
    return JSON.stringify({ drew: true, firstAt: frames[at].t, rows: first.length, frames: frames.length - at, last: frames.at(-1).t });
  };
})()`;

const argv = process.argv.slice(2);
const outDir = argv[0];
const flag = (n, d) => {
  const i = argv.indexOf(`--${n}`);
  return i >= 0 ? argv[i + 1] : d;
};
if (!outDir || outDir.startsWith("--")) {
  console.error("usage: trips.mjs <out dir> --port <port> [--only a,b] [--workers 2]");
  process.exit(2);
}
const port = Number(flag("port", 5317));
const workers = Number(flag("workers", 2));
const only = flag("only", "").split(",").filter(Boolean);
const gapFlag = flag("gap", null);
const strict = argv.includes("--strict");
mkdirSync(outDir, { recursive: true });

const table = PAGES.flatMap((p) => p.trips.map((t) => ({ ...t, page: p.page, route: p.route })));
const every = table.filter((t) => strict || !t.strict);
const wanted = (t) => !only.length || only.some((o) => [t.id, t.kind, t.finding, t.page, t.route].some((v) => v?.includes(o)));

async function worker(queue, results) {
  const chrome = await launch("knobas-trips");
  const { send, evaluate, key, stop } = chrome;
  const opener = screenOpener(chrome, port);
  await opener.install();
  await send("Page.addScriptToEvaluateOnNewDocument", { source: LIB });
  await send("Emulation.setDeviceMetricsOverride", { width: 1440, height: 900, deviceScaleFactor: 1, mobile: false });
  await send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-color-scheme", value: "light" }] });

  const snap = async (rows) => JSON.parse(await evaluate(`window.__trip.snap(${JSON.stringify(rows ?? "")})`));
  const go = async (route) => {
    await evaluate(`location.hash = ${JSON.stringify(`#/${route}`)}`);
    await opener.settleAfterChange();
  };
  const open = async (t) => {
    await opener.openScreen({ name: t.id, route: t.route, query: t.query });
    if (!(await evaluate(`window.__trip.wrap()`))) throw new Error("no __TAURI_INTERNALS__ to wrap");
    for (const s of t.prep ?? []) {
      if (s.wait) await sleep(s.wait);
      if (s.click && !(await evaluate(`window.__trip.click(${JSON.stringify({ sel: s.click, text: s.text, starts: s.starts })})`))) throw new Error(`prep can't find ${s.click} "${s.text ?? s.starts ?? ""}"`);
      if (s.eval) await evaluate(s.eval);
      if (s.fill && !(await evaluate(`window.__trip.fill(${JSON.stringify(s.fill)}, ${JSON.stringify(s.text)})`))) throw new Error(`prep can't find ${s.fill}`);
    }
  };
  const focusPress = async (t) => {
    const raw = await evaluate(`window.__trip.focusPress(${JSON.stringify(t.press)})`);
    if (raw === null || raw === undefined) throw new Error(`can't focus ${t.press.sel} ${t.press.text ?? t.press.starts ?? ""}`.trim());
    return JSON.parse(raw);
  };
  const added = (later, earlier) => later.writes.slice(earlier.writes.length);
  const fit = (w, h) => send("Emulation.setDeviceMetricsOverride", { width: w, height: h, deviceScaleFactor: 1, mobile: false });
  const tidy = (list) => list.map((r) => `"${r.slice(0, 48)}"`).join(", ");

  async function enterTwice(t) {
    await open(t);
    const { words: pressed, row: pressedRow } = await focusPress(t);
    const before = await snap(t.rows);
    await key("Enter");
    const held = t.leaves || t.returns;
    await sleep(held ? Number(gapFlag ?? t.gap ?? 200) : t.after ?? 500);
    const first = await snap(t.rows);
    const did = added(first, before).length || first.hash !== before.hash || first.dialogs !== before.dialogs || first.rows.length !== before.rows.length || first.toasts.join() !== before.toasts.join() || first.active.words !== before.active.words || !first.active.pressed;
    if (!did) throw new Error(`the first Enter on "${pressed}" changed nothing`);
    if (first.hash !== before.hash && !t.leaves) throw new Error(`the first Enter on "${pressed}" left for ${first.hash}`);
    if (t.leaves && first.hash === before.hash) throw new Error(`the first Enter on "${pressed}" was meant to leave and stayed`);
    if (first.dialogs > before.dialogs && !t.opens) throw new Error(`the first Enter on "${pressed}" opened a dialog: ${first.dialog}`);
    if (t.opens && first.dialogs <= before.dialogs) throw new Error(`the first Enter on "${pressed}" was meant to open a dialog and opened none`);
    const rowLeft = !!t.rows && first.rows.length < before.rows.length;
    const landed = (a) => a.control && !a.pressed && a.words !== pressed;
    const reasons = [];
    const back = (words) => (t.returnsCounting ? words.startsWith(t.returns) : words === t.returns);
    if (t.returns && !back(first.active.words)) reasons.push(`the first Enter on "${pressed}" should give focus back to "${t.returns}" and it is on "${first.active.words || first.active.tag}"`);
    else if (landed(first.active) && !t.returns && !t.opens) reasons.push(`the first Enter on "${pressed}" put focus on "${first.active.words}", so the second presses that`);
    await key("Enter");
    await sleep(t.after ?? 500);
    const second = await snap(t.rows);
    if (second.hash !== first.hash) reasons.push(`the second Enter changed the address to ${second.hash}`);
    if (second.dialogs > first.dialogs) reasons.push(`the second Enter opened "${second.dialog}"`);
    if (t.opens && second.dialogs < first.dialogs) reasons.push(`the second Enter closed the dialog "${first.dialog}" the first had opened, focus being on "${first.active.words || first.active.tag}"`);
    const sent = added(second, first);
    const inRow = first.active.row && first.active.row !== pressedRow ? ` in "${first.active.row}", pressed in "${pressedRow}"` : "";
    if (sent.length && !(rowLeft && first.active.words === pressed)) reasons.push(`the second Enter sent ${sent.join(", ")}, focus being on "${first.active.words}"${first.active.pressed ? " (the pressed control)" : inRow || (t.rows && !rowLeft ? " with the pressed row still there" : "")}`);
    if (t.returns && !back(second.active.words) && second.hash === first.hash && second.dialogs <= first.dialogs) reasons.push(`after the second Enter focus is on "${second.active.words || second.active.tag}", not "${t.returns}"`);
    if (!t.returns && !t.opens && second.hash === first.hash && second.dialogs <= first.dialogs && landed(second.active) && !landed(first.active)) reasons.push(`after the second Enter focus is on "${second.active.words}", not "${pressed}"`);
    return { reasons, note: `"${pressed}": first sent ${added(first, before).join(", ") || "nothing"}, focus then "${first.active.words || first.active.tag}"${rowLeft ? ", row left" : ""}; second sent ${sent.join(", ") || "nothing"}, focus then "${second.active.words || second.active.tag}"` };
  }

  async function enterFast(t) {
    const reasons = [];
    const notes = [];
    for (const gap of t.gaps ?? [0, 120]) {
      await open(t);
      const { words: pressed } = await focusPress(t);
      const before = await snap(t.rows);
      await key("Enter");
      if (gap) await sleep(gap);
      await key("Enter");
      await sleep(t.after ?? 500);
      const after = await snap(t.rows);
      const sent = added(after, before);
      if (!sent.length) throw new Error(`Enter on "${pressed}" wrote nothing`);
      const gone = before.rows.filter((r) => !after.rows.includes(r));
      if (sent.length > 1) reasons.push(`at ${gap} ms "${pressed}" wrote twice (${sent.join(", ")}), rows ${before.rows.length} to ${after.rows.length}, taking ${tidy(gone)}`);
      notes.push(`${gap} ms: sent ${sent.join(", ")}, rows ${before.rows.length} to ${after.rows.length}`);
    }
    return { reasons, note: notes.join("; ") };
  }

  async function enterWalks(t) {
    await open(t);
    const { words: pressed } = await focusPress(t);
    const before = await snap(t.rows);
    await key("Enter");
    const gap = t.gap ?? 600;
    await sleep(gap);
    const first = await snap(t.rows);
    if (first.rows.length >= before.rows.length) throw new Error(`"${pressed}" hid no row (${before.rows.length} then ${first.rows.length})`);
    if (first.active.words !== pressed) throw new Error(`focus went to "${first.active.words || first.active.tag}", not the next row's "${pressed}", so there is no walk to try`);
    await key("Enter");
    await sleep(t.after ?? 500);
    const second = await snap(t.rows);
    const sent = added(second, first);
    const reasons = [];
    if (!sent.length) reasons.push(`a second Enter ${gap} ms after the first on the next row's "${pressed}" wrote nothing, so a person can't walk down the list`);
    return { reasons, note: `"${pressed}" after ${gap} ms: rows ${before.rows.length}, ${first.rows.length}, ${second.rows.length}; second sent ${sent.join(", ") || "nothing"}` };
  }

  async function pointerTwice(t) {
    const reasons = [];
    const notes = [];
    const press = async (p, clickCount) => {
      await send("Input.dispatchMouseEvent", { type: "mousePressed", x: p.x, y: p.y, button: "left", clickCount });
      await send("Input.dispatchMouseEvent", { type: "mouseReleased", x: p.x, y: p.y, button: "left", clickCount });
    };
    for (const gap of t.gaps ?? [120, 300, 600]) {
      await open(t);
      let raw = await evaluate(`window.__trip.point(${JSON.stringify(t.press)})`);
      if (!raw) throw new Error(`can't find ${t.press.sel} ${t.press.text ?? t.press.starts ?? ""}`.trim());
      if (t.reveal && !JSON.parse(raw).under) {
        // A row's fading group takes no press until the pointer is on the row, as a person's hand is before it presses.
        const at = JSON.parse(raw);
        await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: at.x, y: at.y });
        await sleep(200);
        raw = await evaluate(`window.__trip.point(${JSON.stringify(t.press)})`);
      }
      const p = JSON.parse(raw);
      if (!p.under) throw new Error(`something else is at 15% of "${p.words}"`);
      await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: p.x, y: p.y });
      await sleep(120);
      const before = await snap(t.rows);
      await press(p, 1);
      await sleep(gap);
      const first = await snap(t.rows);
      if (first.dialogs > before.dialogs) throw new Error(`the first press on "${p.words}" opened "${first.dialog}"`);
      // With `rows`, a press that writes nothing (*Show N new*) acts by changing them.
      const moved = (a, b) => !!t.rows && (a.rows.length !== b.rows.length || a.rows.join() !== b.rows.join());
      if (!added(first, before).length && first.toasts.join() === before.toasts.join() && !moved(first, before)) throw new Error(`the first press on "${p.words}" changed nothing`);
      const under = await evaluate(`window.__trip.under(${p.x}, ${p.y})`);
      await press(p, gap < 500 ? 2 : 1); // The second of a double-click counts 2; at 600 ms most systems count a new click.
      await sleep(t.after ?? 400);
      const second = await snap(t.rows);
      if (second.dialogs > first.dialogs) reasons.push(`at ${gap} ms, a second press at 15% of "${p.words}" lands on "${under}" and opens "${second.dialog}"`);
      if (moved(second, first)) reasons.push(`at ${gap} ms, a second press at 15% of "${p.words}" lands on "${under}" and changes the rows again`);
      if (second.hash !== first.hash) reasons.push(`at ${gap} ms, a second press at 15% of "${p.words}" lands on "${under}" and goes to ${second.hash}`);
      if (t.once && added(second, first).length) reasons.push(`at ${gap} ms, a second press on "${p.words}" sent ${added(second, first).join(", ")}`);
      notes.push(`${gap} ms: point holds "${under || "no control"}", ${second.dialogs > first.dialogs ? "dialog" : "no dialog"}`);
    }
    return { reasons, note: notes.join("; ") };
  }

  async function undoStale(t) {
    const value = async () => evaluate(`(${t.value})`);
    const change = async () => {
      await open(t);
      const was = await value();
      if (was === null || was === undefined) throw new Error("can't read the setting's value");
      const seenValues = [was];
      for (const c of t.changes) {
        if (!(await evaluate(`window.__trip.click(${JSON.stringify(c)})`))) throw new Error(`can't find ${c.sel} ${c.row ?? c.text ?? ""}`.trim());
        await sleep(500);
        seenValues.push(await value());
      }
      return { was, seenValues, undos: JSON.parse(await evaluate(`JSON.stringify(window.__trip.undos())`)), toasts: (await snap()).toasts };
    };
    const reasons = [];
    const notes = [];
    const first = await change();
    if (first.seenValues[1] === first.was) throw new Error(`the first change left the value at "${first.was}"`);
    if (first.toasts.length > 1) reasons.push(`after ${first.seenValues.join(", ")}, ${first.toasts.length} toasts on one setting are up together: ${tidy(first.toasts)}`);
    if (!first.undos.length) {
      if (first.seenValues.at(-1) !== first.was) throw new Error(`no Undo is up after ${first.seenValues.join(", ")}`);
      return { reasons, note: `${first.seenValues.join(", ")}: back at the first with no Undo to press` };
    }
    // Newest first, then the older toast's, on a fresh page: with one toast up, as Round 14 asks, the second pass doesn't happen.
    for (const which of first.undos.length > 1 ? [first.undos.length - 1, 0] : [0]) {
      const run = which === first.undos.length - 1 || first.undos.length === 1 ? first : await change();
      const pressed = run.undos[which];
      if (!(await evaluate(`window.__trip.pressUndo(${which})`))) throw new Error(`no Undo on "${pressed}"`);
      await sleep(500);
      const now = await value();
      const still = JSON.parse(await evaluate(`JSON.stringify(window.__trip.undos())`));
      if (now !== run.was) reasons.push(`values ${run.seenValues.join(", ")}, Undo on "${pressed}" gives "${now}", and it should give the first, "${run.was}"`);
      if (still.length) reasons.push(`Undo on "${pressed}" gives "${now}", with ${tidy(still)} still up and offering its own Undo`);
      notes.push(`${run.seenValues.join(", ")}, Undo on "${pressed}" gives "${now}"`);
    }
    return { reasons, note: notes.join("; ") };
  }

  async function columnFilter(t) {
    const reasons = [];
    const notes = [];
    const columns = async () => JSON.parse((await evaluate(`window.__trip.columns()`)) ?? "null");
    const rows = () => evaluate(`window.__trip.tableRows()`);
    try {
      for (const [w, h] of t.sizes ?? [[1440, 900]]) {
        await fit(w, h);
        await open(t);
        const start = await columns();
        if (!start?.length) throw new Error(`no table at ${w}`);
        const rowsAtStart = await rows();
        let rowsChanged = false;
        const moved = new Map();
        const compare = async (what) => {
          await opener.settleAfterChange();
          const now = await columns();
          if (!now) throw new Error(`the table went away after ${what} at ${w}`);
          if ((await rows()) !== rowsAtStart) rowsChanged = true;
          for (const c of now) {
            const was = start.find((s) => s.name === c.name);
            if (was && Math.abs(c.left - was.left) > 1 && !moved.has(c.name)) moved.set(c.name, `${c.name} from ${was.left} to ${c.left} on ${what}`);
          }
        };
        if (t.filter) {
          if (!(await evaluate(`(() => { const i = window.__trip.find({ sel: ".tf-main .tf-fsearch input" }); if (!i) return false; i.focus(); return true; })()`))) throw new Error("no filter field");
          for (const ch of t.filter) {
            await send("Input.insertText", { text: ch });
            await compare(`"${ch}"`);
          }
        }
        if (t.pill) {
          if (!(await evaluate(`window.__trip.click(${JSON.stringify(t.pill)})`))) throw new Error(`no pill "${t.pill.text}"`);
          await compare(`the pill "${t.pill.text}"`);
        }
        if (!rowsChanged) throw new Error(`the rows stayed at ${rowsAtStart} at ${w}, so nothing was filtered`);
        if (moved.size) reasons.push(`at ${w} px, ${[...moved.values()].join(", ")}`);
        notes.push(`${w} px: ${rowsAtStart} rows, ${moved.size} columns moved`);
      }
    } finally {
      await fit(1440, 900);
    }
    return { reasons, note: notes.join("; ") };
  }

  async function rowRoom(t) {
    const reasons = [];
    const notes = [];
    const rowsOf = `[...document.querySelectorAll(${JSON.stringify(`${t.table} tbody tr`)})].filter((r) => r.querySelector(${JSON.stringify(t.cell)}))`;
    // Each button of row i against the table's box, which clips (overflow: clip), so a button past it is cut.
    const read = async (i) => JSON.parse(await evaluate(`(() => {
      const box = document.querySelector(${JSON.stringify(t.table)}).getBoundingClientRect();
      const row = ${rowsOf}[${i}];
      const name = (row.querySelector("td")?.textContent ?? "").replace(/\\s+/g, " ").trim().slice(0, 30);
      const past = [...row.querySelectorAll(${JSON.stringify(`${t.cell} button`)})].map((b) => ({ b, r: b.getBoundingClientRect() }))
        .filter(({ r }) => r.right > box.right + 0.5 || r.left < box.left - 0.5)
        .map(({ b, r }) => b.textContent.trim() + " by " + Math.ceil(Math.max(r.right - box.right, box.left - r.left)) + " px");
      return JSON.stringify({ name, past, buttons: row.querySelectorAll(${JSON.stringify(`${t.cell} button`)}).length });
    })()`));
    try {
      for (const hovers of [true, false]) {
        await send("Emulation.setTouchEmulationEnabled", hovers ? { enabled: false } : { enabled: true, maxTouchPoints: 1 });
        for (const [w, h] of t.sizes ?? [[1440, 900]]) {
          await fit(w, h);
          await open(t);
          if ((await evaluate(`matchMedia("(hover: hover)").matches`)) !== hovers) throw new Error(`the page reads hover ${!hovers} where the trip set ${hovers}`);
          const count = await evaluate(`${rowsOf}.length`);
          if (!count) throw new Error(`no row of ${t.table} holds ${t.cell} at ${w}`);
          let buttons = 0;
          for (let i = 0; i < count; i++) {
            const at = JSON.parse(await evaluate(`(() => { const r = ${rowsOf}[${i}]; r.scrollIntoView({ block: "center" }); const b = r.querySelector("td").getBoundingClientRect(); return JSON.stringify([b.left + 10, b.top + b.height / 2]); })()`));
            const ways = [["at rest", async () => send("Input.dispatchMouseEvent", { type: "mouseMoved", x: 1, y: 1 })]];
            if (hovers) ways.push(["hovered", async () => send("Input.dispatchMouseEvent", { type: "mouseMoved", x: at[0], y: at[1] })]);
            ways.push(["with focus in its group", async () => {
              await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: 1, y: 1 });
              // The control before the group's first, then Tab, so focus arrives as by key and :focus-visible holds.
              const ok = await evaluate(`(() => { const first = ${rowsOf}[${i}].querySelector(${JSON.stringify(`${t.cell} .tf-rowacts button`)}); if (!first) return false; const all = [...document.querySelectorAll("a[href], button:not([disabled]), input, select, textarea, [tabindex]:not([tabindex='-1'])")]; const before = all[all.indexOf(first) - 1]; if (!before) return false; before.focus(); return true; })()`);
              if (!ok) throw new Error(`row ${i + 1} has no group to Tab into at ${w}`);
              await key("Tab");
              if (!(await evaluate(`!!document.activeElement?.matches(":focus-visible") && !!document.activeElement.closest(".tf-rowacts") && ${rowsOf}[${i}].contains(document.activeElement)`))) throw new Error(`Tab did not reach row ${i + 1}'s group at ${w}`);
            }]);
            for (const [how, go] of ways) {
              await go();
              await sleep(200); // The group's fade is 120 ms.
              const m = await read(i);
              buttons = Math.max(buttons, m.buttons);
              if (m.past.length) reasons.push(`at ${w} px ${hovers ? "with" : "without"} hover, "${m.name}" ${how}: ${m.past.join(", ")} past the table's edge`);
            }
            await evaluate(`document.activeElement?.blur()`);
          }
          notes.push(`${w} px ${hovers ? "hover" : "no hover"}: ${count} rows, up to ${buttons} buttons`);
        }
      }
    } finally {
      await send("Emulation.setTouchEmulationEnabled", { enabled: false });
      await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: 1, y: 1 });
      await fit(1440, 900);
    }
    return { reasons, note: notes.join("; ") };
  }

  async function leaveBack(t) {
    await open(t);
    const { words: pressed } = await focusPress(t);
    const before = await snap(t.rows);
    await key("Enter");
    await sleep(t.after ?? 500);
    const after = await snap(t.rows);
    if (!after.toasts.length) throw new Error(`"${pressed}" raised no toast`);
    if (!t.rowsStay && after.rows.length >= before.rows.length) throw new Error(`"${pressed}" hid no row (${before.rows.length} then ${after.rows.length})`);
    const toast = after.toasts.at(-1);
    await go(t.via);
    const away = await snap();
    await go(t.route);
    await sleep(150);
    if (t.reopen) {
      if (!(await evaluate(`window.__trip.click(${JSON.stringify({ sel: t.reopen.click, text: t.reopen.text, starts: t.reopen.starts })})`))) throw new Error(`can't find ${t.reopen.click} on the return`);
      await sleep(700);
    }
    const back = await snap(t.rows);
    if (!back.toasts.includes(toast)) throw new Error(`the toast "${toast}" had gone by the return, so nothing was proved`);
    // Digits out: rows carry the clock (14:47), and a minute can turn mid-trip. These are counts, so a row that returns still raises one.
    const same = (r) => r.replace(/\d+/g, "#");
    const count = (list) => list.reduce((m, r) => m.set(same(r), (m.get(same(r)) ?? 0) + 1), new Map());
    const was = count(after.rows);
    const now = count(back.rows);
    const returned = back.rows.filter((r) => (now.get(same(r)) ?? 0) > (was.get(same(r)) ?? 0));
    const lost = after.rows.filter((r) => (was.get(same(r)) ?? 0) > (now.get(same(r)) ?? 0));
    const reasons = [];
    if (returned.length) reasons.push(`after "${pressed}", ${t.via} and back, ${[...new Set(returned)].map((r) => `"${r.slice(0, 70)}"`).join(", ")} is drawn again under the toast "${toast.slice(0, 60)}"`);
    if (lost.length) reasons.push(`after "${pressed}", ${t.via} and back, ${[...new Set(lost)].map((r) => `"${r.slice(0, 70)}"`).join(", ")} has gone`);
    return { reasons, note: `"${pressed}": ${before.rows.length} rows, ${after.rows.length} after the press, ${back.rows.length} after ${t.via} (${away.hash}) and back, toast still up` };
  }

  async function typeOne(sel, type) {
    const ch = type === "number" ? "9" : "x";
    await send("Input.dispatchKeyEvent", { type: "keyDown", key: ch, code: type === "number" ? "Digit9" : "KeyX", text: ch, windowsVirtualKeyCode: ch.toUpperCase().charCodeAt(0) });
    await send("Input.dispatchKeyEvent", { type: "keyUp", key: ch, code: type === "number" ? "Digit9" : "KeyX", windowsVirtualKeyCode: ch.toUpperCase().charCodeAt(0) });
    await sleep(60);
    return evaluate(`window.__trip.valueOf(${JSON.stringify(sel)})`);
  }

  async function escSweep(t) {
    await open(t);
    const reasons = [];
    const notes = [];
    const n = t.fields?.length ?? (await evaluate(`window.__trip.fields()`));
    if (!n) throw new Error("no field on the page");
    for (let i = 0; i < n; i++) {
      const raw = await evaluate(`window.__trip.focusField(${JSON.stringify(t.fields?.[i] ?? null)}, ${i})`);
      if (!raw) { reasons.push(`field ${i + 1} of ${n} can't take focus`); continue; }
      const { label } = JSON.parse(raw);
      const at = await evaluate(`location.hash`);
      await key("Escape");
      await sleep(300);
      const a = await snap();
      if (a.active.body) reasons.push(`Esc on "${label}" dropped focus to the body`);
      if (strict && a.active.scroller) reasons.push(`Esc on "${label}" landed focus on the scroller ${a.active.tag}.${a.active.scroller.split(" ").join(".")}`);
      notes.push(`${label}: ${a.active.body ? "body" : `${a.active.tag} "${a.active.words.slice(0, 24)}"`}${a.hash !== at ? `, went to ${a.hash}` : ""}`);
      if (a.hash !== at) await open(t);
    }
    for (const sel of t.typed ?? []) {
      await open(t);
      const raw = await evaluate(`window.__trip.focusField(${JSON.stringify(sel)}, 0)`);
      if (!raw) throw new Error(`can't focus ${sel}`);
      const { label, value, type } = JSON.parse(raw);
      const typed = await typeOne(sel, type);
      if (typed === value) throw new Error(`typing into "${label}" changed nothing`);
      await key("Escape");
      await sleep(300);
      const fell = await evaluate(`window.__trip.onBody()`);
      await go("inbox");
      await go(t.route);
      await sleep(100);
      const later = await evaluate(`window.__trip.valueOf(${JSON.stringify(sel)})`);
      if (later !== value) reasons.push(`"${label}": ${value} typed to ${typed}, then Esc${fell ? " dropped focus to the body" : ""}, and it reads ${later} after inbox and back`);
      else if (fell) reasons.push(`"${label}": with ${typed} typed, Esc dropped focus to the body`);
      notes.push(`${label} typed: ${value} to ${typed}, back to ${later}`);
    }
    return { reasons, note: notes.join("; ") };
  }

  async function railTop(t) {
    const reasons = [];
    const notes = [];
    const top = () => evaluate(`document.querySelector(${JSON.stringify(t.scroller)})?.scrollTop ?? null`);
    const toEnd = async () => {
      await open(t);
      const at = await evaluate(`(() => { const e = document.querySelector(${JSON.stringify(t.scroller)}); if (!e) return null; e.scrollTop = e.scrollHeight; return e.scrollTop; })()`);
      if (!at) throw new Error(`${t.scroller} has nowhere to scroll (${at})`);
      await sleep(120);
      return at;
    };
    const [w, h] = t.size ?? [1440, 900];
    await fit(w, h);
    try {
      const fromP = await toEnd();
      const raw = await evaluate(`window.__trip.point(${JSON.stringify(t.press)})`);
      if (!raw) throw new Error(`can't find ${t.press.sel} ${t.press.text ?? ""}`.trim());
      const p = JSON.parse(raw);
      await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: p.x, y: p.y });
      await sleep(60);
      await send("Input.dispatchMouseEvent", { type: "mousePressed", x: p.x, y: p.y, button: "left", clickCount: 1 });
      await send("Input.dispatchMouseEvent", { type: "mouseReleased", x: p.x, y: p.y, button: "left", clickCount: 1 });
      await opener.settleAfterChange();
      await sleep(150);
      const byPointer = await top();
      if (byPointer !== 0) reasons.push(`by pointer, from ${fromP}, "${t.press.text}" leaves ${t.scroller} at ${byPointer}`);
      const fromK = await toEnd();
      await focusPress(t);
      await key("Enter");
      await opener.settleAfterChange();
      await sleep(150);
      const byKey = await top();
      if (byKey !== 0) reasons.push(`by key, from ${fromK}, "${t.press.text}" leaves ${t.scroller} at ${byKey}`);
      notes.push(`from ${fromP}: pointer ${byPointer}; from ${fromK}: key ${byKey}`);
    } finally {
      await fit(1440, 900);
    }
    return { reasons, note: notes.join("; ") };
  }

  async function aimedAfterClose(t) {
    await open(t);
    const pressAt = async (p) => {
      const raw = await evaluate(`window.__trip.point(${JSON.stringify(p)})`);
      if (!raw) throw new Error(`can't find ${p.sel} ${p.text ?? ""}`.trim());
      const at = JSON.parse(raw);
      await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: at.x, y: at.y });
      await sleep(60);
      await send("Input.dispatchMouseEvent", { type: "mousePressed", x: at.x, y: at.y, button: "left", clickCount: 1 });
      await send("Input.dispatchMouseEvent", { type: "mouseReleased", x: at.x, y: at.y, button: "left", clickCount: 1 });
    };
    await pressAt(t.open);
    await sleep(400);
    if (!(await snap()).dialogs) throw new Error(`"${t.open.text}" opened no dialog`);
    await pressAt(t.close);
    await sleep(60);
    if ((await snap()).dialogs) throw new Error(`"${t.close.text}" closed no dialog`);
    await key("Tab");
    const before = await snap();
    if (!before.active.control) throw new Error(`Tab landed on "${before.active.words || before.active.tag}", not a control`);
    await key("Enter");
    await sleep(300);
    const after = await snap();
    const acted = added(after, before).length > 0 || after.toasts.join() !== before.toasts.join() || after.dialogs !== before.dialogs || after.hash !== before.hash;
    return { reasons: acted ? [] : [`a dialog closed, then Tab to "${before.active.words}" and Enter, and that Enter did nothing`], note: `Tab to "${before.active.words}": ${added(after, before).join(", ") || "no write"}, toast "${(after.toasts.at(-1) ?? "").slice(0, 40)}"` };
  }

  const mouseTo = (x, y) => send("Input.dispatchMouseEvent", { type: "mouseMoved", x, y, button: "none" });
  const pressAt = async (x, y) => {
    await send("Input.dispatchMouseEvent", { type: "mousePressed", x, y, button: "left", clickCount: 1 });
    await send("Input.dispatchMouseEvent", { type: "mouseReleased", x, y, button: "left", clickCount: 1 });
  };
  // Timed by the clock and not by a count of moves, so a busy Mac still glides in the same time. Returns how long it took.
  const glide = async (from, to, ms) => {
    const start = Date.now();
    for (;;) {
      const f = Math.min(1, (Date.now() - start) / ms);
      await mouseTo(from.x + (to.x - from.x) * f, from.y + (to.y - from.y) * f);
      if (f === 1) return Date.now() - start;
      await sleep(Math.max(0, Math.min(16, ms - (Date.now() - start))));
    }
  };
  const keyOf = async (name, vk, modifiers = 0) => {
    await send("Input.dispatchKeyEvent", { type: "keyDown", key: name, code: name === "[" ? "BracketLeft" : name, windowsVirtualKeyCode: vk, modifiers });
    await send("Input.dispatchKeyEvent", { type: "keyUp", key: name, code: name === "[" ? "BracketLeft" : name, windowsVirtualKeyCode: vk, modifiers });
  };
  const typeWord = async (word) => {
    for (const ch of word) {
      const code = ch === " " ? "Space" : /\d/.test(ch) ? `Digit${ch}` : `Key${ch.toUpperCase()}`;
      const vk = ch.toUpperCase().charCodeAt(0);
      await send("Input.dispatchKeyEvent", { type: "keyDown", key: ch, code, text: ch, windowsVirtualKeyCode: vk });
      await send("Input.dispatchKeyEvent", { type: "keyUp", key: ch, code, windowsVirtualKeyCode: vk });
      await sleep(40);
    }
  };

  async function walkDone(t) {
    await open(t);
    const raw = await evaluate(`window.__trip.walkPair(${JSON.stringify(t.press)}, ${JSON.stringify(t.rows)})`);
    if (!raw) throw new Error(`no two rows running hold ${t.press.sel} ${t.press.text ?? ""}`.trim());
    const { from, to, shown, words: pressed, row, aimed } = JSON.parse(raw);
    if (!shown) throw new Error(`"${pressed}" on "${row.slice(0, 40)}" and the next row don't both fit on screen`);
    await mouseTo(from.x, from.y); // A row's actions take no pointer until it is hovered, so what's under a point is asked from there.
    await sleep(120);
    if ((await evaluate(`window.__trip.under(${from.x}, ${from.y})`)) !== pressed) throw new Error(`something else is at the middle of "${pressed}" on "${row.slice(0, 40)}"`);
    const before = await snap(t.rows);
    await pressAt(from.x, from.y);
    const took = await glide(from, to, t.glide ?? 190);
    await sleep(t.rest ?? 150);
    const under = await evaluate(`window.__trip.rowAt(${to.x}, ${to.y}, ${JSON.stringify(t.rows)})`);
    await pressAt(to.x, to.y);
    await sleep(300);
    await mouseTo(8, 8); // Off the list, so a hold lets go and the rows can be read.
    await opener.settleAfterChange();
    await sleep(300);
    const after = await snap(t.rows);
    const same = (r) => r.replace(/\d+/g, "#");
    const left = after.rows.map(same);
    const gone = before.rows.filter((r) => { const i = left.indexOf(same(r)); if (i < 0) return true; left.splice(i, 1); return false; });
    const has = (r) => gone.some((g) => same(g) === same(r));
    const short = (r) => `"${r.slice(0, 44)}"`;
    if (!gone.length) throw new Error(`"${pressed}" on ${short(row)} and the next row finished nothing`);
    const dy = Math.round(to.y - from.y);
    const walk = `"${pressed}" on ${short(row)}, ${dy} px down in ${took} ms, a press ${t.rest ?? 150} ms later`;
    const reasons = [];
    if (!has(aimed)) reasons.push(`${walk}, aimed at ${short(aimed)}: it is still there, the pointer being on ${short(under)}`);
    const others = gone.filter((r) => same(r) !== same(row) && same(r) !== same(aimed));
    if (others.length) reasons.push(`${walk}: ${tidy(others)} finished without being aimed at`);
    if (!has(row)) reasons.push(`${walk}: the row first pressed, ${short(row)}, is still there`);
    return { reasons: reasons.map((r) => `${r}, toast "${(after.toasts.at(-1) ?? "").slice(0, 40)}"`), note: `${walk}: ${gone.length} rows went, ${added(after, before).join(", ") || "no write yet"}` };
  }

  async function dialogTyping(t) {
    await open(t);
    const read = async () => JSON.parse((await evaluate(`window.__trip.dialog()`)) ?? "null");
    const was = await read();
    if (!was) throw new Error(`no dialog after the prep`);
    if (!was.selects.length) throw new Error(`"${was.title}" holds no select, so typing can't change one`);
    const other = was.selects.flatMap((s) => s.others).find((w) => /^[A-Za-z]/.test(w));
    const word = t.word ?? `${(other ?? "").split(" ")[0]} rota`;
    if (!t.word && !other) throw new Error(`no select in "${was.title}" has another option to type toward`);
    const on = was.active.label ? `${was.active.tag} "${was.active.label}"` : was.active.tag;
    const reasons = [];
    if (t.stays) {
      if (was.active.tag !== "select") reasons.push(`"${was.title}" is settled to start on its select, and focus is on ${on}`);
      return { reasons, note: `"${was.title}" starts on ${on}, as settled` };
    }
    if (was.arrive && !was.active.arrive) reasons.push(`"${was.title}" names "${was.arrive}" with data-arrive, and focus is on ${on}`);
    await typeWord(word);
    await sleep(150);
    const now = await read();
    if (!now) throw new Error(`typing "${word}" closed "${was.title}"`);
    const changed = now.selects.flatMap((s) => { const w = was.selects.find((x) => x.label === s.label); return w && w.value !== s.value ? [`${s.label} from "${w.shown}" to "${s.shown}"`] : []; });
    const landed = now.fields.find((f) => f.value.includes(word));
    if (changed.length) reasons.push(`"${was.title}", focus on ${on}: typing "${word}" changed ${changed.join(" and ")}${landed ? "" : `, and it landed in no field${now.fields.some((f) => !f.value) ? `, ${now.fields.filter((f) => !f.value).map((f) => `"${f.label}"`).join(", ")} staying empty` : ""}`}`);
    else if (!landed) reasons.push(`"${was.title}", focus on ${on}: typing "${word}" landed in no field`);
    return { reasons, note: `"${was.title}": focus on ${on}, "${word}" landed in ${landed ? `"${landed.label}"` : "no field"}, ${changed.length} selects changed` };
  }

  async function keptOpening(t) {
    if (t.size) await fit(...t.size);
    try { return await keptOpeningAt(t); } finally { if (t.size) await fit(1440, 900); }
  }

  async function keptOpeningAt(t) {
    const near = (a, b) => a !== null && Math.abs(a - b) <= 2;
    const read = (sel = t.scroller) => evaluate(`window.__trip.scrollOf(${JSON.stringify(sel)})`);
    const quiet = async (ms = 700) => { await opener.settleAfterChange(); await sleep(ms); }; // A return holds its scroll for 500 ms.
    await open(t);
    const from = await evaluate(`window.__trip.scrollTo(${JSON.stringify(t.scroller)}, ${t.at ?? 600})`);
    if (!from || from < 100) throw new Error(`${t.scroller} has nowhere to scroll (${from})`);
    await sleep(200);
    const reasons = [];
    let how = "";
    const pressRow = async (p) => {
      const raw = await evaluate(`window.__trip.point(${JSON.stringify(p)})`);
      if (!raw) throw new Error(`can't find ${p.sel} ${p.text ?? p.starts ?? ""}`.trim());
      const at = JSON.parse(raw);
      await mouseTo(at.x, at.y);
      await sleep(60);
      await pressAt(at.x, at.y);
      return at.words;
    };
    if (t.then === "other") { await go(t.via); await go(t.other); how = `${t.via}, then ${t.other}`; }
    if (t.then === "launcher") {
      await go(t.via);
      await key("Meta+k");
      await sleep(400);
      await typeWord(t.words);
      await sleep(500);
      await key("Enter");
      how = `${t.via}, then ⌘K "${t.words}"`;
    }
    if (t.then === "same") { await go(t.via); await go(t.route); how = `${t.via} and back`; }
    if (t.then === "press") { const said = await pressRow(t.press); how = `a press on "${said.slice(0, 30)}"`; }
    if (t.then === "esc") { await go(t.other); await key("Escape"); how = `${t.other}, then Esc`; }
    if (t.then === "back") { await go(t.other); await keyOf("[", 219, 4); how = `${t.other}, then ⌘[`; }
    if (t.then === "continue") { if (t.via) await go(t.via); await go("docs"); await pressRow(t.press); how = `${t.via ? `${t.via}, ` : ""}Docs, *Continue reading*`; }
    if (t.then === "landing") {
      await go(t.via);
      if (t.press) { await go(t.route); const said = await pressRow(t.press); how = `${t.via} and back, then "${said.slice(0, 30)}"`; }
      else { await go(t.other); how = `${t.via}, then ${t.other}`; }
    }
    await quiet();
    const hash = await evaluate(`location.hash`);
    const top = await read();
    const main = await read(".tf-main");
    const wantHash = t.then === "press" ? hash : t.then === "other" || t.then === "launcher" || t.then === "landing" ? `#/${t.other}` : `#/${t.route}`;
    if (hash !== wantHash) throw new Error(`${how} is at ${hash}, not ${wantHash}`);
    if (t.then === "other" || t.then === "launcher") {
      if (top !== 0) reasons.push(`${t.scroller} scrolled to ${from}, ${how}: it opens at ${top}, not 0`);
      if (main) reasons.push(`.tf-main opens at ${main} after ${how}`);
    }
    if (t.then === "press" && hash === `#/${t.route}` && !t.stays) reasons.push(`${how} didn't change the address, so it proves nothing`);
    if (t.then === "same" || t.then === "esc" || t.then === "back" || t.then === "continue" || t.then === "press") {
      if (!near(top, from)) reasons.push(`${t.scroller} scrolled to ${from}, ${how}: it reads ${top}`);
    }
    let landed = "";
    if (t.then === "landing") {
      const raw = await evaluate(`window.__trip.landing(${JSON.stringify(t.scroller)}, ${JSON.stringify(t.to)})`);
      if (!raw) throw new Error(`no ${t.to} at ${hash} after ${how}`);
      const l = JSON.parse(raw);
      landed = `, ${t.to} ${l.gap} px below the top`;
      if (t.top && !(l.gap >= -2 && l.gap <= 80) && !l.atEnd) reasons.push(`${how}: ${t.to} is ${l.gap} px below the top of ${t.scroller}, at scroll ${l.top}`);
      if (!t.top && !l.inView) reasons.push(`${how}: ${t.to} is out of view, ${l.gap} px from the top of a ${l.height} px ${t.scroller}, at scroll ${l.top}`);
    }
    return { reasons, note: `from ${from}, ${how}: reads ${top}${landed}` };
  }

  async function stripWalk(t) {
    const reasons = [];
    const notes = [];
    try {
      for (const [w, h] of t.sizes) {
        await fit(w, h);
        await open(t);
        const col = await evaluate(`window.__trip.focusRow(${JSON.stringify(t.row)})`);
        if (col === null || col === undefined) throw new Error(`can't focus "${t.row}" at ${w}`);
        const read = async () => JSON.parse(await evaluate(`window.__trip.strip(${JSON.stringify(col)})`));
        const start = await read();
        if (start.x === null || start.pane === null) throw new Error(`no column ${col} or pane at ${w}`);
        const moves = [];
        const step = async (name, vk) => {
          const was = await read();
          await keyOf(name, vk);
          await opener.settleAfterChange();
          const now = await read();
          const arrow = name === "ArrowUp" ? "↑" : "↓";
          const dx = now.x === null ? null : Math.abs(now.x - start.x);
          const dp = now.pane === null ? null : Math.abs(now.pane - start.pane);
          if (dx === null || dx > 1 || dp === null || dp > 1) moves.push(`${arrow} onto "${now.row}" moves the column from x ${start.x} to ${now.x} and the pane's left edge from ${start.pane} to ${now.pane} (${start.paneWidth} to ${now.paneWidth} wide)`);
          return { moved: now.row !== was.row, now };
        };
        let presses = 0;
        for (let i = 0; i < 40; i++) { const { moved } = await step("ArrowUp", 38); if (!moved) break; presses++; }
        if (!presses) throw new Error(`↑ from "${t.row}" moved nothing at ${w}`);
        const walked = presses;
        for (let i = 0; i < 40; i++) { const { moved } = await step("ArrowDown", 40); if (!moved) break; presses++; }
        if (presses === walked) throw new Error(`↓ moved nothing at ${w}`);
        if (moves.length) reasons.push(`at ${w} px, ${moves.length} of ${presses} presses move the strip: ${moves[0]}`);
        // Back up to a row with a ›, and open it.
        let on = await read();
        for (let i = 0; i < 40 && !on.holder; i++) { const { moved, now } = await step("ArrowUp", 38); on = now; if (!moved) break; }
        if (!on.holder) throw new Error(`no row with a › in column ${col} at ${w}`);
        await keyOf("ArrowRight", 39);
        await opener.settleAfterChange();
        const opened = await read();
        const next = Number(col) + 1;
        if (opened.col !== String(next)) reasons.push(`at ${w} px, → on "${on.row}" leaves focus on "${opened.row}" in column ${opened.col}, and column ${next} should have taken it`);
        notes.push(`${w} px: ${presses} presses, ${moves.length} moved it, → on "${on.row}" to "${opened.row}" in column ${opened.col}`);
      }
    } finally {
      await fit(1440, 900);
    }
    return { reasons, note: notes.join("; ") };
  }

  async function addSourceFold(t) {
    const reasons = [];
    const notes = [];
    const TOKEN = "trip-token-not-kept";
    try {
      for (const [w, h] of t.sizes) {
        await fit(w, h);
        await open(t);
        const read = async () => JSON.parse(await evaluate(`JSON.stringify((() => { const d = document.querySelector(".tf-dialog"); const a = document.activeElement; const rect = (e) => { if (!e) return null; const r = e.getBoundingClientRect(); return { x: r.left + Math.min(40, r.width / 2), y: r.top + r.height / 2, top: r.top, bottom: r.bottom }; }; const body = d?.querySelector(".tf-body")?.getBoundingClientRect(); return { open: !!d, rows: d ? d.querySelectorAll("input[name='tf-add-adapter']").length : 0, chosen: d?.querySelector("label:has(input[name='tf-add-adapter']:checked)")?.textContent.trim().slice(0, 12) ?? null, chosenBox: rect(d?.querySelector("label:has(input[name='tf-add-adapter']:checked)")), change: rect([...(d?.querySelectorAll("button") ?? [])].find((b) => b.textContent.trim() === "Change")), address: rect(d?.querySelector("#tf-add-address")), body: body ? { top: body.top, bottom: body.bottom } : null, token: d?.querySelector("#tf-add-secret")?.value ?? null, focus: a?.id || a?.type && a.tagName === "INPUT" && a.type || a?.textContent.trim().slice(0, 20), kept: JSON.stringify([Object.entries(localStorage), Object.entries(sessionStorage)]).includes(${JSON.stringify(TOKEN)}) }; })())`));
        const clickKind = async (word, count) => {
          const raw = await evaluate(`(() => { const l = [...document.querySelectorAll(".tf-dialog label")].find((x) => x.textContent.trim().startsWith(${JSON.stringify(word)})); if (!l) return null; const r = l.getBoundingClientRect(); return JSON.stringify({ x: r.left + 40, y: r.top + r.height / 2 }); })()`);
          if (!raw) throw new Error(`no kind "${word}" at ${w}`);
          const p = JSON.parse(raw);
          await mouseTo(p.x, p.y); await sleep(60);
          await pressAt(p.x, p.y);
          if (count === 2) { await sleep(120); await pressAt(p.x, p.y); }
          await opener.settleAfterChange();
        };
        const clickAt = async (p, count) => { await mouseTo(p.x, p.y); await sleep(60); await pressAt(p.x, p.y); if (count === 2) { await sleep(120); await pressAt(p.x, p.y); } await opener.settleAfterChange(); };
        const before = await snap();
        await clickKind("Jira", 2);
        let now = await read();
        if (now.rows !== 1 || now.chosen?.startsWith("Jira") !== true) reasons.push(`at ${w}, a double press on Jira left ${now.rows} rows, ${now.chosen} chosen`);
        if (now.focus !== "tf-add-address") reasons.push(`at ${w}, a press on a kind left focus on "${now.focus}"`);
        if (!now.chosenBox || !now.body || now.chosenBox.top < now.body.top - 1 || !now.address || now.address.bottom > now.body.bottom + 1) reasons.push(`at ${w}, the chosen row or the address is out of view after a pick: row ${now.chosenBox?.top}, address ${now.address?.bottom}, body ${now.body?.top} to ${now.body?.bottom}`);
        await mouseTo(4, 4);
        await sleep(800);
        await evaluate(`document.querySelector("#tf-add-secret").focus()`);
        await send("Input.insertText", { text: TOKEN });
        now = await read();
        await clickAt(now.change, 2);
        now = await read();
        const all = now.rows;
        if (all < 2) reasons.push(`at ${w}, Change opened ${all} rows`);
        if (now.focus !== "radio" || now.chosen?.startsWith("Jira") !== true) reasons.push(`at ${w}, a double press on Change left focus on "${now.focus}", ${now.chosen} chosen`);
        if (now.token !== TOKEN) reasons.push(`at ${w}, Change emptied the token typed for the kind it still shows`);
        if (now.kept) reasons.push(`at ${w}, the token is in storage after Change`);
        await keyOf("Escape", 27);
        await opener.settleAfterChange();
        now = await read();
        if (!now.open || now.rows !== 1 || now.focus !== "Change") reasons.push(`at ${w}, Esc from Change: open ${now.open}, ${now.rows} rows, focus "${now.focus}"`);
        await key("Enter");
        await key("Enter");
        await opener.settleAfterChange();
        now = await read();
        if (now.rows !== all || now.focus !== "radio") reasons.push(`at ${w}, Enter twice on Change: ${now.rows} rows, focus "${now.focus}"`);
        await keyOf("ArrowDown", 40);
        await opener.settleAfterChange();
        now = await read();
        if (now.rows !== all) reasons.push(`at ${w}, an arrow folded the list`);
        if (now.chosen?.startsWith("Jira") !== false) reasons.push(`at ${w}, an arrow picked nothing: ${now.chosen}`);
        if (now.token !== "") reasons.push(`at ${w}, another kind kept the token typed for the kind before`);
        await keyOf("Tab", 9);
        await opener.settleAfterChange();
        now = await read();
        if (now.rows !== 1 || now.focus !== "tf-add-address") reasons.push(`at ${w}, Tab after an arrow: ${now.rows} rows, focus "${now.focus}"`);
        await keyOf("Escape", 27);
        await opener.settleAfterChange();
        await sleep(100);
        await evaluate(`[...document.querySelectorAll("button")].find((b) => b.textContent.trim().startsWith("Add a source")).click()`);
        await opener.settleAfterChange();
        now = await read();
        if (now.rows !== 1 || now.token !== "" || now.kept || now.focus !== "tf-add-address") reasons.push(`at ${w}, the restored draft: ${now.rows} rows, token "${now.token}", kept ${now.kept}, focus "${now.focus}"`);
        const sent = added(await snap(), before);
        if (sent.length) reasons.push(`at ${w}, the trip wrote: ${sent.join(", ")}`);
        await evaluate(`[...document.querySelectorAll(".tf-dialog button")].find((b) => b.textContent.trim() === "Cancel").click()`);
        await opener.settleAfterChange();
        notes.push(`${w}: ${all} kinds, folds to 1`);
      }
    } finally {
      await fit(1440, 900);
    }
    return { reasons, note: notes.join("; ") };
  }

  async function stripPress(t) {
    const reasons = [];
    const notes = [];
    try {
      for (const [w, h] of t.sizes) {
        await fit(w, h);
        await open(t);
        const rowNames = (col) => evaluate(`JSON.stringify([...document.querySelectorAll('ol.col[data-col="' + ${JSON.stringify(col)} + '"] button.row .lbl')].map((l) => l.textContent.trim()))`).then(JSON.parse);
        const col = await evaluate(`window.__trip.focusRow(${JSON.stringify(t.row)})`);
        if (col === null || col === undefined) throw new Error(`can't find "${t.row}" at ${w}`);
        const read = async () => JSON.parse(await evaluate(`window.__trip.strip(${JSON.stringify(col)})`));
        const names = await rowNames(col);
        if (names.length < 3) throw new Error(`column ${col} has ${names.length} rows at ${w}`);
        const walk = [...names, ...names.slice(0, -1).reverse()];
        let last = await read();
        const moves = [];
        for (const name of walk) {
          const at = await evaluate(`(() => { const e = [...document.querySelectorAll('ol.col[data-col="' + ${JSON.stringify(col)} + '"] button.row')].find((b) => b.querySelector(".lbl")?.textContent.trim() === ${JSON.stringify(name)}); if (!e) return null; e.scrollIntoView({ block: "nearest" }); const r = e.getBoundingClientRect(); return JSON.stringify({ x: r.left + r.width / 2, y: r.top + r.height / 2 }); })()`);
          if (!at) throw new Error(`"${name}" has gone from column ${col} at ${w}`);
          const { x, y } = JSON.parse(at);
          await mouseTo(x, y);
          await sleep(60);
          await pressAt(x, y);
          await opener.settleAfterChange();
          const now = await read();
          const selected = await evaluate(`document.querySelector('ol.col[data-col="' + ${JSON.stringify(col)} + '"] button.row.tip .lbl')?.textContent.trim() ?? null`);
          if (selected !== name) reasons.push(`at ${w} px, a press on "${name}" selected "${selected}"`);
          if (now.x === null || now.pane === null || Math.abs(now.x - last.x) > 1 || Math.abs(now.pane - last.pane) > 1) moves.push(`"${name}" moves the column from x ${last.x} to ${now.x} and the pane's left edge from ${last.pane} to ${now.pane}`);
          last = now;
        }
        if (moves.length > 1) reasons.push(`at ${w} px, ${moves.length} of ${walk.length} presses move the strip: ${moves.slice(0, 3).join("; ")}`);
        notes.push(`${w} px: ${walk.length} presses, ${moves.length} moved it${moves.length ? ` (${moves[0]})` : ""}`);
      }
    } finally {
      await fit(1440, 900);
    }
    return { reasons, note: notes.join("; ") };
  }

  async function findKey(t) {
    const reasons = [];
    const shown = `(document.querySelector(".tf-kept:not([hidden]) .tf-rd-main") ?? document.querySelector(".tf-main"))`;
    const said = `document.activeElement?.getAttribute("aria-label") ?? null`;
    const press = async () => { await keyOf("f", 70, 4); await sleep(250); };
    await fit(1440, 900);
    await open(t);
    const from = await evaluate(`(() => { const e = ${shown}; if (e) e.scrollTop = 600; return e?.scrollTop ?? 0; })()`);
    await sleep(120);
    const was = await evaluate(`location.hash`);
    const held = await evaluate(said);
    await press();
    if (t.nothing) {
      if ((await evaluate(said)) !== held) reasons.push(`focus moved from ${held} to ${await evaluate(said)}`);
      if ((await evaluate(`location.hash`)) !== was) reasons.push("the address changed");
      return { reasons, note: `focus stayed on ${held ?? "the page"}` };
    }
    const at = await evaluate(said);
    if (at !== t.box) return { reasons: [`focus is on ${at ?? "the body"}, not ${t.box}`], note: "" };
    const seen = JSON.parse(await evaluate(`(() => { const r = document.activeElement.getBoundingClientRect(); return JSON.stringify({ top: r.top, bottom: r.bottom }); })()`));
    if (seen.top < 0 || seen.bottom > 900) reasons.push(`the box is off screen, ${Math.round(seen.top)} to ${Math.round(seen.bottom)}`);
    if (t.seed) return { reasons, note: `${t.box} holds focus` };
    const open0 = await evaluate(`${shown}?.scrollTop ?? 0`);
    await typeWord(t.words ?? "re");
    await sleep(300);
    if (t.count) {
      const read = await evaluate(`document.querySelector(".tf-kept:not([hidden]) .tf-findn")?.textContent ?? null`);
      if (read !== t.count) reasons.push(`it counts ${read}, not ${t.count}`);
      const moved = await evaluate(`${shown}?.scrollTop ?? 0`);
      if (moved < open0) reasons.push(`typing moved the page up, ${open0} to ${moved}`);
    }
    await evaluate(`document.activeElement.setSelectionRange(1, 1)`);
    await press();
    const sel = JSON.parse(await evaluate(`JSON.stringify([${said}, document.activeElement.selectionStart, document.activeElement.selectionEnd, document.activeElement.value?.length])`));
    if (sel[0] !== t.box) reasons.push(`pressed again, focus is on ${sel[0]}`);
    else if (sel[1] !== 0 || sel[2] !== sel[3]) reasons.push(`pressed again, the selection is ${sel[1]} to ${sel[2]} of ${sel[3]}`);
    for (let i = 0; i < 3 && (await evaluate(said)) === t.box; i++) {
      await key("Escape");
      await sleep(300);
    }
    if ((await evaluate(said)) === t.box) reasons.push("three Esc did not leave the box");
    if ((await evaluate(`location.hash`)) !== was) reasons.push("Esc out of the box left the page");
    if (await evaluate(`document.activeElement === document.body`)) reasons.push("Esc dropped focus on the body");
    if (t.count) {
      const back = await evaluate(`${shown}?.scrollTop ?? 0`);
      if (Math.abs(back - from) > 1) reasons.push(`Esc left the page at ${back}, not ${from}`);
    }
    return { reasons, note: `${t.box} holds focus, selects on a second press, and Esc stays on the page` };
  }

  // Round 16. A real press on a nav entry by its words, and the scroller a selector shows.
  const entryAt = async (name) => {
    const raw = await evaluate(`(() => { const a = [...document.querySelectorAll('.tf-nav a')].find((e) => e.textContent.trim().startsWith(${JSON.stringify(name)})); if (!a) return null; const r = a.getBoundingClientRect(); return JSON.stringify({ x: r.left + r.width / 2, y: r.top + r.height / 2 }); })()`);
    if (!raw) throw new Error(`no nav entry "${name}"`);
    return JSON.parse(raw);
  };
  const pressEntry = async (name) => {
    const at = await entryAt(name);
    await mouseTo(at.x, at.y);
    await sleep(60);
    await pressAt(at.x, at.y);
  };
  const shownScroll = (sel) => `[...document.querySelectorAll(${JSON.stringify(sel)})].find((e) => e.getClientRects().length && !e.closest('[hidden]'))`;
  const scrollTop = async (sel) => { const v = await evaluate(`${shownScroll(sel)}?.scrollTop ?? null`); return v === null ? null : Math.round(v); };
  const setScroll = (sel, top) => evaluate(`(() => { const e = ${shownScroll(sel)}; if (!e) return null; e.scrollTop = ${top}; return Math.round(e.scrollTop); })()`);

  async function navReturn(t) {
    const reasons = [];
    const notes = [];
    try {
      for (const [w, h] of t.sizes ?? [[1440, 560], [1100, 560]]) {
        await fit(w, h);
        await open(t);
        if (t.enter) { await pressEntry(t.section); await opener.settleAfterChange(); await sleep(200); }
        const wants = {};
        for (const [sel, top] of Object.entries(t.scroll)) {
          const got = await setScroll(sel, top);
          if (!got || got < 50) throw new Error(`${sel} has nowhere to scroll at ${w} x ${h} (${got})`);
          wants[sel] = got;
        }
        await sleep(150);
        for (const s of t.steps ?? []) {
          if (s.go) { await go(s.go); await sleep(150); }
          if (s.eval) { await evaluate(s.eval); await opener.settleAfterChange(); await sleep(150); }
          if (s.press) {
            const raw = await evaluate(`window.__trip.point(${JSON.stringify(s.press)})`);
            if (!raw) throw new Error(`can't find ${s.press.sel}`);
            const at = JSON.parse(raw);
            // Pointing at a row may bring it into view, and that is where the page is left.
            for (const sel of Object.keys(wants)) wants[sel] = await scrollTop(sel);
            await mouseTo(at.x, at.y);
            await sleep(60);
            await pressAt(at.x, at.y);
            await opener.settleAfterChange();
            await sleep(150);
          }
          if (s.scroll) { for (const [sel, top] of Object.entries(s.scroll)) await setScroll(sel, top); await sleep(150); }
        }
        if (!t.same) { await pressEntry(t.via ?? "Docs"); await opener.settleAfterChange(); await sleep(100); }
        await pressEntry(t.section);
        if (t.wheel) {
          await sleep(120);
          await send("Input.dispatchMouseEvent", { type: "mouseWheel", x: w / 2, y: h / 2, deltaX: 0, deltaY: t.wheel, button: "none" });
          await sleep(1600);
        } else {
          await opener.settleAfterChange();
          await sleep(900);
        }
        const hash = await evaluate(`location.hash`);
        const reads = {};
        for (const sel of Object.keys(wants)) {
          reads[sel] = await scrollTop(sel);
          const want = wants[sel] + (t.wheel ?? 0);
          if (reads[sel] === null || Math.abs(reads[sel] - want) > (t.wheel ? 30 : 2)) reasons.push(`at ${w} x ${h}, ${sel} was left at ${wants[sel]}${t.wheel ? ` and wheeled ${t.wheel}` : ""}: it reads ${reads[sel]} at ${hash}`);
        }
        const focus = await evaluate(`document.activeElement?.closest('.tf-nav a')?.textContent.trim() ?? document.activeElement?.tagName ?? 'nothing'`);
        if (!focus.startsWith(t.section)) reasons.push(`at ${w} x ${h}, focus went to ${focus}, not the ${t.section} entry`);
        if (!t.wheel) {
          await key("Escape");
          await sleep(350);
          const after = await evaluate(`location.hash`);
          if (after !== hash) reasons.push(`at ${w} x ${h}, Esc after a nav return went to ${after}`);
          for (const sel of Object.keys(wants)) if (Math.abs((await scrollTop(sel)) - reads[sel]) > 2) reasons.push(`at ${w} x ${h}, Esc moved ${sel}`);
        }
        notes.push(`${w}: ${Object.entries(reads).map(([s, v]) => `${s} ${v}`).join(", ")}, focus ${focus}`);
      }
    } finally {
      await fit(1440, 900);
    }
    return { reasons, note: notes.join("; ") };
  }

  // Each frame's tops of `rows`, as proof that nothing moved.
  const watchRows = (rows) => evaluate(`(() => { window.__tops = []; window.__topsOn = true; const f = () => { if (!window.__topsOn) return; window.__tops.push([...document.querySelectorAll(${JSON.stringify(rows)})].filter((e) => e.getClientRects().length && !e.closest('[hidden]')).map((e) => Math.round(e.getBoundingClientRect().top)).join()); requestAnimationFrame(f); }; f(); })()`);
  const rowMoves = async () => JSON.parse(await evaluate(`(() => { window.__topsOn = false; return JSON.stringify({ frames: window.__tops.length, distinct: [...new Set(window.__tops)] }); })()`));
  const startHold = async (t) => {
    const raw = await evaluate(`window.__trip.walkPair(${JSON.stringify(t.press)}, ${JSON.stringify(t.rows.split(",")[0])})`);
    if (!raw) throw new Error(`no two rows running hold ${t.press.sel} ${t.press.text ?? ""}`.trim());
    const pair = JSON.parse(raw);
    await mouseTo(pair.from.x, pair.from.y);
    await sleep(120);
    if ((await evaluate(`window.__trip.under(${pair.from.x}, ${pair.from.y})`)) !== pair.words) throw new Error(`something else is at the middle of "${pair.words}"`);
    return pair;
  };
  const toastUndo = async () => {
    const raw = await evaluate(`(() => { const b = [...document.querySelectorAll('.tf-toast button')].find((e) => e.textContent.trim() === 'Undo'); if (!b) return null; const r = b.getBoundingClientRect(); return JSON.stringify({ x: r.left + r.width / 2, y: r.top + r.height / 2 }); })()`);
    return raw ? JSON.parse(raw) : null;
  };
  const HELD = ".tf-ib-held, .tf-tm-rowheld";

  async function holdToUndo(t) {
    const reasons = [];
    const notes = [];
    const read = () => evaluate(`[...document.querySelectorAll(${JSON.stringify(t.rows)})].filter((e) => e.getClientRects().length && !e.closest('[hidden]')).map((e) => Math.round(e.getBoundingClientRect().top)).join()`);
    try {
      for (const [w, h] of t.sizes ?? [[1440, 900]]) {
        await fit(w, h);
        await open(t);
        const { from } = await startHold(t);
        const first = await read();
        await pressAt(from.x, from.y);
        await sleep(120);
        const undo = await toastUndo();
        if (!undo) throw new Error(`no Undo on a toast after the press at ${w} x ${h}`);
        await watchRows(t.rows);
        let to = undo;
        if (t.aim) {
          const raw = await evaluate(`(() => { const e = document.querySelector(${JSON.stringify(t.aim)}); if (!e) return null; const r = e.getBoundingClientRect(); return JSON.stringify({ x: r.left + Math.min(r.width / 2, 30), y: r.top + r.height / 2 }); })()`);
          if (!raw) throw new Error(`no ${t.aim}`);
          to = JSON.parse(raw);
        }
        await glide(from, to, 300);
        await sleep(100);
        await send("Input.dispatchMouseEvent", { type: "mousePressed", x: to.x, y: to.y, button: "left", clickCount: 1 });
        const moves = await rowMoves();
        if (moves.distinct.length > 1) reasons.push(`at ${w} x ${h}, rows moved before the press landed, over ${moves.frames} frames: ${moves.distinct.slice(0, 3).join(" then ")}`);
        await send("Input.dispatchMouseEvent", { type: "mouseReleased", x: to.x, y: to.y, button: "left", clickCount: 1 });
        await sleep(500);
        if (t.aim && !(await evaluate(`!!document.querySelector(${JSON.stringify(t.opens)})`))) reasons.push(`at ${w} x ${h}, the press on ${t.aim} landed on nothing: ${t.opens} isn't there`);
        if (!t.aim) {
          const after = await read();
          if (after !== first) reasons.push(`at ${w} x ${h}, after Undo the rows stand at ${after}, not ${first}`);
        }
        if (await evaluate(`!!document.querySelector(${JSON.stringify(HELD)})`)) reasons.push(`at ${w} x ${h}, a row still holds after the press landed`);
        notes.push(`${w}: ${moves.frames} frames still`);
      }
    } finally {
      await fit(1440, 900);
    }
    return { reasons, note: notes.join("; ") };
  }

  async function holdGutter(t) {
    await open(t);
    const { from, to, words, aimed } = await startHold(t);
    await pressAt(from.x, from.y);
    await sleep(120);
    await watchRows(t.rows);
    const right = await evaluate(`Math.round(document.querySelector(${JSON.stringify(t.list)}).getBoundingClientRect().right)`);
    const x = right + 5;
    const y = (from.y + to.y) / 2;
    await glide(from, { x, y }, 200);
    await sleep(t.wait ?? 7200); // The toast goes, and the gutter still counts as the list.
    await glide({ x, y }, to, 200);
    await sleep(100);
    const moves = await rowMoves();
    const reasons = [];
    if (moves.distinct.length > 1) reasons.push(`a drift through x ${x}, right of the list at ${right}, moved rows: ${moves.distinct.slice(0, 3).join(" then ")}`);
    const under = await evaluate(`window.__trip.under(${to.x}, ${to.y})`);
    if (under !== words) reasons.push(`back at the next row, "${under}" is under the pointer, not "${words}"`);
    const row = await evaluate(`window.__trip.rowAt(${to.x}, ${to.y}, ${JSON.stringify(t.rows)})`);
    const same = (r) => r.replace(/[0-9]+/g, "#");
    if (same(row) !== same(aimed)) reasons.push(`back at the next row, the pointer is on "${row.slice(0, 44)}", not "${aimed.slice(0, 44)}"`);
    return { reasons, note: `${moves.frames} frames still, x ${x} with the list ending at ${right}` };
  }

  async function holdLeave(t) {
    await open(t);
    const before = await snap(t.rows);
    const { from } = await startHold(t);
    await pressAt(from.x, from.y);
    await sleep(120);
    await pressEntry(t.via);
    await opener.settleAfterChange();
    await sleep(100);
    await pressEntry(t.section);
    await opener.settleAfterChange();
    await sleep(400);
    const back = await snap(t.rows);
    const reasons = [];
    if (back.rows.length !== before.rows.length - 1) reasons.push(`back during the toast, ${back.rows.length} rows, from ${before.rows.length}: it should be one fewer`);
    if (await evaluate(`!!document.querySelector(${JSON.stringify(HELD)})`)) reasons.push(`a hold outlived the leave`);
    const undo = await toastUndo();
    if (!undo) throw new Error(`the toast had gone by the return`);
    await mouseTo(undo.x, undo.y);
    await sleep(60);
    await pressAt(undo.x, undo.y);
    await sleep(600);
    const after = await snap(t.rows);
    if (after.rows.length !== before.rows.length) reasons.push(`after Undo, ${after.rows.length} rows, from ${before.rows.length}`);
    return { reasons, note: `rows ${before.rows.length}, ${back.rows.length}, then ${after.rows.length} after Undo` };
  }

  async function commentView(t) {
    if (!t.size) return commentViewAt(t);
    await fit(...t.size);
    try { return await commentViewAt(t); } finally { await fit(1440, 900); }
  }
  async function commentViewAt(t) {
    await open(t);
    const parts = `(() => { const box = [...document.querySelectorAll('.tf-compose textarea')].find((e) => e.getClientRects().length && !e.closest('[hidden]')); return { box, send: [...box.closest('.tf-compose').querySelectorAll('button')].find((e) => e.textContent.trim() === 'Comment') }; })()`;
    await evaluate(`${parts}.box.focus()`);
    await typeWord("looks right");
    await sleep(100);
    const fit6 = await evaluate(`(() => { const { box, send } = ${parts}; let s = box; while (s && s !== document.body) { s = s.parentElement; if (s && /auto|scroll/.test(getComputedStyle(s).overflowY) && s.scrollHeight > s.clientHeight) { s.scrollTop += send.getBoundingClientRect().bottom - s.getBoundingClientRect().bottom + 6; return s.scrollHeight - s.clientHeight - s.scrollTop; } } return null; })()`);
    if (fit6 === null || fit6 === undefined) throw new Error("the composer's page doesn't scroll, so a send can't push it out of view");
    await sleep(150);
    if (t.how === "key") {
      await send("Input.dispatchKeyEvent", { type: "keyDown", key: "Enter", code: "Enter", windowsVirtualKeyCode: 13, modifiers: 4 });
      await send("Input.dispatchKeyEvent", { type: "keyUp", key: "Enter", code: "Enter", windowsVirtualKeyCode: 13, modifiers: 4 });
    } else {
      const at = JSON.parse(await evaluate(`(() => { const r = ${parts}.send.getBoundingClientRect(); return JSON.stringify({ x: r.left + r.width / 2, y: r.top + r.height / 2 }); })()`));
      await mouseTo(at.x, at.y);
      await sleep(60);
      await pressAt(at.x, at.y);
    }
    await sleep(600);
    const read = JSON.parse(await evaluate(`(() => { const { box, send } = ${parts}; const edge = document.querySelector('.tf-main').getBoundingClientRect().bottom; return JSON.stringify({ edge: Math.round(edge), box: Math.round(box.getBoundingClientRect().bottom), send: Math.round(send.getBoundingClientRect().bottom), focus: document.activeElement === box }); })()`));
    const reasons = [];
    if (read.box > read.edge) reasons.push(`the box ends at ${read.box}, under the status bar at ${read.edge}`);
    if (read.send > read.edge) reasons.push(`*Comment* ends at ${read.send}, under the status bar at ${read.edge}`);
    if (!read.focus) reasons.push(`focus left the box`);
    return { reasons, note: `box to ${read.box}, *Comment* to ${read.send}, status bar at ${read.edge}` };
  }

  // Round 17: a key the page itself moves by, with its real codes, because `key` only knows a few by name.
  const READ_KEYS = { PageDown: [34, "PageDown"], PageUp: [33, "PageUp"], " ": [32, "Space"], ArrowDown: [40, "ArrowDown"], ArrowUp: [38, "ArrowUp"], Home: [36, "Home"], End: [35, "End"] };
  const readKey = async (name) => {
    const [keyCode, code] = READ_KEYS[name];
    await send("Input.dispatchKeyEvent", { type: "keyDown", key: name, code, windowsVirtualKeyCode: keyCode, ...(name === " " ? { text: " " } : {}) });
    await send("Input.dispatchKeyEvent", { type: "keyUp", key: name, code, windowsVirtualKeyCode: keyCode });
    await sleep(350);
  };
  // A point on plain words in `sel`, in view, not on a control.
  const wordsAt = async (sel) => {
    const raw = await evaluate(`(() => {
      const box = ${shownScroll(sel)};
      if (!box) return null;
      const edge = box.getBoundingClientRect();
      const walk = document.createTreeWalker(box, NodeFilter.SHOW_TEXT);
      for (let n = walk.nextNode(); n; n = walk.nextNode()) {
        if (n.textContent.trim().length < 12 || n.parentElement.closest('a, button, input, select, textarea, summary, label, [role=button], [tabindex="0"]')) continue;
        const r = document.createRange();
        const at = n.textContent.search(/\\S/);
        r.setStart(n, at); r.setEnd(n, at + 8);
        const q = r.getClientRects()[0];
        if (!q || q.width < 4 || q.top < edge.top + 4 || q.bottom > Math.min(edge.bottom, innerHeight) - 30 || q.left < edge.left || q.right > edge.right - 12) continue;
        const x = q.left + q.width / 2, y = q.top + q.height / 2;
        const under = document.elementFromPoint(x, y);
        if (!under || !box.contains(under) || under.closest('a, button, input, select, textarea, summary, label')) continue;
        return JSON.stringify({ x, y, right: q.right + 60, words: n.textContent.trim().slice(0, 30) });
      }
      return null;
    })()`);
    if (!raw) throw new Error(`no plain words in view in ${sel}`);
    return JSON.parse(raw);
  };
  const ringOn = () => evaluate(`(() => { const a = document.activeElement; if (!a || a === document.body) return ''; const s = getComputedStyle(a); return s.outlineStyle !== 'none' && parseFloat(s.outlineWidth) > 0 ? a.tagName + '.' + a.className : ''; })()`);
  const focusName = () => evaluate(`(() => { const a = document.activeElement; return a ? a.tagName.toLowerCase() + (a.className ? '.' + String(a.className).split(' ').slice(0, 2).join('.') : '') : 'nothing'; })()`);

  async function scrollerKeys(t) {
    const reasons = [];
    const notes = [];
    const [w, h] = t.size ?? [1440, 600];
    try {
      for (const name of t.keys ?? ["PageDown", " ", "ArrowDown"]) {
        await fit(w, h);
        await open(t);
        await sleep(250);
        const set = t.at ? await setScroll(t.scroller, t.at) : await scrollTop(t.scroller);
        await sleep(100);
        if (t.how === "text" || t.how === "twice" || t.how === "nav") {
          const p = await wordsAt(t.scroller);
          await mouseTo(p.x, p.y);
          await sleep(60);
          await pressAt(p.x, p.y);
          await sleep(100);
          if (t.how === "twice") { await pressAt(p.x, p.y); await sleep(100); }
          const moved = await scrollTop(t.scroller);
          if (Math.abs(moved - (set ?? 0)) > 1) reasons.push(`a press in the words moved ${t.scroller} from ${set} to ${moved}`);
        }
        if (t.how === "tile") {
          const raw = await evaluate(`window.__trip.point(${JSON.stringify(t.tile)})`);
          if (!raw) throw new Error(`can't find ${t.tile.sel}`);
          const at = JSON.parse(raw);
          await mouseTo(at.x, at.y);
          await sleep(60);
          await pressAt(at.x, at.y);
          await opener.settleAfterChange();
          await sleep(300);
        }
        if (t.how === "nav") {
          await pressEntry(t.via ?? "Inbox");
          await opener.settleAfterChange();
          await sleep(100);
          await pressEntry(t.section);
          await opener.settleAfterChange();
          await sleep(800);
        }
        const was = await scrollTop(t.scroller);
        if (was === null) throw new Error(`no ${t.scroller} on the page`);
        const room = await evaluate(`(() => { const e = ${shownScroll(t.scroller)}; return e.scrollHeight - e.clientHeight - e.scrollTop; })()`);
        if (room < 60) throw new Error(`${t.scroller} has ${room} px to go at ${w} x ${h}`);
        await readKey(name);
        const now = await scrollTop(t.scroller);
        const on = await focusName();
        const ring = await ringOn();
        if (now <= was) reasons.push(`${name === " " ? "Space" : name} after ${t.how} left ${t.scroller} at ${was}, focus on ${on}`);
        if (ring) reasons.push(`after ${t.how} and ${name === " " ? "Space" : name}, ${ring} draws a ring`);
        notes.push(`${name === " " ? "Space" : name} ${was} to ${now}, focus ${on}`);
      }
    } finally {
      await fit(1440, 900);
    }
    return { reasons, note: notes.join("; ") };
  }

  async function scrollerSelect(t) {
    const reasons = [];
    const [w, h] = t.size ?? [1440, 600];
    try {
      await fit(w, h);
      await open(t);
      await sleep(250);
      const was = await scrollTop(t.scroller);
      const p = await wordsAt(t.in ?? t.scroller);
      await mouseTo(p.x, p.y);
      await send("Input.dispatchMouseEvent", { type: "mousePressed", x: p.x, y: p.y, button: "left", clickCount: 1 });
      await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: p.x + 30, y: p.y, button: "left", buttons: 1 });
      await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: p.right, y: p.y, button: "left", buttons: 1 });
      await send("Input.dispatchMouseEvent", { type: "mouseReleased", x: p.right, y: p.y, button: "left", clickCount: 1 });
      await sleep(100);
      const picked = await evaluate(`getSelection().toString()`);
      if (!picked) reasons.push(`a drag across "${p.words}" selected nothing`);
      if ((await scrollTop(t.scroller)) !== was) reasons.push(`the drag moved ${t.scroller}`);
      const on = await focusName();
      await readKey("PageDown");
      const now = await scrollTop(t.scroller);
      if (now <= was) reasons.push(`PageDown after the drag left ${t.scroller} at ${was}, focus on ${on}`);
      const still = await evaluate(`getSelection().toString()`);
      if (still !== picked) reasons.push(`PageDown changed the selection from "${picked}" to "${still}"`);
      const ring = await ringOn();
      if (ring) reasons.push(`${ring} draws a ring`);
      return { reasons, note: `selected "${picked}", focus ${on}, PageDown ${was} to ${now}, selection then "${still}"` };
    } finally {
      await fit(1440, 900);
    }
  }

  async function paneKeys(t) {
    const reasons = [];
    const [w, h] = t.size ?? [1440, 460];
    try {
      await fit(w, h);
      await open(t);
      await sleep(250);
      const edge = JSON.parse(await evaluate(`(() => { const e = ${shownScroll("aside.pane")}; const r = e.getBoundingClientRect(); return JSON.stringify({ x: r.left, y: r.top, w: r.width, h: r.height, room: e.scrollHeight - e.clientHeight }); })()`));
      if (t.how !== "row" && t.how !== "column" && edge.room < 40) throw new Error(`the pane has ${edge.room} px to go at ${w} x ${h}`);
      const at = { x: edge.x + edge.w / 2, y: edge.y + edge.h / 2 };
      const wheel = (x, y) => send("Input.dispatchMouseEvent", { type: "mouseWheel", x, y, deltaX: 0, deltaY: 60, button: "none" });
      if (t.how === "text") { const p = await wordsAt("aside.pane"); await mouseTo(p.x, p.y); await sleep(60); await pressAt(p.x, p.y); }
      if (t.how === "blank") { await mouseTo(edge.x + 5, at.y); await sleep(60); await pressAt(edge.x + 5, at.y); }
      if (t.how === "wheel") { await mouseTo(at.x, at.y); await wheel(at.x, at.y); await sleep(250); }
      if (t.how === "link" || t.how === "row") {
        const raw = await evaluate(`window.__trip.point(${JSON.stringify(t.how === "link" ? t.link : t.row)})`);
        if (!raw) throw new Error(`can't find what to press in ${t.how}`);
        const p = JSON.parse(raw);
        await mouseTo(p.x, p.y);
        await sleep(60);
        await pressAt(p.x, p.y);
        await opener.settleAfterChange();
        await sleep(400);
      }
      if (t.how === "column") {
        const raw = await evaluate(`(() => { const r = ${shownScroll("ol.col")}.getBoundingClientRect(); return JSON.stringify({ x: r.left + r.width / 2, y: r.top + r.height / 2 }); })()`);
        const p = JSON.parse(raw);
        await mouseTo(p.x, p.y);
        await wheel(p.x, p.y);
        await sleep(250);
      }
      await sleep(100);
      const hash = await evaluate(`location.hash`);
      const was = await scrollTop("aside.pane");
      const on = await focusName();
      await readKey("ArrowDown");
      await opener.settleAfterChange();
      const after = await evaluate(`location.hash`);
      const now = await scrollTop("aside.pane");
      if (t.walks) {
        if (after === hash) reasons.push(`↓ after ${t.how} stayed on ${hash}, focus on ${on}: the walk should answer`);
        return { reasons, note: `${hash} to ${after}, focus ${on}` };
      }
      if (after !== hash) reasons.push(`↓ after ${t.how} replaced the asset with ${after}, focus on ${on}`);
      else if (now <= was && (t.how !== "link" || edge.room >= 40) && (await evaluate(`(() => { const e = ${shownScroll("aside.pane")}; return e.scrollHeight - e.clientHeight - e.scrollTop; })()`)) > 2) reasons.push(`↓ after ${t.how} left the pane at ${was}, focus on ${on}`);
      if (after === hash) {
        await readKey("ArrowUp");
        if ((await evaluate(`location.hash`)) !== hash) reasons.push(`↑ after ${t.how} replaced the asset`);
      }
      const ring = await ringOn();
      if (ring) reasons.push(`${ring} draws a ring`);
      return { reasons, note: `↓ ${was} to ${now} on ${after === hash ? "the same asset" : after}, focus ${on}` };
    } finally {
      await fit(1440, 900);
    }
  }

  // Round 17's way back.
  const stillAt = async (p) => {
    const raw = await evaluate(`window.__trip.point(${JSON.stringify(p)})`);
    if (!raw) throw new Error(`can't find ${p.sel} ${p.text ?? p.starts ?? ""}`.trim());
    const at = JSON.parse(raw);
    await mouseTo(at.x, at.y);
    await sleep(60);
    return at;
  };
  const press1 = async (p) => { const at = await stillAt(p); await pressAt(at.x, at.y); await opener.settleAfterChange(); await sleep(150); return at; };
  const toEntry = async (name) => { await pressEntry(name); await opener.settleAfterChange(); await sleep(100); };
  const hashNow = () => evaluate(`location.hash`);

  async function navReopen(t) {
    const reasons = [];
    const notes = [];
    const left = `#/${t.left}`;
    const bare = `#/${t.bare}`;
    const list = `#/${t.route}`;
    try {
      for (const [w, h] of t.sizes ?? [[1440, 560], [1100, 560]]) {
        for (const how of t.how ?? ["pointer", "key"]) {
          const where = `at ${w} x ${h} by ${how}`;
          await fit(w, h);
          await open(t);
          await go(t.left);
          await sleep(400);
          if (t.fill && !(await evaluate(`window.__trip.fill(${JSON.stringify(t.fill.sel)}, ${JSON.stringify(t.fill.text)})`))) throw new Error(`can't fill ${t.fill.sel}`);
          const want = await setScroll(t.scroller, t.at);
          if (!want || want < 50) throw new Error(`${t.scroller} has nowhere to scroll ${where} (${want})`);
          await sleep(150);
          for (const via of t.far ?? []) await toEntry(via);
          if (t.via !== null) await toEntry(t.via ?? "Docs");
          if (t.then === "launcher") {
            await key("Meta+k");
            await sleep(400);
            await typeWord(t.words);
            await sleep(500);
            await key("Enter");
            await opener.settleAfterChange();
            await sleep(700);
            const reads = await scrollTop(t.scroller);
            if ((await hashNow()) !== bare) reasons.push(`${where}, the launcher's jump went to ${await hashNow()}, not ${bare}`);
            if (reads > 1) reasons.push(`${where}, the launcher's jump opened at ${reads}, not the top`);
            notes.push(`${w} launcher ${reads}`);
            continue;
          }
          if (t.then === "back") {
            await keyOf("[", 219, 4);
            await opener.settleAfterChange();
            await sleep(700);
            if ((await hashNow()) !== left) throw new Error(`${where}, ⌘[ went to ${await hashNow()}, not ${left}`);
            if (Math.abs((await scrollTop(t.scroller)) - want) > 2) reasons.push(`${where}, ⌘[ read ${await scrollTop(t.scroller)}, left at ${want}`);
            await toEntry("Docs");
            await keyOf("[", 219, 4);
            await opener.settleAfterChange();
            await sleep(300);
            await key("Escape");
            await opener.settleAfterChange();
            await sleep(300);
            await press1(t.row);
            await sleep(500);
            if ((await hashNow()) !== bare) reasons.push(`${where}, a row after ⌘[ and Esc went to ${await hashNow()}, not ${bare}`);
            notes.push(`${w} back`);
            continue;
          }
          await toEntry(t.section);
          const landed = await hashNow();
          if (landed !== list) reasons.push(`${where}, the entry landed on ${landed}, not ${list}`);
          if (t.then === "tab") {
            await press1(t.tab);
            await sleep(500);
            if ((await hashNow()) !== `#/${t.tabTo}`) reasons.push(`${where}, a link naming a tab went to ${await hashNow()}, not #/${t.tabTo}`);
            notes.push(`${w} tab`);
            continue;
          }
          if (t.then === "other") {
            await press1(t.other);
            await sleep(500);
            const other = await hashNow();
            if (other === left || other === bare || !other.startsWith("#/build/")) reasons.push(`${where}, another run's row went to ${other}`);
            const reads = await scrollTop(t.scroller);
            if (reads > 1) reasons.push(`${where}, another run's row opened at ${reads}`);
            await key("Escape");
            await opener.settleAfterChange();
            await sleep(400);
            await press1(t.row);
            await sleep(600);
            if ((await hashNow()) !== bare) reasons.push(`${where}, the first run's row after another's went to ${await hashNow()}, not ${bare}`);
            if ((await scrollTop(t.scroller)) > 1) reasons.push(`${where}, the first run's row after another's opened at ${await scrollTop(t.scroller)}`);
            notes.push(`${w} other ${other.slice(-12)}`);
            continue;
          }
          const steps = await evaluate(`history.length`);
          if (how === "pointer") {
            const at = await stillAt(t.row);
            await pressAt(at.x, at.y);
            await sleep(120);
            await pressAt(at.x, at.y);
          } else {
            await sleep(500); // The press guard holds a key press on another control for 400 ms after the entry. A person goes by Tab, which ends that; this focus is a script's.
            if ((await evaluate(`window.__trip.focusPress(${JSON.stringify(t.row)})`)) == null) throw new Error(`can't focus ${t.row.sel}`);
            await key("Enter");
            await sleep(120);
            await key("Enter");
          }
          await opener.settleAfterChange();
          await sleep(900);
          const hash = await hashNow();
          const reads = await scrollTop(t.scroller);
          const pushed = (await evaluate(`history.length`)) - steps;
          const focus = await evaluate(`document.activeElement?.tagName ?? "nothing"`);
          if (hash !== left) reasons.push(`${where}, the row twice went to ${hash}, not ${left}`);
          if (reads === null || Math.abs(reads - want) > 2) reasons.push(`${where}, ${t.scroller} was left at ${want} and reads ${reads}`);
          if (steps < 50 && pushed !== 1) reasons.push(`${where}, the row twice made ${pushed} history entries, not 1`);
          if (await evaluate(`!!document.querySelector('.tf-dialog')`)) reasons.push(`${where}, the second press opened a dialog`);
          const kept = t.fill ? await evaluate(`document.querySelector(${JSON.stringify(t.fill.sel)})?.value ?? null`) : null;
          await key("Escape");
          await opener.settleAfterChange();
          await sleep(500);
          if (t.fill && kept) { await key("Escape"); await sleep(120); await key("Escape"); await opener.settleAfterChange(); await sleep(500); }
          const out = await hashNow();
          if (out !== list) reasons.push(`${where}, Esc after the reopen went to ${out}, not ${list}`);
          await press1(t.row);
          await sleep(500);
          const again = await hashNow();
          const top = await scrollTop(t.scroller);
          if (again !== bare) reasons.push(`${where}, a press after Esc went to ${again}, not ${bare}: it is used once`);
          if (top > 1) reasons.push(`${where}, a press after Esc opened at ${top}, not the top`);
          notes.push(`${w} ${how}: ${reads} of ${want}, focus ${focus}${t.fill ? `, ${t.fill.sel} reads "${kept}"` : ""}`);
        }
      }
    } finally {
      await fit(1440, 900);
    }
    return { reasons, note: notes.join("; ") };
  }

  async function navDocs(t) {
    const reasons = [];
    const notes = [];
    const folds = `JSON.stringify([...document.querySelectorAll('.tf-kept:not([hidden]) .tf-dl-body details')].filter((d) => d.getClientRects().length && d.open).map((d) => d.querySelector('summary').textContent.trim().replace(/\\s+/g, ' ')))`;
    const open_ = async () => JSON.parse(await evaluate(folds));
    try {
      for (const [w, h] of t.sizes ?? [[1440, 900], [1100, 760]]) {
        await fit(w, h);
        if (t.then === "search") {
          await open(t);
          const search = `#/${t.route}`;
          const clear = { sel: ".tf-kept:not([hidden]) button", text: "Clear" };
          const hits = `document.querySelectorAll('.tf-kept:not([hidden]) .tf-dl-body a[href^="#/page/"]').length`;
          const had = await evaluate(hits);
          if (!had) throw new Error("the search found nothing");
          await toEntry("Inbox");
          await toEntry("Docs");
          await sleep(300);
          if ((await hashNow()) !== search) reasons.push(`at ${w}, the entry went to ${await hashNow()}, not ${search}`);
          if ((await evaluate(hits)) !== had) reasons.push(`at ${w}, ${await evaluate(hits)} hits came back of ${had}`);
          if (!(await evaluate(`window.__trip.point(${JSON.stringify(clear)})`))) reasons.push(`at ${w}, the search has no Clear on its return`);
          if (!(await evaluate(`document.activeElement?.closest('.tf-nav a') !== null`))) reasons.push(`at ${w}, focus left the entry`);
          await press1({ sel: ".tf-kept:not([hidden]) .tf-dl-body a[href^='#/page/']" });
          await sleep(300);
          if (!(await hashNow()).startsWith("#/page/")) reasons.push(`at ${w}, a hit went to ${await hashNow()}`);
          await toEntry("Docs");
          await sleep(300);
          if ((await hashNow()) !== search) reasons.push(`at ${w}, the entry from a hit went to ${await hashNow()}, not ${search}`);
          await press1(clear);
          await sleep(300);
          if ((await hashNow()) !== "#/docs") reasons.push(`at ${w}, Clear went to ${await hashNow()}, not #/docs`);
          await toEntry("Inbox");
          await toEntry("Docs");
          await sleep(300);
          if ((await hashNow()) !== "#/docs") reasons.push(`at ${w}, the entry after Clear went to ${await hashNow()}, not #/docs`);
          if (await evaluate(`window.__trip.point(${JSON.stringify(clear)})`)) reasons.push(`at ${w}, Clear is back after Clear`);
          notes.push(`${w}: ${had} hits`);
        } else {
          await open(t);
          await sleep(300);
          const space = { sel: ".tf-kept:not([hidden]) .tf-dl-body details > summary.tf-spc", starts: "Engineering" };
          const inner = { sel: ".tf-kept:not([hidden]) .tf-dl-body details details > summary", starts: "Payments" };
          const arrow = async (p) => { const raw = await evaluate(`window.__trip.point(${JSON.stringify(p)})`); if (!raw) throw new Error(`can't find ${p.starts}`); const at = JSON.parse(raw); await mouseTo(at.x, at.y); await sleep(40); await pressAt(at.x, at.y); await sleep(200); };
          await arrow(space);
          const first = await open_();
          if (!first.some((f) => f.startsWith("Engineering"))) throw new Error(`pressing Engineering opened ${first}`);
          // A nested fold's summary holds a link, so a press on its edge or a key opens it. The tree's own keys are what a person has.
          await evaluate(`[...document.querySelectorAll('.tf-kept:not([hidden]) .tf-dl-body details details > summary')].find((s) => s.textContent.trim().startsWith('Payments')).click()`);
          await sleep(200);
          const before = await open_();
          if (!before.some((f) => f.startsWith("Payments"))) throw new Error(`no Payments fold open: ${before}`);
          await toEntry("Inbox");
          await toEntry("Docs");
          await sleep(300);
          const after = await open_();
          if (after.join() !== before.join()) reasons.push(`at ${w}, folds ${before} came back as ${after}`);
          // The same set on the space's own page, and closed there it is closed on the home.
          await go("docs/space/ENG");
          await sleep(300);
          const onSpace = await open_();
          if (!onSpace.some((f) => f.startsWith("Payments"))) reasons.push(`at ${w}, the space's own page shows ${onSpace}, without the fold opened on the home`);
          await evaluate(`[...document.querySelectorAll('.tf-kept:not([hidden]) .tf-dl-body details > summary')].find((s) => s.textContent.trim().startsWith('Payments')).click()`);
          await sleep(200);
          await toEntry("Inbox");
          await toEntry("Docs");
          await sleep(300);
          const shut = await open_();
          if (shut.some((f) => f.startsWith("Payments"))) reasons.push(`at ${w}, a fold closed by the reader came back open: ${shut}`);
          if (!shut.some((f) => f.startsWith("Engineering"))) reasons.push(`at ${w}, Engineering closed on its own: ${shut}`);
          await arrow(space);
          await toEntry("Inbox");
          await toEntry("Docs");
          await sleep(300);
          if ((await open_()).some((f) => f.startsWith("Engineering"))) reasons.push(`at ${w}, Engineering closed by the reader came back open`);
          notes.push(`${w}: ${after.join(", ")}`);
        }
      }
    } finally {
      await fit(1440, 900);
    }
    return { reasons, note: notes.join("; ") };
  }

  async function gearReturn(t) {
    const reasons = [];
    const notes = [];
    const gear = { sel: ".tf-top a[aria-label='Settings']" };
    const pane = ".tf-stg-scroll";
    const here = `#/${t.route}`;
    try {
      for (const [w, h] of t.sizes ?? [[1440, 620], [1100, 620]]) {
        await fit(w, h);
        await open(t);
        await sleep(300);
        const want = await setScroll(pane, 349);
        if (!want || want < 50) throw new Error(`${pane} has nowhere to scroll at ${w} (${want})`);
        await sleep(150);
        // By pointer, twice with a still pointer, and by ⌘,.
        await toEntry("Inbox");
        const at = await stillAt(gear);
        await pressAt(at.x, at.y);
        await sleep(120);
        await pressAt(at.x, at.y);
        await opener.settleAfterChange();
        await sleep(800);
        if ((await hashNow()) !== here) reasons.push(`at ${w}, the gear went to ${await hashNow()}, not ${here}`);
        if (Math.abs((await scrollTop(pane)) - want) > 2) reasons.push(`at ${w}, the gear read ${await scrollTop(pane)}, left at ${want}`);
        await key("Escape");
        await opener.settleAfterChange();
        await sleep(400);
        if ((await hashNow()) !== "#/inbox") reasons.push(`at ${w}, Esc from the gear went to ${await hashNow()}, not #/inbox`);
        await keyOf(",", 188, 4);
        await opener.settleAfterChange();
        await sleep(800);
        if ((await hashNow()) !== here) reasons.push(`at ${w}, ⌘, went to ${await hashNow()}, not ${here}`);
        if (Math.abs((await scrollTop(pane)) - want) > 2) reasons.push(`at ${w}, ⌘, read ${await scrollTop(pane)}, left at ${want}`);
        // On the section: nothing moves, whichever way it's asked, and Esc still leaves.
        await keyOf(",", 188, 4);
        await sleep(300);
        if ((await evaluate(`window.__trip.focusPress(${JSON.stringify(gear)})`)) == null) throw new Error("can't focus the gear");
        await key("Enter");
        await sleep(120);
        await key("Enter");
        await sleep(400);
        if ((await hashNow()) !== here || Math.abs((await scrollTop(pane)) - want) > 2) reasons.push(`at ${w}, the gear on its own section moved to ${await hashNow()} at ${await scrollTop(pane)}`);
        await key("Escape");
        await opener.settleAfterChange();
        await sleep(300);
        if ((await hashNow()) !== "#/inbox") reasons.push(`at ${w}, Esc after the gear on its own section went to ${await hashNow()}`);
        // A source's page goes to Sources, from the page and after leaving it.
        await go(t.source);
        await sleep(300);
        await toEntry("Inbox");
        await press1(gear);
        await sleep(300);
        if ((await hashNow()) !== "#/settings/sources") reasons.push(`at ${w}, the gear after a source's page went to ${await hashNow()}`);
        await go(t.source);
        await sleep(300);
        await press1(gear);
        await sleep(300);
        if ((await hashNow()) !== "#/settings/sources") reasons.push(`at ${w}, the gear on a source's page went to ${await hashNow()}`);
        // The status bar's Sources goes to Sources, from another section too.
        await go(t.route);
        await sleep(200);
        await press1({ sel: "a.tf-sources" });
        await sleep(300);
        if ((await hashNow()) !== "#/settings") reasons.push(`at ${w}, the status bar's Sources went to ${await hashNow()}`);
        notes.push(`${w}: ${want}`);
      }
    } finally {
      await fit(1440, 900);
    }
    return { reasons, note: notes.join("; ") };
  }

  async function walkPlace(t) {
    const reasons = [];
    const notes = [];
    const inView = `(() => { const r = document.activeElement?.closest(${JSON.stringify(t.rows)}); if (!r) return null; const b = r.getBoundingClientRect(); const m = document.querySelector('.tf-main').getBoundingClientRect(); return { top: b.top >= m.top - 1, bottom: b.bottom <= m.bottom + 1, id: r.dataset.row ?? r.dataset.entity }; })()`;
    const walk = (name) => key(name);
    const read = async (what, want) => {
      await sleep(150);
      const got = await scrollTop(t.scroller);
      const at = await evaluate(inView);
      if (at === null) reasons.push(`${what}: focus is on no row`);
      else if (!at.top || !at.bottom) reasons.push(`${what}: the row isn't in view`);
      if (want !== undefined && Math.abs(got - want) > 2) reasons.push(`${what}: scroll went from ${want} to ${got}`);
      return at?.id;
    };
    try {
      for (const [w, h] of t.sizes ?? [[1440, 560], [1100, 560]]) {
        // From scroll 0: the first row, the next, and back.
        await fit(w, h);
        await open(t);
        await sleep(200);
        await walk("j");
        const a = await read(`at ${w}, j at scroll 0`);
        await walk("j");
        const b = await read(`at ${w}, j again`);
        await walk("k");
        const c = await read(`at ${w}, k after j j`);
        if (a === b) reasons.push(`at ${w}, the second j stayed on the same row`);
        if (c !== a) reasons.push(`at ${w}, k came to ${c}, not back to ${a}`);
        // A return: the first key keeps the place.
        await open(t);
        await sleep(200);
        const want = await setScroll(t.scroller, t.at);
        if (!want || want < 50) throw new Error(`${t.scroller} has nowhere to scroll at ${w} (${want})`);
        await sleep(150);
        await toEntry("Docs");
        await toEntry(t.section);
        await sleep(900);
        const back = await scrollTop(t.scroller);
        await walk("j");
        const first = await read(`at ${w}, j after a return`, back);
        await open(t);
        await sleep(200);
        await setScroll(t.scroller, t.at);
        await sleep(150);
        await toEntry("Docs");
        await toEntry(t.section);
        await sleep(900);
        const again = await scrollTop(t.scroller);
        await walk("k");
        const last = await read(`at ${w}, k with no row focused after a return`, again);
        notes.push(`${w}: j on ${first}, k on ${last} at ${back}`);
      }
    } finally {
      await fit(1440, 900);
    }
    return { reasons, note: notes.join("; ") };
  }

  async function fieldInView(t) {
    const reasons = [];
    const notes = [];
    const [w, h] = t.size ?? [1440, 900];
    try {
      await fit(w, h);
      await open(t);
      await sleep(300);
      if (!(await evaluate(`window.__trip.click(${JSON.stringify(t.open)})`))) throw new Error(`can't find ${t.open.sel}`);
      await sleep(250);
      for (let n = 0; n < (t.times ?? 4); n += 1) {
        await evaluate(`(() => { const f = document.activeElement; const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set; set.call(f, 'An item number ${n}'); f.dispatchEvent(new Event('input', { bubbles: true })); })()`);
        await key("Enter");
        await sleep(300);
        const raw = JSON.parse(await evaluate(`(() => { const f = document.activeElement; const r = f.getBoundingClientRect(); return JSON.stringify({ label: f.getAttribute('aria-label'), bottom: Math.round(r.bottom), top: Math.round(r.top), edge: Math.round(document.querySelector('.tf-status').getBoundingClientRect().top), head: Math.round(document.querySelector('.tf-main').getBoundingClientRect().top) }); })()`));
        if (raw.label !== t.field) reasons.push(`after Enter ${n + 1} focus is on ${raw.label}`);
        else if (raw.bottom > raw.edge) reasons.push(`after Enter ${n + 1} the field ends at ${raw.bottom}, under the status bar at ${raw.edge}`);
        else if (raw.top < raw.head) reasons.push(`after Enter ${n + 1} the field starts at ${raw.top}, above the page at ${raw.head}`);
        notes.push(`${raw.top} to ${raw.bottom}`);
      }
    } finally {
      await fit(1440, 900);
    }
    return { reasons, note: notes.join(", ") };
  }

  async function pressHolds(t) {
    const reasons = [];
    const rect = () => evaluate(`(() => { const b = [...document.querySelectorAll(${JSON.stringify(t.press.sel)})].find((e) => e.getClientRects().length && !e.closest('[hidden]') && e.textContent.trim() === ${JSON.stringify(t.press.text)}); return b ? Math.round(b.getBoundingClientRect().top) : null; })()`);
    const lineBelow = () => evaluate(`(() => { const l = [...document.querySelectorAll(${JSON.stringify(t.line)})].find((e) => e.getClientRects().length && !e.closest('[hidden]')); const b = [...document.querySelectorAll(${JSON.stringify(t.press.sel)})].find((e) => e.getClientRects().length && !e.closest('[hidden]') && e.textContent.trim() === ${JSON.stringify(t.press.text)}); return !!l && l.getBoundingClientRect().top >= b.getBoundingClientRect().bottom; })()`);
    await open(t);
    await sleep(250);
    const raw = await evaluate(`window.__trip.point(${JSON.stringify(t.press)})`);
    if (!raw) throw new Error(`can't find ${t.press.sel}`);
    const p = JSON.parse(raw);
    await evaluate(`[...document.querySelectorAll(${JSON.stringify(t.press.sel)})].find((e) => e.textContent.trim() === ${JSON.stringify(t.press.text)})?.scrollIntoView({ block: "center" })`);
    await sleep(150);
    const q = JSON.parse(await evaluate(`window.__trip.point(${JSON.stringify(t.press)})`));
    const top = await rect();
    await mouseTo(q.x, q.y);
    await sleep(100);
    for (const n of [1, 2]) {
      await pressAt(q.x, q.y);
      await sleep(500);
      const now = await rect();
      if (now !== top) reasons.push(`pointer press ${n} moved "${t.press.text}" from ${top} to ${now}`);
      const under = await evaluate(`window.__trip.under(${q.x}, ${q.y})`);
      if (under !== t.press.text) reasons.push(`after pointer press ${n} the pointer is on "${under}"`);
    }
    if (!(await lineBelow())) reasons.push(`${t.line} didn't draw below "${t.press.text}"`);
    await open(t);
    await sleep(250);
    const { words } = await focusPress(t);
    const keyTop = await rect();
    await key("Enter");
    await sleep(500);
    await key("Enter");
    await sleep(500);
    const keyNow = await rect();
    if (keyNow !== keyTop) reasons.push(`Enter twice moved "${words}" from ${keyTop} to ${keyNow}`);
    const focused = await evaluate(`document.activeElement?.textContent.trim()`);
    if (focused !== t.press.text) reasons.push(`after Enter twice focus is on "${focused}"`);
    await pressEntry(t.via ?? "Inbox");
    await opener.settleAfterChange();
    await sleep(100);
    await evaluate(`history.back()`);
    await opener.settleAfterChange();
    await sleep(500);
    const back = await rect();
    if (back === null) reasons.push(`"${t.press.text}" is gone after a leave and a return`);
    return { reasons, note: `top ${top} through two presses, line below, ${keyTop} through two Enters, back at ${back}` };
  }

  async function scrollerTab(t) {
    const reasons = [];
    const [w, h] = t.size ?? [1440, 420];
    try {
      await fit(w, h);
      await open(t);
      await sleep(250);
      const set = await setScroll(t.scroller, t.at ?? 120);
      if (!set || set < 40) throw new Error(`${t.scroller} has nowhere to scroll at ${w} x ${h}`);
      await sleep(100);
      const p = await wordsAt(t.scroller);
      await mouseTo(p.x, p.y);
      await sleep(60);
      await pressAt(p.x, p.y);
      await sleep(100);
      await key("Tab");
      await sleep(200);
      const now = await scrollTop(t.scroller);
      const on = await focusName();
      if (Math.abs(now - set) > 1) reasons.push(`Tab moved ${t.scroller} from ${set} to ${now}, focus on ${on}`);
      if (on.startsWith("body")) reasons.push(`Tab dropped focus on the body`);
      return { reasons, note: `${t.scroller} stays at ${now}, focus ${on}` };
    } finally {
      await fit(1440, 900);
    }
  }

  async function paneEsc(t) {
    const reasons = [];
    const [w, h] = t.size ?? [1440, 460];
    try {
      await fit(w, h);
      await open(t);
      await sleep(250);
      const p = await wordsAt("aside.pane");
      await mouseTo(p.x, p.y);
      await sleep(60);
      await pressAt(p.x, p.y);
      await sleep(100);
      const hash = await evaluate(`location.hash`);
      await key("Escape");
      await opener.settleAfterChange();
      await sleep(500);
      const after = await evaluate(`location.hash`);
      const on = await focusName();
      if (after === hash) reasons.push(`Esc stayed on ${hash}`);
      if (on.startsWith("aside.pane") || on.startsWith("body")) reasons.push(`Esc left focus on ${on}`);
      await readKey("ArrowDown");
      await opener.settleAfterChange();
      const walked = await evaluate(`location.hash`);
      // An only child has nowhere to walk, and the walk's answer is focus on its row.
      const then = await focusName();
      if (walked === after && !then.startsWith("button.row")) reasons.push(`↓ after the Esc stayed on ${after}, focus on ${then}: the walk should answer`);
      return { reasons, note: `${after}, focus ${on}, ↓ then ${walked}, focus ${then}` };
    } finally {
      await fit(1440, 900);
    }
  }

  // Round 17. ⌘F's one ladder, from a page scrolled to 600 where it can scroll.
  async function findLadder(t) {
    const reasons = [];
    const page = `([...document.querySelectorAll(".tf-kept:not([hidden]) .tf-scroller, .tf-main")].find((e) => getComputedStyle(e).overflowY !== "visible" && e.scrollHeight > e.clientHeight) ?? document.querySelector(".tf-main"))`;
    const said = `document.activeElement === document.body ? "the body" : document.activeElement?.getAttribute("aria-label")`;
    const top = () => evaluate(`Math.round(${page}.scrollTop)`);
    const count = () => evaluate(`document.querySelector(".tf-findhold .tf-findn")?.textContent ?? null`);
    const files = () => evaluate(`document.querySelectorAll(".tf-kept:not([hidden]) .tf-dfiles [data-file]").length`);
    const find = async () => { await keyOf("f", 70, 4); await sleep(250); };
    await fit(1440, 900);
    await open(t);
    await sleep(300);
    await evaluate(`document.activeElement?.blur?.()`);
    if (t.fallback) await evaluate(`window.Highlight = undefined; 1`);
    await evaluate(`${page}.scrollTop = 600`);
    await sleep(150);
    const from = await top();
    const was = await evaluate(`location.hash`);
    const tall = await evaluate(`${page}.scrollHeight`);
    const held = await files();
    await find();
    const at = await evaluate(said);
    if (at !== t.box) return { reasons: [`focus is on ${at}, not ${t.box}`], note: "" };
    if (!t.words) {
      if (t.chip) {
        const chip = await evaluate(`document.activeElement.parentElement?.parentElement?.textContent ?? ""`);
        if (!chip.includes(t.chip)) reasons.push(`the launcher opened without the ${t.chip} chip: "${chip.slice(0, 40)}"`);
      }
      return { reasons, note: `${t.box} holds focus` };
    }
    // The bar takes no room, and the box is on screen.
    if ((await top()) !== from) reasons.push(`opening moved the page, ${from} to ${await top()}`);
    if (Math.abs((await evaluate(`${page}.scrollHeight`)) - tall) > 1) reasons.push("opening changed the page's height");
    const seen = JSON.parse(await evaluate(`(() => { const r = document.activeElement.getBoundingClientRect(); return JSON.stringify({ top: r.top, bottom: r.bottom }); })()`));
    if (seen.top < 0 || seen.bottom > 900) reasons.push(`the box is off screen, ${Math.round(seen.top)} to ${Math.round(seen.bottom)}`);
    await typeWord(t.words);
    await sleep(250);
    if ((await files()) !== held) reasons.push(`typing changed which files show, ${held} to ${await files()}`);
    if (await evaluate(`!!document.querySelector(".tf-kept:not([hidden]) .tf-flist-none")`)) reasons.push("typing emptied the diff: No files match");
    const first = await count();
    if (!t.count.test(first ?? "")) reasons.push(`it counts "${first}"`);
    if (!t.fallback && !(await evaluate(`(CSS.highlights.get("tf-find-now")?.size ?? 0) === 1`))) reasons.push("no current match is drawn");
    await evaluate(`document.activeElement.setSelectionRange(1, 1)`);
    await find();
    const sel = JSON.parse(await evaluate(`JSON.stringify([document.activeElement.selectionStart, document.activeElement.selectionEnd, document.activeElement.value.length])`));
    if (sel[0] !== 0 || sel[1] !== sel[2]) reasons.push(`pressed again, the selection is ${sel[0]} to ${sel[1]} of ${sel[2]}`);
    // Esc goes back, unstepped.
    await key("Escape");
    await sleep(300);
    if (Math.abs((await top()) - from) > 1) reasons.push(`Esc left the page at ${await top()}, not ${from}`);
    if ((await evaluate(said)) === "the body") reasons.push("Esc dropped focus on the body");
    if (await evaluate(`!!document.querySelector(".tf-findhold")`)) reasons.push("Esc left the bar open");
    if (await evaluate(`CSS.highlights?.has?.("tf-find") ?? false`)) reasons.push("Esc left the marks drawn");
    if ((await evaluate(`location.hash`)) !== was) reasons.push("Esc left the page");
    // Stepped, Esc stays.
    await find();
    await typeWord(t.words);
    await sleep(150);
    await key("Enter");
    await sleep(200);
    if (t.fallback) {
      if (!(await evaluate(`getSelection().toString().toLowerCase() === ${JSON.stringify(t.words)}`))) reasons.push("with no marks, a step drew no selection");
      await typeWord("y");
      await sleep(150);
      const typed = await evaluate(`document.activeElement.value`);
      if (typed !== `${t.words}y`) reasons.push(`with no marks, a key typed after a step made "${typed}"`);
    }
    const stepped = await top();
    await key("Escape");
    await sleep(300);
    if (Math.abs((await top()) - stepped) > 1) reasons.push(`after a step, Esc moved the page, ${stepped} to ${await top()}`);
    return { reasons, note: `${first}, nothing moved as it opened, Esc back to ${from}` };
  }

  // Reconcile, round 17. Where the find bar meets the rest. `how: "box"`: a real press in `scroller`'s words, ⌘F, `words`, Esc: focus must be back on `scroller`, and PageDown must move it.
  // `how: "tab"`: ⌘F, `words`, then each of `tabs` by pointer. `{ text, stays }`: the bar is still there after it, with the words, or none is left at all, and Esc then closes the one or leaves by the page's own way.
  async function findMeet(t) {
    const reasons = [];
    const [w, h] = t.size ?? [1440, 600];
    const find = async () => { await keyOf("f", 70, 4); await sleep(250); };
    const bars = () => evaluate(`document.querySelectorAll(".tf-findhold").length`);
    await fit(w, h);
    await open(t);
    await sleep(250);
    try {
      if (t.how === "box") {
        await setScroll(t.scroller, 150);
        await sleep(120);
        const p = await wordsAt(t.scroller);
        await mouseTo(p.x, p.y);
        await sleep(60);
        await pressAt(p.x, p.y);
        await sleep(100);
        const on = await focusName();
        if (!on.includes(t.scroller.slice(1))) throw new Error(`the press left focus on ${on}`);
        await find();
        await typeWord(t.words);
        await sleep(250);
        await key("Escape");
        await sleep(300);
        const back = await focusName();
        if (!back.includes(t.scroller.slice(1))) reasons.push(`Esc left focus on ${back}, not ${t.scroller}`);
        const was = await scrollTop(t.scroller);
        await readKey("PageDown");
        const now = await scrollTop(t.scroller);
        if (now <= was) reasons.push(`PageDown left ${t.scroller} at ${was}`);
        return { reasons, note: `Esc back on ${back}, PageDown ${was} to ${now}` };
      }
      await find();
      await typeWord(t.words);
      await sleep(250);
      const notes = [];
      for (const tab of t.tabs) {
        const raw = await evaluate(`window.__trip.point(${JSON.stringify({ sel: ".tf-tabs a", starts: tab.text })})`);
        if (!raw) throw new Error(`no tab ${tab.text}`);
        const at = JSON.parse(raw);
        await mouseTo(at.x, at.y);
        await sleep(60);
        await pressAt(at.x, at.y);
        await opener.settleAfterChange();
        await sleep(200);
        const left = await bars();
        if (tab.stays && left !== 1) reasons.push(`${tab.text} left ${left} bars, not the one`);
        if (!tab.stays && left !== 0) reasons.push(`${tab.text} left ${left} bars standing on the page`);
        notes.push(`${tab.text} ${left}`);
      }
      return { reasons, note: `bars after: ${notes.join(", ")}` };
    } finally {
      await fit(1440, 900);
    }
  }

  async function rowsStill(t) {
    const reasons = [];
    const notes = [];
    const ways = t.ways ?? ["first", "return", "sync", "shown", "launcher"];
    const prep = async () => {
      for (const s of t.prep ?? []) {
        if (s.wait) await sleep(s.wait);
        if (s.click && !(await evaluate(`window.__trip.click(${JSON.stringify({ sel: s.click, text: s.text, starts: s.starts })})`))) throw new Error(`prep can't find ${s.click} "${s.text ?? s.starts ?? ""}"`);
      }
      if (t.prep) await opener.settleAfterChange();
    };
    // On the page already (shown, palette), the first frame is taken as it arms; arriving, it is the destination's first paint.
    const arm = (now = false) => evaluate(`(${STILL}, window.__still.arm(${JSON.stringify(t.rows)}, ${now}))`);
    // Settled: quiet, and longer than the latest late read the query holds back.
    const settled = async () => { await opener.settleAfterChange(); await sleep(t.tail ?? 600); await opener.settleAfterChange(); };
    const grow = async () => {
      if (!(await evaluate(`!!window.__fake`))) throw new Error("no window.__fake: the query needs fake-hooks");
      if (t.grow) await evaluate(t.grow);
      await evaluate(`window.__fake.endRuns(), 1`);
    };
    for (const way of ways) {
      await opener.openScreen({ name: t.id, route: way === "first" || way === "launcher" ? t.via : t.route, query: t.query });
      if (way !== "first" && way !== "launcher") await prep();
      await evaluate(`(${STILL}, 1)`);
      if (way === "first") { await arm(); await evaluate(`location.hash = ${JSON.stringify(`#/${t.route}`)}`); }
      if (way === "return") { await go(t.via); await arm(); await evaluate(`location.hash = ${JSON.stringify(`#/${t.route}`)}`); }
      if (way === "sync") { await go(t.via); await sleep(1000); await grow(); await sleep(300); await arm(); await evaluate(`location.hash = ${JSON.stringify(`#/${t.route}`)}`); }
      // A sync lands a while after the page drew, once the run states have been read (`role-sync`'s `ran` counts from then).
      if (way === "shown") {
        await sleep(1000);
        if (t.rest) {
          const at = JSON.parse(await evaluate(`JSON.stringify((() => { const b = [...document.querySelectorAll(${JSON.stringify(t.rest)})].find((e) => e.getClientRects().length)?.getBoundingClientRect(); return b ? { x: Math.round(b.left + 20), y: Math.round(b.top + b.height / 2) } : null; })())`));
          if (at === null) { reasons.push(`shown: no ${t.rest} to rest the pointer on`); continue; }
          await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: at.x, y: at.y });
          await sleep(200);
        }
        await arm(true);
        await grow();
      }
      if (way === "palette") {
        // The launcher's own list, opened on the page: its home, then whatever reads land after.
        await arm();
        await key("Meta+k");
      }
      if (way === "launcher") {
        await arm();
        await key("Meta+k");
        await sleep(400);
        await typeWord(t.words);
        await sleep(600);
        await key("Enter");
      }
      // After a sync, the page reads again when its run ends: wait for a frame that differs, or `tail`, before settling.
      if (way === "shown" || way === "sync") {
        const until = Date.now() + (t.tail ?? 600) + 1500;
        while (Date.now() < until && (await evaluate(`(window.__still.frames?.length ?? 0) < 2`))) await sleep(50);
      }
      await settled();
      const hash = await evaluate(`location.hash`);
      const v = JSON.parse(await evaluate(`window.__still.verdict()`));
      if (way === "palette") await key("Escape");
      if (hash !== `#/${t.route}`) { reasons.push(`${way}: ended at ${hash}, not #/${t.route}`); continue; }
      if (!v.drew) { reasons.push(`${way}: no ${t.rows} drew in view`); continue; }
      if (v.problem) reasons.push(`${way}: ${v.problem}`);
      else notes.push(`${way} drew at ${v.firstAt} ms, ${v.rows} rows still through ${v.frames} frames to ${v.last} ms`);
      if (way === "shown" && t.held && !v.problem) {
        const held = JSON.stringify(t.pill ?? ".tf-fheld");
        const words = await evaluate(`[...document.querySelectorAll(${held})].find((b) => b.getClientRects().length)?.textContent.trim() ?? ""`);
        if (!/^Show \d+ new( runs?)?$/.test(words)) { reasons.push(`shown: the rows the sync brought wait behind no *Show N new* (${t.pill ?? "the bar's end"} reads "${words}")`); continue; }
        const before = await evaluate(`document.querySelectorAll(${JSON.stringify(t.rows)}).length`);
        await evaluate(`[...document.querySelectorAll(${held})].find((b) => b.getClientRects().length).click(), 1`);
        await opener.settleAfterChange();
        const after = await evaluate(`document.querySelectorAll(${JSON.stringify(t.rows)}).length`);
        const left = await evaluate(`[...document.querySelectorAll(${held})].find((b) => b.getClientRects().length)?.textContent.trim() ?? ""`);
        if (left !== "") reasons.push(`shown: after "${words}" ${t.pill ?? "the bar"} still offers "${left}"`);
        notes.push(`"${words}" brought ${after - before >= 0 ? after - before : 0} more in view (windowed lists draw what fits)`);
      }
    }
    return { reasons, note: notes.join("; ") };
  }

  const KINDS = { "rows-still": rowsStill, "find-meet": findMeet, "find-ladder": findLadder, "scroller-tab": scrollerTab, "pane-esc": paneEsc, "field-in-view": fieldInView, "press-holds": pressHolds, "nav-reopen": navReopen, "nav-docs": navDocs, "gear-return": gearReturn, "walk-place": walkPlace, "scroller-keys": scrollerKeys, "scroller-select": scrollerSelect, "pane-keys": paneKeys, "comment-view": commentView, "nav-return": navReturn, "hold-to-undo": holdToUndo, "hold-gutter": holdGutter, "hold-leave": holdLeave, "find-key": findKey, "add-source-fold": addSourceFold, "strip-press": stripPress, "rail-top": railTop, "aimed-after-close": aimedAfterClose, "enter-twice": enterTwice, "leave-back": leaveBack, "esc-sweep": escSweep, "enter-fast": enterFast, "enter-walks": enterWalks, "pointer-twice": pointerTwice, "undo-stale": undoStale, "column-filter": columnFilter, "row-room": rowRoom, "walk-done": walkDone, "dialog-typing": dialogTyping, "kept-opening": keptOpening, "strip-walk": stripWalk };
  for (let t = queue.shift(); t; t = queue.shift()) {
    const began = Date.now();
    const base = { id: t.id, kind: t.kind, finding: t.finding, page: t.page, route: t.route };
    try {
      const { reasons, note } = await KINDS[t.kind](t);
      results.push({ ...base, status: reasons.length ? "fail" : "pass", reasons, note, took: Date.now() - began });
    } catch (err) {
      results.push({ ...base, status: "can't run", reasons: [String(err?.message ?? err).split("\n")[0]], note: "", took: Date.now() - began });
    }
    const last = results.at(-1);
    console.log(`${last.status.padEnd(9)} ${t.kind} ${t.id} ${(last.took / 1000).toFixed(1)} s`);
  }
  stop();
}

const queue = every.filter(wanted);
if (!queue.length) {
  console.error(`no trip matches --only ${only.join(",")}${table.some((t) => t.strict && wanted(t)) ? ". Some strict ones do: add --strict" : ""}`);
  process.exit(2);
}
const total = queue.length;
console.log(`${total} trips with ${workers} workers`);
const leftOut = strict ? 0 : table.filter((t) => t.strict && wanted(t)).length;
if (leftOut) console.log(`${leftOut} strict trips left out. --strict runs them, and they fail until Round 14's fixes are in`);
const results = [];
const startedAt = Date.now();
await Promise.all(Array.from({ length: workers }, () => worker(queue, results)));
const took = (Date.now() - startedAt) / 1000;
const order = every.map((t) => t.id);
results.sort((a, b) => order.indexOf(a.id) - order.indexOf(b.id));
writeFileSync(join(outDir, "trips.json"), JSON.stringify(results, null, 1));

const fails = results.filter((r) => r.status === "fail");
const cant = results.filter((r) => r.status === "can't run");
const passes = results.filter((r) => r.status === "pass");
const line = (r) => `- **${r.kind}** \`${r.id}\` (${r.finding}, \`#/${r.route}\`): ${r.reasons.join(". ")}.`;
const md = [
  `# Trips`,
  ``,
  `${total} trips in ${took.toFixed(0)} s. **${fails.length} failing, ${cant.length} that can't run**, ${passes.length} passing.`,
  ``,
  `## Failing`,
  ``,
  ...(fails.length ? fails.map(line) : ["None."]),
  ``,
  `## Can't run`,
  ``,
  ...(cant.length ? cant.map(line) : ["None."]),
  ``,
  `## Passing`,
  ``,
  ...(passes.length ? passes.map((r) => `- **${r.kind}** \`${r.id}\`: ${r.note}.`) : ["None."]),
  ``,
].join("\n");
writeFileSync(join(outDir, "trips.md"), md);
console.log(`${total} trips in ${took.toFixed(0)} s: ${fails.length} failing, ${cant.length} can't run, ${passes.length} passing. See ${join(outDir, "trips.md")}`);
process.exit(fails.length || cant.length ? 1 : 0);
