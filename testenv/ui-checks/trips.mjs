// Trips across knobas under ?fake-ipc in headless Chrome: what a person does on a second press, on leaving and coming back, and on Esc.
// It presses things, from the table below, written by hand. testenv/README.md, *UI checks*, says how to run it.
//
// Usage:
//   testenv/ui-checks/build-app.sh . <folder>
//   testenv/ui-checks/with-built.sh <folder> <port> node testenv/ui-checks/trips.mjs <out dir> --port <port> [options]
// Options:
//   --only a,b      only trips whose id, kind, finding, page or route contains one of these
//   --workers n     parallel Chromes (default 2)
//   --part k/m      every m-th trip from the k-th of those picked, so m runs share them (a run too long for one turn on a
//                   machine's queue, the final suite, 2026-11-22)
//   --gap n         ms between the two presses of a trip with `leaves` or `returns`, to prove one fails once its hold has let go
//   --trip-limit s  the most one trip may take (default 180; the longest real ones took 77 s on the loaded Mac). A trip past it
//                   fails by name, its Chrome is stopped and a fresh one takes the rest (`trip-runner.mjs`).
//   --strict        also run the trips marked `strict`, and hold esc-sweep to the scroller rule. They fail on a head without Round 14's
//                   fixes, so a plain run leaves them out and gives the same trips and verdicts as before. The whole-round check runs both.
//
// Writes <out dir>/trips.json and trips.md, and trips.log as it goes: each trip's line, and a progress line every minute naming
// what each worker is on. Exits 1 if a trip fails or can't run: a trip that no longer finds its control says so.
//
// Kinds, and what fails:
//   enter-twice   focus a control, press Enter twice with real key events. Fails if the second Enter changes the address, opens a
//                 dialog or drawer, or sends a write, or if the press lands on a control with different words. A control that keeps
//                 focus and changes its words is a hold, not a landing. A second write is fine only once the pressed row has left
//                 the list and focus took the next row's same words (the keeper's rule), which needs the trip's `rows`.
//                 `leaves`: the first Enter is meant to change the address. The second must still write and open nothing, and stay there.
//                 `returns`: the press closes a dialog or an editor and focus goes back to the control with these words, as the spec asks.
//                 That landing is accepted; a second Enter that writes, reopens or moves still fails.
//                 `fits`: a menu the prep opened (a selector), which must stand inside the window with no row of it wider than the
//                 menu, before the first Enter (the render-as pass, 2026-11-19: a renderer's long program ran out of *Render as*).
//                 `returnsCounting`: the control focus goes back to counts as it goes (*Cloning 3 of 9…*), so `returns` is its start.
//                 `opens`: the first Enter is meant to open a dialog (*Clone all…*, PR #2479). The second must leave it open and send nothing. These trips press again after `gap`
//                 ms (200 by default), inside the hold. --gap n presses after n ms instead, so --gap 1000 shows the trip can fail.
//   leave-back    press by key, which raises a toast and hides a row. While the toast is up, go to `via` and back. Fails if the rows
//                 differ from just after the press. Can't run if the press hides no row or the toast has gone by the return.
//                 `rowsStay`: the press isn't meant to hide one (*Save link set*), so the rows are only compared. `ack`: the press hands
//                 something to another program (*Try it*), which a chip says for a second (ux21), so a chip stands for the toast
//                 and nothing has to span the leave. `reopen`: what to press on the
//                 return, for rows in something a leave closes. `backTo`: where to come back to, for a press that itself leaves (*Create pull
//                 request* lands on *Pull requests*); the route by default.
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
//                 `deeper`: `steps` went deeper in the section (a run, a ticket, a pull request), so the entry first comes back there (2026-11-13), and the
//                 checks are of a second press, once the press's hold has let go: the section's own page as it was left.
//   nav-place     2026-11-13 (Björn: "when i select a page in docs, then go somewhere else and then return to docs my selected page isn't shown").
//                 At `route`, a place deeper than the section's own page, `scroller` at `at` if named: by pointer, `via`'s entry (Inbox if absent),
//                 then `section`'s pressed twice with a still pointer; by key, Enter twice on the focused entry. Fails unless that lands on `route` on one
//                 history entry, as the same kept page (not drawn anew), `scroller` where it was left, focus on the entry (the heading by key). Once the press's hold has
//                 let go, a press must go to `top` (a prefix of the address). `toast`: { press, rows }, pressed before the leave, whose toast must
//                 still stand on the return with the rows as they were after the press. `section: "Settings"` is the gear. `far`: entries pressed
//                 first, enough to push the page out of keep-alive, so it is drawn anew and must still come back at its scroll.
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
//                 `route`. Press `row` twice with a still pointer, or Enter twice. Fails unless that opens `left` at `at` on one history entry. Esc must go to `route` (to `esc` where the page sits under another, #2559), and `row` again must open `bare` at 0, as it is used once.
//                 From another section the entry first comes back to `left` at `at` (2026-11-13), and a second press, once the press's hold has let go, is `route`.
//                 `far`: more entries on the way, which push the page out of keep-alive. `then: "other"`: `other`'s row first, which opens `otherBare` at 0 and ends the memory. `then: "launcher"`: ⌘K `words` from `via`, which opens `bare` at 0.
//                 `then: "tab"`: `tab`'s link, which opens `tabTo`, not `left`. `then: "back"`: ⌘[ from `via` to `left`, then the entry and `row`, which opens `bare` at 0. `keeps` is { sel, value } that must read `value` after the reopen.
//   nav-docs      Round 17. `then: "search"`: a search, Inbox, the entry, each pressed by pointer: it must stand with its *Clear*. A hit, the entry, Clear, Inbox, the entry goes to #/docs. `then: "folds"`: folds opened and closed on the home and on a space hold through Inbox and the entry
//                 (which comes back to the space it was left on, and pressed again goes to the home, 2026-11-13).
//   gear-return   Round 17. Settings' section scrolled, Inbox, the gear by pointer and ⌘,: address and scroll as left, Esc back to Inbox. On the section, the gear pressed twice and Enter twice move nothing. On a source's page
//                 the gear goes to Sources, and from another section back to the source's page (2026-11-13); the status bar's *Sources* goes to #/settings.
//   walk-place    Round 17. `scroller` at `at`, `via` and `section`, then `j`: scroll stays, focus on a row in view. With no row focused `k` likewise. From scroll 0, `j` then `j` then `k` walk from the first row.
//
//   field-in-view Round 17. Press `open`, then type a line and Enter `times` times with real keys. Fails if `field` lost focus or ends under the status bar after any Enter. `size` is a [w, h].
//   press-holds   Round 17. Press `press` twice by pointer with a still pointer, and on a fresh page Enter twice by key, then `via`'s nav entry and back. Fails if the control moves from under the pointer or the pressed point stops being that control,
//                 or if `line` didn't draw below it.
//
//   scroller-tab  Round 17. `scroller` at `at`, a real press in its words, then Tab. Fails if the scroller moves, or if focus lands on nothing.
//   pane-esc      Round 17. A real press in the pane's words, then Esc. Fails unless the address moved, focus left the pane, and the next ↓ walks.
//
//   right-click   2026-11-16 (Björn: "did, somewhere, we check if rightclick-options make sense?"). A real right press on `on` (or, with
//                 `key`, Shift+F10 on it focused). Fails if the web view's menu wasn't stopped (or was, with `platform`: a field keeps it),
//                 if knobas's menu isn't up by the next frame (or is, with `none`), if its rows aren't `items` (their ids), if opening
//                 it moved the address or sent a write, if a second right press there doesn't draw it again in place, or if Esc
//                 doesn't close it with focus back where it was. `select` selects `on`'s words first. `pick` (and `sub`, a row of
//                 *Open in… ▸*) is chosen by pointer, and then `toast` must start a toast, `ack` an opening's chip (ux21), `hash`
//                 start the address, or `launcher` be the launcher's box. The clipboard is the page's own stub: nothing reaches the machine's.
//
//   right-spam    2026-11-21 (Björn: "When I spam right click … the context menu stays open indefinitely"). Three rounds of real right
//                 presses on `on`'s rows, 10 to 50 ms apart, the pointer drifting a few px and now and then resting on a row of the
//                 open menu. Then the pointer rests on the menu's `rest` row (*Open in… ▸* opens its submenu), and one `close`:
//                 `esc`, `away` (a left press off the menus), `blur` (the window losing focus, as a switch to another program does)
//                 or `hash` (another address). Fails if more than a menu and its submenu stood at once, or if anything of the menu
//                 (a node in the float host, a row still marked) is left after the close. Before ux27 the submenu outlived its
//                 menu on a blur or a press that came up elsewhere, with nothing left to close it.
//
//   app-page      The application page (PR #2479, 2026-11-13). From `route` (a repository), *Open in… › Add an application*, the page
//                 must be at its address; name and command typed, the second repository picked, *It will run* must name that repository's
//                 checkout; *Try it* must start it there, *Add application* must save and go back to `route`, whose *Open in…* then lists it.
//
//   list-return   2026-11-17 (Björn: "After adding a new application for open in it opens another page instead of going back to the checkouts and open…").
//                 From `route`, open `list`, then each of `steps` by real pointer: `open` (a `sel` on the row holding `row`'s words), `fill`, `press`. Fails unless
//                 the address is `list` again with focus on the row holding `lands`, in view and not a control a second press would press; and unless Back
//                 from the list goes to `route`, not to a page of it.
//
//   tree-walk     2026-11-19 (Björn: "not switch to the space when I'm just clicking through the tree"). At `route` (and `size`), each of `steps`
//                 by real pointer: { fold } opens the fold whose head starts with those words, { scroll } scrolls the rail's body there, { press } presses
//                 a row of the rail (`starts` its words), which must change the address. Around each press: the Space picker's value, the rail's
//                 scroll and every row of the rail (fold heads, links, the filter) with where it stands. Fails if a press changes any of them by
//                 more than 1 px: a press in the tree opens the page and moves nothing in the rail.
//
//   card-drag     2026-11-21 (Björn: "drag/drop ... it also shows, after picking up the ticket, where you only can drag the ticket", ADR-0054).
//                 At `route`, by real mouse: a card of `from` rests, is pressed and glides to `to`, a column it can reach. While held the wrap says
//                 `data-held` and each column `data-drop` (`to` "yes", `no` "no"); the release lands the card in `to` with its write chip and one
//                 move queued. A second card glides to `no`, a column it cannot reach: the release leaves it in `from` and queues nothing. Fails if
//                 a column keeps `data-drop` or the float host keeps a copy, a line or a reason at rest, or if a card drawn elsewhere moved.
//
//   tree-click    2026-11-21 (Björn: "when I click on a tree element, I want it to always expand … click on it again, it should toggle"). At `route`,
//                 by real pointer on `fold`'s title: the first click must open a page and the fold, the next two on the open page close and open it,
//                 two on its chevron close and open it with no page. Fails if a press moves a row at or above the pressed one, if the fold is ever drawn
//                 open with none of its pages for a frame, or if Enter on `keyFold`'s row opens its fold (the keys stay as they were).
//                 Settings' nav is a tree of the same rule (2026-11-21): `rail`, `folds`, `rowsOf`, `under` and `chevronX` name its parts,
//                 Docs' by default.
//
//   frame-scroll  2026-11-17 (Björn: "When I have an application render an html for code folders I can't scroll it even though it's longer than the page").
//                 A renderer's long page in its sandboxed `frame` (a folder renderer's under `fake-folder-render=page <words>`, the *Cat* file renderer's
//                 under `fake-render-page=<words>`). Fails if the frame's sandbox isn't "", if `header` or the frame's bottom is out of the pane at rest
//                 (`fits`), unless a wheel over it scrolls the page (and not the pane), sideways too with `wide`, unless after a press in it PageDown,
//                 ↓ and Space each scroll it, and unless ⌘ or Ctrl with the wheel over it zooms the window and scrolls nothing, held from the app and
//                 again after a press on `header` takes focus back from the page (with focus in the page the app hears no key). Puts the zoom back at 100 %.
//
//   cold-start    2026-11-20 (Björn: "the knobas app should open in the all-tickets view. It looks kind of broken when it starts in the
//                 'no job has been selected'"). Opens the app cold on no address, with `chosen` in the browser's storage (an entity id, or
//                 nothing), and records every frame from the first one the shell draws: its address and the shown page's heading. Fails if
//                 any frame draws `never` (the words of a page that must not paint), if the first frame isn't at `first` with a heading
//                 starting `heading` (when named), or if it doesn't end at `lands`.
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
//                 `touch`: under Chrome's touch emulation, so `hover: none` matches and a row's actions stand in the row, as on a touch
//                 screen or in headless Chrome on Linux (headless Chrome on macOS reports `hover: hover`).
//
//   press-result  2026-11-21 (the themes follow-up: Settings › Appearance › Code colours). `store`: localStorage keys set and the page
//                 loaded again, so it opens as a person who kept them would. `before` (a function, as text) is read in the page, then
//                 each of `steps` by real input: { press } by real pointer at 15% of the control, with `file` the file chooser that press
//                 opens answered with that file of `testenv/ui-checks/`; { key } a key; { type } words typed into what holds focus;
//                 { focus } a selector focused without a press; { chars } letters pressed one key at a time, as a person types into a
//                 focused <select> to choose by its first letters (an arrow key or a press opens macOS's native menu, which headless
//                 Chrome can't drive); { selectAll } the platform's select-all, ⌘A on macOS and Ctrl+A elsewhere, sent with Chrome's
//                 selectAll command, since a key event alone edits nothing on macOS. Then
//                 `check`, given what `before` read, must answer null within `within` ms (4000): until then it gives the reason it
//                 fails. Also fails if the press opened a dialog, or moved the address unless the trip `leaves`.
//
// A first Enter that changes nothing can't run: the trip would pass without having pressed anything.
// Writes are counted by wrapping window.__TAURI_INTERNALS__.invoke, which @tauri-apps/api looks up on every call (shell/dev/fake-tauri.ts).
import { appendFileSync, mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { launch, screenOpener, sleep } from "./chrome.mjs";
import { runQueue } from "./trip-runner.mjs";

const REPO = "repo/mock%3Atidewater%2Fpayout-service";
/** Picks *Working tree* in the repository header's branch box, as a pick in its list does (2026-11-21). */
const CHOOSE_WORKING_TREE = `(() => { const box = document.querySelector(".tf-bsel select"); box.value = "~working-tree"; box.dispatchEvent(new Event("change", { bubbles: true })); })()`;
/** In the page: why the menu `sel` doesn't fit (out of the window, or a row of it wider than it), or null. */
const FITS = (sel) => String.raw`(() => {
  const menu = [...document.querySelectorAll(${JSON.stringify(sel)})].find((e) => e.getBoundingClientRect().width > 0);
  if (!menu) return "no " + ${JSON.stringify(sel)} + " is open";
  const b = menu.getBoundingClientRect();
  if (b.left < -0.5 || b.top < -0.5 || b.right > innerWidth + 0.5 || b.bottom > innerHeight + 0.5) return "the menu runs out of the window: " + [b.left, b.top, b.right, b.bottom].map(Math.round).join(", ") + " in " + innerWidth + " x " + innerHeight;
  if (menu.scrollWidth > menu.clientWidth + 1) return "the menu scrolls sideways by " + (menu.scrollWidth - menu.clientWidth) + " px";
  for (const row of menu.querySelectorAll("*")) {
    const r = row.getBoundingClientRect();
    if (r.width > 0 && (r.right > b.right + 1 || r.left < b.left - 1)) return "'" + row.textContent.trim().slice(0, 60) + "' runs " + Math.round(Math.max(r.right - b.right, b.left - r.left)) + " px out of the menu";
  }
  return null;
})()`;
// The roster's monitors not hidden: a hidden one is drawn after them (the monitors-page pass), so a Hide leaves this count.
const MONITOR_ROWS = ".tf-mon-row:not(.tf-mon-hid)";
const RUN = "build/teamcity%3AbuildType%3APayout_IntegrationTests/teamcity%3Abuild%3APayout_IntegrationTests-1187";

// The application page's trips start with its name and command typed.
/** A folder renderer typed on its page (ADR-0044). */
const FOLDER_TYPED = [{ wait: 600 }, { fill: "#tf-frp-name", text: "Tree" }, { fill: "#tf-frp-cmd", text: "tree {path}" }, { fill: "#tf-frp-folder", text: "src" }, { wait: 300 }];
const APP_TYPED = [{ wait: 600 }, { fill: "#tf-app-name", text: "MyGitViewer" }, { fill: "#tf-app-command", text: "MyGitViewer {path}" }, { wait: 300 }];

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

// Settings › Appearance › Code colours (the themes follow-up, 2026-11-21): what each press there must leave.
const ENGINE_NAMES = { pierre: "Pierre Highlights", tanstack: "TanStack Highlight", shiki: "Shiki" };
/** The preview's words, its lines' boxes and its colours, and from now on every job the port sends the worker, by engine. */
const PREVIEW_BEFORE = String.raw`() => {
  window.__opens = [];
  if (!Worker.prototype.__tripOpens) {
    const post = Worker.prototype.postMessage;
    Worker.prototype.postMessage = function (m, ...rest) { if (m && m.type === "open") window.__opens.push(m.engine); return post.call(this, m, ...rest); };
    Worker.prototype.__tripOpens = true;
  }
  const pre = document.querySelector(".tf-kept:not([hidden]) .tf-thm-pre");
  return { text: pre.textContent, boxes: [...pre.querySelectorAll(".tf-l, .tf-ln")].map((l) => { const r = l.getBoundingClientRect(); return [r.x, r.y, r.width, r.height].map(Math.round).join(); }).join(";"), sig: [...pre.querySelectorAll("[class^=tf-hl-]")].map((s) => s.className + ":" + s.textContent).join("|") };
}`;
/** A choice of highlighter recolours the preview in place: radio, kept value, every job asked again of it, words and lines still, colours new. */
const ENGINE_CHECK = (engine) => String.raw`(b) => {
  const pre = document.querySelector(".tf-kept:not([hidden]) .tf-thm-pre");
  if (!document.querySelector('input[name=tf-thm-engine][value="${engine}"]')?.checked) return "${engine}'s radio is not checked";
  const kept = localStorage.getItem("knobas.highlighter");
  if (kept !== ${engine === "pierre" ? "null" : `"${engine}"`}) return "knobas.highlighter holds " + kept;
  const asked = window.__opens.filter((e) => e === "${engine}").length;
  if (asked < 5) return "the preview's five jobs were asked again of ${engine} " + asked + " times (" + (window.__opens.join(", ") || "none at all") + ")";
  if (pre.textContent !== b.text) return "the preview's words changed";
  const boxes = [...pre.querySelectorAll(".tf-l, .tf-ln")].map((l) => { const r = l.getBoundingClientRect(); return [r.x, r.y, r.width, r.height].map(Math.round).join(); }).join(";");
  if (boxes !== b.boxes) return "a line of the preview moved or changed size: " + b.boxes + " then " + boxes;
  const sig = [...pre.querySelectorAll("[class^=tf-hl-]")].map((s) => s.className + ":" + s.textContent).join("|");
  if (sig === b.sig) return "the preview still has the colours of before";
  return null;
}`;
/** A person's own themes as kept, and the light slot. */
const THEMES_BEFORE = String.raw`() => ({ own: JSON.parse(localStorage.getItem("knobas.themes") ?? "[]").length, light: localStorage.getItem("knobas.theme.light") })`;
const DUPLICATE_CHECK = String.raw`(b) => {
  const own = JSON.parse(localStorage.getItem("knobas.themes") ?? "[]");
  if (own.length !== b.own + 1) return own.length + " themes of one's own are kept, " + b.own + " before";
  const copy = own.at(-1);
  if (copy.name !== "knobas Light copy" || copy.kind !== "light") return "the copy is " + copy.name + ", " + copy.kind;
  // While code follows the window's theme, the copy goes to the window's slot and code goes on following it (ADR-0055).
  if (localStorage.getItem("knobas.app-theme.light") !== copy.id) return "the window's light slot holds " + localStorage.getItem("knobas.app-theme.light") + ", not the copy";
  if (localStorage.getItem("knobas.theme.light") !== null) return "the code slot was set apart to " + localStorage.getItem("knobas.theme.light");
  const page = document.querySelector(".tf-kept:not([hidden])");
  const h = page.querySelector(".tf-thm-edh h2")?.textContent.trim();
  if (h !== "knobas Light copy") return "the editor shows " + h;
  if (page.querySelector(".tf-thm-slot[data-slot=light] select").value !== "") return "the light slot's select no longer follows the app theme";
  const swatches = page.querySelectorAll(".tf-thm-roles button.tf-thm-sw").length;
  if (swatches !== 21) return swatches + " swatches are buttons, not the 21 of a theme one can change";
  return null;
}`;
const IMPORT_CHECK = (names, toast, syntax) => String.raw`(b) => {
  const own = JSON.parse(localStorage.getItem("knobas.themes") ?? "[]");
  const want = ${JSON.stringify(names)};
  const got = own.slice(b.own).map((t) => t.name);
  if (got.join("|") !== want.join("|")) return "kept after the import: " + (got.join(", ") || "nothing");
  const first = own[b.own];
  for (const [role, colour] of Object.entries(${JSON.stringify(syntax)})) if (first.syntax[role] !== colour) return first.name + "'s " + role + " is " + first.syntax[role] + ", not " + colour;
  const toasts = [...document.querySelectorAll(".tf-toast")].map((t) => t.textContent.replace(/\s+/g, " ").trim());
  if (!toasts.some((t) => t.startsWith(${JSON.stringify(toast)}))) return "no toast says ${toast} (" + (toasts.join("; ") || "no toast") + ")";
  const h = document.querySelector(".tf-kept:not([hidden]) .tf-thm-edh h2")?.textContent.trim();
  if (h !== want[0]) return "the editor shows " + h + ", not " + want[0];
  return null;
}`;
const SLOT_BEFORE = String.raw`() => ({ kw: getComputedStyle(document.documentElement).getPropertyValue("--hl-kw").trim() })`;
const SLOT_CHECK = String.raw`(b) => {
  const page = document.querySelector(".tf-kept:not([hidden])");
  const select = page.querySelector(".tf-thm-slot[data-slot=light] select");
  if (select.value !== "github-light") return "the light slot's select shows " + select.value;
  if (localStorage.getItem("knobas.theme.light") !== "github-light") return "the light slot keeps " + localStorage.getItem("knobas.theme.light");
  const kw = getComputedStyle(document.documentElement).getPropertyValue("--hl-kw").trim().toUpperCase();
  if (kw !== "#C3243A") return "the window's keyword colour is " + kw + ", " + b.kw + " before";
  const h = page.querySelector(".tf-thm-edh h2")?.textContent.trim();
  if (h !== "GitHub Light") return "the editor shows " + h;
  const chip = page.querySelector(".tf-thm-slot[data-slot=light] .tf-thm-chips i[title=Keyword]");
  if (chip && getComputedStyle(chip).backgroundColor !== "rgb(195, 36, 58)") return "the slot's keyword chip is " + getComputedStyle(chip).backgroundColor;
  return null;
}`;
// Settings › Appearance › Theme (ADR-0055).
/**
 * Code colours with a theme in the editor that has window colours the window does not draw (#2516): the line saying so
 * spans the editor's grid above the role list and the preview, which stay side by side; *Edit* focused the editor.
 */
const BANNER_CHECK = String.raw`() => {
  const page = document.querySelector(".tf-kept:not([hidden])");
  const ed = page.querySelector(".tf-thm-ed");
  const line = ed?.querySelector(":scope > .tf-thm-app");
  const roles = ed?.querySelector(":scope > .tf-thm-roles");
  const side = ed?.querySelector(":scope > .tf-thm-side");
  if (!line || !roles || !side) return "the editor has no window-colours line, role list or preview";
  const [l, r, s] = [line, roles, side].map((el) => el.getBoundingClientRect());
  if (Math.abs(l.left - r.left) > 1 || Math.abs(l.right - s.right) > 1) return "the line runs " + l.left + " to " + l.right + ", the list starts at " + r.left + " and the preview ends at " + s.right;
  if (l.bottom > r.top + 1 || l.bottom > s.top + 1) return "the line's foot at " + l.bottom + " is under the list's top " + r.top + " or the preview's " + s.top;
  if (Math.abs(r.top - s.top) > 1 || s.left <= r.right) return "the list (" + r.left + ", " + r.top + ") and the preview (" + s.left + ", " + s.top + ") are not side by side";
  const h = ed.querySelector(".tf-thm-edh h2");
  if (document.activeElement !== h) return "Edit left focus on " + (document.activeElement?.outerHTML.slice(0, 80) ?? "nothing");
  return null;
}`;
const APP_BEFORE = String.raw`() => ({ own: JSON.parse(localStorage.getItem("knobas.themes") ?? "[]").length, sheet: getComputedStyle(document.documentElement).getPropertyValue("--sheet").trim() })`;
const appPage = String.raw`document.querySelector(".tf-kept:not([hidden])")`;
const APP_SLOT_CHECK = String.raw`(b) => {
  const page = ${appPage};
  if (page.querySelector(".tf-thm-slot[data-slot=light] select").value !== "github-light") return "the light slot's select shows " + page.querySelector(".tf-thm-slot[data-slot=light] select").value;
  if (localStorage.getItem("knobas.app-theme.light") !== "github-light") return "the window's light slot keeps " + localStorage.getItem("knobas.app-theme.light");
  const sheet = document.documentElement.style.getPropertyValue("--sheet-light").trim();
  if (sheet !== "#FFFFFF") return "--sheet-light on <html> is " + (sheet || "unset");
  const h = page.querySelector(".tf-thm-edh h2")?.textContent.trim();
  if (h !== "GitHub Light") return "the editor shows " + h;
  return null;
}`;
const APP_DUPLICATE_CHECK = String.raw`(b) => {
  const own = JSON.parse(localStorage.getItem("knobas.themes") ?? "[]");
  if (own.length !== b.own + 1) return own.length + " themes of one's own are kept, " + b.own + " before";
  const copy = own.at(-1);
  if (localStorage.getItem("knobas.app-theme.light") !== copy.id) return "the window's light slot holds " + localStorage.getItem("knobas.app-theme.light") + ", not the copy";
  const page = ${appPage};
  if (page.querySelector(".tf-thm-edh h2")?.textContent.trim() !== "knobas Light copy") return "the editor shows " + page.querySelector(".tf-thm-edh h2")?.textContent.trim();
  const swatches = page.querySelectorAll(".tf-apt-roles button.tf-thm-sw").length;
  if (swatches !== 32) return swatches + " swatches are buttons, not the 32 of a theme one can change";
  return null;
}`;
/** A light theme of one's own in the window's light slot: knobas Light with a white panel. */
const APP_OWN_STORE = {
  "knobas.themes": JSON.stringify([{ id: "own-tripappx", name: "Trip window", kind: "light", from: "knobas Light", app: { sheet: "#FFFFFF" }, syntax: {} }]),
  "knobas.app-theme.light": "own-tripappx",
};
/** The same with a Passed too light to read on it. */
const APP_LOW_STORE = {
  "knobas.themes": JSON.stringify([{ id: "own-tripappx", name: "Trip window", kind: "light", from: "knobas Light", app: { sheet: "#FFFFFF", ok: "#7BC67E" }, syntax: {} }]),
  "knobas.app-theme.light": "own-tripappx",
};
const keptApp = String.raw`(JSON.parse(localStorage.getItem("knobas.themes") ?? "[]").find((t) => t.id === "own-tripappx")?.app ?? {})`;
const APP_SWATCH_CHECK = String.raw`(b) => {
  const row = ${appPage}.querySelector(".tf-thm-role[data-row=ok]");
  if (${keptApp}.ok !== "#7BC67E") return "the theme keeps Passed " + ${keptApp}.ok;
  if (!row.classList.contains("tf-low") || !row.querySelector(".tf-thm-why button")) return "the Passed row draws no warning with its fix";
  const chip = ${appPage}.querySelector(".tf-apt-stage .tf-chip.tf-ok");
  if (getComputedStyle(chip).color !== "rgb(123, 198, 126)") return "the preview's Passed chip draws " + getComputedStyle(chip).color;
  return null;
}`;
const APP_FIX_CHECK = String.raw`(b) => {
  const row = ${appPage}.querySelector(".tf-thm-role[data-row=ok]");
  if (row.classList.contains("tf-low")) return "the Passed row is still under its mark: " + row.querySelector(".tf-thm-cr").textContent;
  if (${keptApp}.ok === "#7BC67E") return "the theme keeps the light green";
  return null;
}`;
const APP_APART_CHECK = String.raw`(b) => {
  const app = ${keptApp};
  if (!app.ok || !app.warn || !app.err) return "the three states are not set: " + JSON.stringify(app);
  const line = ${appPage}.querySelector(".tf-apt-told span").textContent;
  if (!line.startsWith("Told apart")) return "the line says " + line;
  return null;
}`;
const APP_BAND_CHECK = (how) => String.raw`(b) => {
  const own = JSON.parse(localStorage.getItem("knobas.themes") ?? "[]");
  const night = own.find((t) => t.name === "Trip Harbour Night");
  if (!night) return "Trip Harbour Night was not kept";
  const app = localStorage.getItem("knobas.app-theme.dark"), code = localStorage.getItem("knobas.theme.dark");
  if (${JSON.stringify(how)} === "follows" && (app !== night.id || code !== null)) return "window " + app + ", code " + code;
  if (${JSON.stringify(how)} === "apart" && (app !== night.id || code !== "knobas-dark")) return "window " + app + ", code " + code;
  if (${JSON.stringify(how)} === "code" && (app !== null || code !== night.id)) return "window " + app + ", code " + code;
  if (${appPage}.querySelector(".tf-apt-use")) return "the band is still there";
  return null;
}`;
const APP_PICK_CHECK = String.raw`(b) => {
  const page = ${appPage};
  if (page.querySelector(".tf-apt-stage.tf-apt-picking")) return "the pick mode is still on";
  if (!page.querySelector(".tf-thm-pop[aria-label=\"Attention's tint\"]")) return "Attention's tint's panel is not open";
  return null;
}`;
/** Dracula while dark, GitHub Light while light, One Dark's code while dark. */
const APP_KEPT_STORE = { "knobas.app-theme.dark": "dracula", "knobas.app-theme.light": "github-light", "knobas.theme.dark": "one-dark" };
const APP_RESET_CHECK = (undone) => String.raw`(b) => {
  const kept = ["knobas.app-theme.dark", "knobas.app-theme.light", "knobas.theme.dark"].map((k) => localStorage.getItem(k));
  const want = ${undone} ? ["dracula", "github-light", "one-dark"] : [null, null, null];
  if (kept.join() !== want.join()) return "kept: " + kept.join(", ");
  if (!${undone} && ![...document.querySelectorAll(".tf-toast")].some((t) => t.textContent.includes("knobas's own themes are back"))) return "no toast says knobas's own themes are back";
  return null;
}`;
/** A light theme of one's own in the light slot, its keyword the knobas purple. */
const OWN_THEME_STORE = {
  "knobas.themes": JSON.stringify([{ id: "own-tripswch", name: "Trip swatch", kind: "light", from: "knobas Light", syntax: { com: "#5F6975", kw: "#7A3EB1", str: "#1F7444", fn: "#2F55D4" } }]),
  "knobas.theme.light": "own-tripswch",
};
const SWATCH_BEFORE = String.raw`() => ({ ratio: document.querySelector(".tf-kept:not([hidden]) .tf-thm-role[data-role=kw] .tf-thm-cr").textContent.trim() })`;
/** #C8B6E2 on the light sheet is about 1.9:1: the row says so and offers the fix, the preview draws it, the theme keeps it. */
const SWATCH_CHECK = String.raw`(b) => {
  const page = document.querySelector(".tf-kept:not([hidden])");
  const row = page.querySelector(".tf-thm-role[data-role=kw]");
  const hex = row.querySelector(".tf-thm-hex").textContent.trim();
  if (hex !== "#C8B6E2") return "the keyword row reads " + hex;
  const ratio = row.querySelector(".tf-thm-cr").textContent.trim();
  if (ratio === b.ratio) return "the keyword's contrast still reads " + ratio;
  if (!(parseFloat(ratio) < 4.5)) return "the keyword's contrast reads " + ratio + ", not under 4.5:1";
  if (!row.classList.contains("tf-low") || !row.querySelector(".tf-thm-why button")) return "the row draws no warning with its fix at " + ratio;
  const kept = JSON.parse(localStorage.getItem("knobas.themes") ?? "[]").find((t) => t.id === "own-tripswch");
  if (kept?.syntax.kw !== "#C8B6E2") return "the theme keeps " + kept?.syntax.kw;
  const word = page.querySelector(".tf-thm-pre .tf-hl-kw");
  if (word && getComputedStyle(word).color !== "rgb(200, 182, 226)") return "the preview's keyword draws " + getComputedStyle(word).color;
  return null;
}`;

// Moving a ticket on the board (ADR-0054): a drag to a column it can reach, and one to a column it cannot.
const MOVES = [
  { page: "Moving a ticket", route: "tickets", trips: [
    { id: "card-drag", kind: "card-drag", strict: true, finding: "2026-11-21-ticket-move", size: [1440, 868], from: "To Do", to: "In Progress", no: "Done" },
  ] },
];

// By page. `press` finds the first visible control with those words; `prep` runs first: { click, text | starts }, { fill, text }, { eval } or { wait }.
// The right-click menu's cases (2026-11-16): one per kind of thing a person right-clicks daily, a field, and nothing.
const RIGHT_CLICK = [
  { page: "Right-click, Tickets", route: "tickets", trips: [
    { id: "rc-ticket-card", kind: "right-click", finding: "right-click", on: { sel: ".tf-tcard" }, items: ["open", "work", "timer", "move", "assign", "priority", "link", "web", "copy-key", "copy-key-title", "copy-link"], pick: "copy-key", toast: "Copied " },
    { id: "rc-ticket-key", kind: "right-click", finding: "right-click", key: true, on: { sel: ".tf-tcard" }, items: ["open", "work", "timer", "move", "assign", "priority", "link", "web", "copy-key", "copy-key-title", "copy-link"] },
    { id: "rc-field", kind: "right-click", finding: "right-click", platform: true, on: { sel: ".tf-main input" } },
  ] },
  { page: "Right-click, Code", route: "code", trips: [
    { id: "rc-repo-row", kind: "right-click", finding: "right-click", on: { sel: "tbody tr.tf-row td:last-child" }, items: ["open", "prs", "branches", "commits", "link", "open-default", "open-in", "web", "run-first", "run-in", "copy-name", "copy-full", "copy-link"], pick: "prs", hash: "#/repo/" },
  ] },
  // The Run group stands after *Open in* whatever the person's commands, *Run a command…* with none (ADR-0060, section 8).
  { page: "Right-click, a repository's files", route: `${REPO}/files`, trips: [
    { id: "rc-folder", kind: "right-click", finding: "right-click", on: { sel: ".tf-ftrows .tf-fdir" }, items: ["open-default", "open-in", "forge", "run-first", "run-in", "copy-path", "copy-full", "copy-link"], pick: "open-in", sub: "app-terminal", ack: "Opening " },
    { id: "rc-file", kind: "right-click", finding: "right-click", on: { sel: ".tf-ftrows a" }, items: ["open", "history", "open-default", "open-in", "forge", "run-first", "run-in", "copy-path", "copy-full", "copy-link"], pick: "copy-path", toast: "Copied " },
  ] },
  { page: "Right-click, a pull request's commits", route: "pr/mock%3Apayout-service%23142/commits", trips: [
    { id: "rc-commit", kind: "right-click", finding: "right-click", on: { sel: "a[href*='/commits/mock']" }, starts: ["open", "open-default", "open-in", "web", "copy-sha", "copy-short", "copy-link"], pick: "copy-short", toast: "Copied c90d11e" },
  ] },
  { page: "Right-click, Inbox", route: "inbox", trips: [
    { id: "rc-inbox-line", kind: "right-click", finding: "right-click", on: { sel: ".tf-ib a" }, starts: ["open"] },
    { id: "rc-words", kind: "right-click", finding: "right-click", select: true, on: { sel: ".tf-ib time" }, starts: ["copy-sel", "search-sel", "open"], pick: "search-sel", launcher: true },
    { id: "rc-nothing", kind: "right-click", finding: "right-click", none: true, on: { sel: ".tf-main h1" } },
  ] },
  { page: "Right-click, Monitors", route: "monitors", trips: [
    { id: "rc-monitor", kind: "right-click", finding: "right-click", on: { sel: ".tf-mon-row .tf-mon-state" }, starts: ["open"] },
  ] },
  { page: "Right-click, a run", route: RUN, trips: [
    { id: "rc-run-heading", kind: "right-click", finding: "right-click", on: { sel: ".tf-main h1" }, starts: ["log", "tests", "rerun"] },
  ] },
];

// The cold start (2026-11-20): with no chosen job the window opens on All tickets from its first frame, never on a page that only
// points at it; with one, on home drawing it; with one knobas no longer holds, on All tickets once it has looked.
const NEVER = ["No job is chosen yet", "knobas does not hold this ticket"];
const COLD = [
  { page: "Cold start", route: "", trips: [
    { id: "cold-no-job", kind: "cold-start", finding: "start-all-tickets", chosen: null, first: "#/tickets", heading: "All tickets", lands: "#/tickets", never: NEVER },
    { id: "cold-chosen-job", kind: "cold-start", finding: "start-all-tickets", chosen: "mock:PAY-231", first: "#/job", lands: "#/job", never: NEVER, drawn: ".tf-jobcard .tf-jobkey" },
    { id: "cold-gone-job", kind: "cold-start", finding: "start-all-tickets", chosen: "mock:PAY-99999", lands: "#/tickets", heading: "All tickets", never: NEVER },
  ] },
];

// Right-click spam (ux27, 2026-11-21): each page with a right-click menu, each way it closes.
const SPAM = (id, on, close, rest) => ({ id, kind: "right-spam", finding: "right-spam", on: { sel: on }, close, rest });
const RIGHT_SPAM = [
  { page: "Right-click spam, Code", route: "code", trips: [
    SPAM("rs-repo-blur", "tbody tr.tf-row td:last-child", "blur", "open-in"),
    SPAM("rs-repo-esc", "tbody tr.tf-row td:last-child", "esc", "prs"),
    SPAM("rs-repo-away", "tbody tr.tf-row td:last-child", "away", "open-in"),
  ] },
  { page: "Right-click spam, a repository's files", route: `${REPO}/files`, trips: [
    SPAM("rs-folder-blur", ".tf-ftrows .tf-fdir", "blur", "open-in"),
    SPAM("rs-file-hash", ".tf-ftrows a", "hash", "open-in"),
    SPAM("rs-file-esc", ".tf-ftrows a", "esc", "history"),
  ] },
  { page: "Right-click spam, a pull request's commits", route: "pr/mock%3Apayout-service%23142/commits", trips: [
    SPAM("rs-commit-blur", "a[href*='/commits/mock']", "blur", "open-in"),
  ] },
  { page: "Right-click spam, Tickets", route: "tickets", trips: [SPAM("rs-ticket-away", ".tf-tcard", "away", "web")] },
  { page: "Right-click spam, Inbox", route: "inbox", trips: [SPAM("rs-inbox-esc", ".tf-ib a", "esc", null)] },
  { page: "Right-click spam, Monitors", route: "monitors", trips: [SPAM("rs-monitor-blur", ".tf-mon-row .tf-mon-state", "blur", null)] },
  { page: "Right-click spam, a run", route: RUN, trips: [SPAM("rs-run-away", ".tf-main h1", "away", "tests")] },
  { page: "Right-click spam, Docs", route: "docs/space/ENG", trips: [SPAM("rs-docs-hash", ".tf-dl-leaf, .tf-sl", "hash", null)] },
];

const PAGES = [
  ...COLD,
  ...RIGHT_CLICK,
  ...MOVES,
  ...RIGHT_SPAM,
  // The still-rows pass (PR #2479, 2026-11-11): each late read lands after the first draw, and nothing drawn may move.
  { page: "Code, Repositories", route: "code", trips: [
    // *Show N new* itself: a sync brings a repository above the drawn rows, then the press, twice by key and twice by a still pointer.
    { id: "show-new-key", kind: "enter-twice", strict: true, finding: "still-rows", query: STILL_Q, prep: [{ wait: 800 }, { eval: GROW_FORGE }, { eval: "window.__fake.endRuns(), 1" }, { wait: 2500 }], press: { sel: ".tf-fheld", starts: "Show" }, rows: "tbody tr.tf-row" },
    { id: "show-new-pointer", kind: "pointer-twice", strict: true, finding: "still-rows", query: STILL_Q, prep: [{ wait: 800 }, { eval: GROW_FORGE }, { eval: "window.__fake.endRuns(), 1" }, { wait: 2500 }], press: { sel: ".tf-fheld", starts: "Show" }, rows: "tbody tr.tf-row" },
    { id: "code-rows-still", kind: "rows-still", strict: true, finding: "still-rows", via: "inbox", words: "code", rows: "tbody tr.tf-row", held: true, query: STILL_Q, grow: GROW_FORGE },
    // Where nothing hovers (a touch screen, headless Chrome on Linux), a row's actions stand in the row: *Clone*, which the
    // checkout read brings, grew a row from 34 to 41 px once it landed (the ux28 gate, 2026-11-21). Touch emulation gives
    // `hover: none` on every machine, and the checkouts come 2 s late, after the rows have drawn.
    { id: "code-rows-still-touch", kind: "rows-still", strict: true, finding: "still-rows", via: "inbox", ways: ["first"], touch: true, rows: "tbody tr.tf-row", query: STILL_Q.replace("entity_checkouts:900", "entity_checkouts:2000"), tail: 2600 },
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
  // Builds' *Recent* (2026-11-25): every pipeline's runs newest first; a run a sync brings waits behind the pill while the
  // pointer rests on the runs, a first draw, a return and a sync away draw it on top; Enter on a run's number opens it once.
  { page: "Builds, Recent", route: "builds/recent", trips: [
    { id: "builds-recent-rows-still", kind: "rows-still", strict: true, finding: "2026-11-25-recent-runs", via: "inbox", words: "recent runs", rows: ".tf-main tr.tf-row", held: true, rest: ".tf-runs tr.tf-row", pill: ".tf-newruns button", query: STILL_Q, grow: "window.__fake.runNew(), 1" },
    { id: "builds-recent-open-key", kind: "enter-twice", strict: true, finding: "2026-11-25-recent-runs", prep: [{ wait: 800 }], press: { sel: ".tf-recent a.tf-run-pno" }, leaves: true },
  ] },
  // The seeded roster, not the scale's bulk one: `monitorDown` takes a seeded monitor down.
  { page: "Monitors", route: "monitors", trips: [
    { id: "monitors-rows-still", kind: "rows-still", strict: true, finding: "still-rows", via: "inbox", words: "monitors", rows: ".tf-mon-row", query: STILL_Q.replace("&fake-scale", ""), grow: "window.__fake.monitorDown(), 1", tail: 3000 },
  ] },
  { page: "Docs", route: "docs", trips: [
    { id: "docs-rows-still", kind: "rows-still", strict: true, finding: "still-rows", via: "inbox", words: "docs", rows: "a.tf-dh-row, [role=treeitem]", query: STILL_Q },
  ] },
  // A press in the tree moves nothing in the rail (2026-11-19): from the list of every space, and from a page's own space.
  { page: "Docs, through the tree", route: "docs", trips: [
    { id: "docs-tree-walk-home", kind: "tree-walk", strict: true, finding: "2026-11-19", size: [1440, 620], steps: [
      { fold: "Engineering" }, { fold: "Payments" }, { scroll: 60 },
      { press: "PSP error codes" }, { press: "Refund flows" },
      { fold: "Operations" }, { press: "On-call handbook" }, { press: "SEPA payout retry design" },
    ] },
  ] },
  { page: "Docs, through the tree", route: "page/confluence%3Asepa", trips: [
    { id: "docs-tree-walk-space", kind: "tree-walk", strict: true, finding: "2026-11-19", size: [1440, 620], steps: [
      { fold: "Customs" }, { scroll: 40 }, { press: "EDI message formats" }, { press: "Payout batch lifecycle" }, { press: "Customs" },
    ] },
  ] },
  // A click on a fold's row opens the page and the fold, a click on the open page's row toggles it, the chevron folds alone,
  // and Enter opens the page only (2026-11-21, Björn: "when I click on a tree element, I want it to always expand").
  { page: "Docs, a fold's row", route: "page/confluence%3Asepa", trips: [
    { id: "docs-tree-click", kind: "tree-click", strict: true, finding: "2026-11-21", size: [1440, 900], fold: "Customs", keyFold: "Tracking" },
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
  // The application page (PR #2479, 2026-11-13): every press it brought, twice by key, twice with the pointer still, and left during its toast.
  { page: "Settings, adding an application", route: "settings/checkouts/add/mock%3Atidewater%2Fpayout-service", trips: [
    { id: "app-try-key", kind: "enter-twice", strict: true, finding: "apps-page", prep: APP_TYPED, press: { sel: ".tf-apprun button", text: "Try it" } },
    { id: "app-try-pointer", kind: "pointer-twice", strict: true, finding: "apps-page", once: true, prep: APP_TYPED, press: { sel: ".tf-apprun button", text: "Try it" } },
    { id: "app-try-leave", kind: "leave-back", strict: true, finding: "apps-page", prep: APP_TYPED, press: { sel: ".tf-apprun button", text: "Try it" }, rows: ".tf-apprun", rowsStay: true, ack: true, via: "inbox" },
    { id: "app-add-key", kind: "enter-twice", strict: true, finding: "apps-page", prep: APP_TYPED, press: { sel: ".tf-apppage button", text: "Add application" }, leaves: true },
    { id: "app-add-pointer", kind: "pointer-twice", strict: true, finding: "apps-page", once: true, prep: APP_TYPED, press: { sel: ".tf-apppage button", text: "Add application" } },
    { id: "app-add-leave", kind: "leave-back", strict: true, finding: "apps-page", prep: APP_TYPED, press: { sel: ".tf-apppage button", text: "Add application" }, rows: ".tf-stg-apps tbody tr", rowsStay: true, via: "inbox", backTo: "settings/checkouts" },
    { id: "app-cancel-key", kind: "enter-twice", strict: true, finding: "apps-page", prep: APP_TYPED, press: { sel: ".tf-apppage button", text: "Cancel" }, leaves: true },
  ] },
  { page: "Settings, an application", route: "settings/checkouts/app/fork", trips: [
    { id: "app-save-key", kind: "enter-twice", strict: true, finding: "apps-page", prep: [{ wait: 600 }, { fill: "#tf-app-command", text: "open -a Fork --new {path}" }, { wait: 200 }], press: { sel: ".tf-apppage button", text: "Save" }, leaves: true },
    { id: "app-remove-key", kind: "enter-twice", strict: true, finding: "apps-page", prep: [{ wait: 600 }], press: { sel: ".tf-apppage button", text: "Remove" }, leaves: true },
    { id: "app-remove-pointer", kind: "pointer-twice", strict: true, finding: "apps-page", once: true, prep: [{ wait: 600 }], press: { sel: ".tf-apppage button", text: "Remove" } },
    { id: "app-remove-leave", kind: "leave-back", strict: true, finding: "apps-page", prep: [{ wait: 600 }], press: { sel: ".tf-apppage button", text: "Remove" }, rows: ".tf-stg-apps tbody tr", rowsStay: true, via: "inbox", backTo: "settings/checkouts" },
  ] },
  { page: "Settings, Applications' Edit and ↑", route: "settings/checkouts", trips: [
    { id: "app-edit-link", kind: "enter-twice", strict: true, finding: "apps-page", press: { sel: ".tf-stg-apps a", text: "Edit" }, leaves: true },
    { id: "app-move-up-key", kind: "enter-twice", strict: true, finding: "apps-page", press: { sel: ".tf-stg-apps button[aria-label='Move Fork up']" }, rows: ".tf-stg-apps tbody tr" },
    { id: "app-move-up-pointer", kind: "pointer-twice", strict: true, finding: "apps-page", press: { sel: ".tf-stg-apps button[aria-label='Move Fork up']" }, rows: ".tf-stg-apps tbody tr" },
  ] },
  // 2026-11-17 (Björn, ux17: "Clicking on edit for checkouts and open in on an application keeps spazzing around for the first few seconds
  // running through the repositories"). *Try it on* reads checkouts in batches, each landing late here: the page must draw its rows once,
  // and the answers only fill them in. From Settings' *Edit*, from a repository's *Open in… › Add an application*, and a folder renderer's page.
  { page: "An application's page", route: "settings/checkouts/app/vscode", trips: [
    { id: "app-page-rows-still", kind: "rows-still", strict: true, finding: "still-rows", via: "settings/checkouts", ways: ["first", "return"], rows: ".tf-apppage .tf-stg-row, .tf-apppage h2, .tf-apprun", query: STILL_Q, tail: 1200 },
  ] },
  { page: "Add an application, from a repository", route: "settings/checkouts/add/mock%3Atidewater%2Fpayout-service", trips: [
    { id: "app-add-rows-still", kind: "rows-still", strict: true, finding: "still-rows", via: REPO, ways: ["first"], rows: ".tf-apppage .tf-stg-row, .tf-apppage h2, .tf-apprun", query: STILL_Q, tail: 1200 },
  ] },
  { page: "A folder renderer's page", route: "settings/renderers/folder/folder-listing", trips: [
    { id: "folder-page-rows-still", kind: "rows-still", strict: true, finding: "still-rows", via: "settings/renderers", ways: ["first", "return"], rows: ".tf-apppage .tf-stg-row, .tf-apppage h2", query: `${STILL_Q}&fake-folder-render`, tail: 1200 },
  ] },
  // An application's page and a folder renderer's hand back to their list (2026-11-17): every way out, by real pointer, from Code.
  { page: "Settings, back to the list", route: "code", trips: [
    { id: "apps-list-return", kind: "list-return", strict: true, finding: "apps-page", list: "settings/checkouts", steps: [
      { name: "add", open: { sel: ".tf-stg-acts a", row: "Add an application" }, fill: [["#tf-app-name", "Trip App"], ["#tf-app-command", "trip-app {path}"]], press: "Add application", lands: "Trip App" },
      { name: "save", open: { sel: ".tf-stg-apps a", row: "Fork" }, fill: [["#tf-app-command", "open -a Fork --new {path}"]], press: "Save", lands: "Fork" },
      { name: "cancel", open: { sel: ".tf-stg-apps a", row: "Terminal" }, press: "Cancel", lands: "Terminal" },
      { name: "cancel an add", open: { sel: ".tf-stg-acts a", row: "Add an application" }, press: "Cancel", lands: "Open in" },
      { name: "remove", open: { sel: ".tf-stg-apps a", row: "Fork" }, press: "Remove", lands: "IntelliJ IDEA" },
    ] },
    { id: "folders-list-return", kind: "list-return", strict: true, finding: "apps-page", query: "&fake-folder-render", list: "settings/renderers", steps: [
      { name: "add", open: { sel: ".tf-stg-acts a", row: "Add a folder renderer" }, fill: [["#tf-frp-name", "Tree"], ["#tf-frp-cmd", "tree {path}"]], press: "Add folder renderer", lands: "Tree" },
      { name: "save", open: { sel: ".tf-rnd-folders a", row: "Folder listing" }, press: "Save", lands: "Folder listing" },
      { name: "remove", open: { sel: ".tf-rnd-folders a", row: "Tree" }, press: "Remove", lands: "Folder listing" },
    ] },
  ] },
  // *Render as* with a renderer whose name and program are long (the render-as pass, 2026-11-19; Björn: "the render as orchestra,
  // when you click on it, and the path of the program is long it gets out of bounds"): the menu stays in the window, and picking
  // the other long renderer by key (the first is the file's default) gives focus back to the face, which names it.
  { page: "A file's Render as", route: `${REPO}/files/src/sepa/retry.rs`, trips: [
    { id: "render-as-long-key", kind: "enter-twice", strict: true, finding: "render-as-path", query: "&fake-long-paths", prep: [{ wait: 600 }, { click: ".tf-rsw-face", starts: "Render as" }, { wait: 400 }], fits: ".tf-rsw-m", press: { sel: "[role=menuitemradio]", starts: "Orchestra on the Mac" }, returns: "Render as Orchestra on the Mac" },
  ] },
  // A pipeline 64th of a chain of 70 long names (the build-header pass, 2026-11-20: Björn's "builds <huge repo link> step 64
  // of the chain x > y > z > foo > bar > etc."): the chain's sign pinned open by a press keeps its bubble inside the window
  // with no step wider than it, and a step picked by Enter opens that step's pipeline; a second Enter opens nothing more.
  { page: "A pipeline's chain", route: "builds/teamcity%3AbuildType%3APayout_Docker", trips: [
    { id: "build-chain-step-key", kind: "enter-twice", strict: true, finding: "build-header-paths", query: "&fake-long-paths", prep: [{ wait: 600 }, { click: ".tf-chainsign .tf-signbtn", starts: "Step 64" }, { wait: 400 }], fits: ".tf-signbubble:not(.tf-sr)", press: { sel: ".tf-chainlist a", starts: "Payout_Deploy_Staging" }, leaves: true },
  ] },
  // A run's chain folded around it: the header's toggle shows every step and folds them again, keeping focus, so a second
  // Enter never opens a step.
  { page: "A run's chain", route: "build/teamcity%3AbuildType%3APayout_Docker/teamcity%3Abuild%3APayout_Docker-611", trips: [
    { id: "build-chain-unfold-key", kind: "enter-twice", strict: true, finding: "build-header-paths", query: "&fake-long-paths", prep: [{ wait: 800 }], press: { sel: ".tf-chain > header button", starts: "Show all" }, rows: ".tf-chainrow a" },
  ] },
  { page: "A repository's Open in…", route: REPO, trips: [
    { id: "openin-browser-key", kind: "enter-twice", strict: true, finding: "open-on-gitea", prep: [{ wait: 600 }, { click: ".tf-openin > button", starts: "Open in" }, { wait: 400 }], press: { sel: "[role=menuitem]", starts: "Browser" }, returns: "Open in…" },
    { id: "openin-add-key", kind: "enter-twice", strict: true, finding: "apps-page", prep: [{ wait: 600 }, { click: ".tf-openin > button", starts: "Open in" }, { wait: 400 }], press: { sel: "[role=menuitem]", text: "Add an application" }, leaves: true },
    { id: "app-page-flow", kind: "app-page", strict: true, finding: "apps-page" },
  ] },
  // The working tree in the branch box (2026-11-21; Björn: "I would like to be able to select the current work tree in the
  // branch selection, not just the branches"): chosen, Files is the checkout's `HEAD` with the changes laid over it. A row
  // opens once by key, the changed file read from disk; Esc in its fields keeps focus.
  { page: "The working tree in the branch box", route: `${REPO}/files/src/sepa/retry.rs`, trips: [
    { id: "wt-row-key", kind: "enter-twice", strict: true, finding: "working-tree", prep: [{ wait: 900 }, { eval: CHOOSE_WORKING_TREE }, { wait: 900 }], press: { sel: ".tf-ftree a[data-path='Cargo.toml']" }, leaves: true },
    { id: "wt-fields", kind: "esc-sweep", strict: true, finding: "working-tree", prep: [{ wait: 900 }, { eval: CHOOSE_WORKING_TREE }, { wait: 900 }] },
  ] },
  // Commits on the working tree (the themes follow-up, 2026-11-21): *Uncommitted changes* first, opening Local changes
  // once by key and once with the pointer still; the commit to push under it, drawn plain.
  { page: "Commits on the working tree", route: `${REPO}/commits`, trips: [
    { id: "wt-commits-uncommitted-key", kind: "enter-twice", strict: true, finding: "working-tree", prep: [{ wait: 900 }, { eval: CHOOSE_WORKING_TREE }, { wait: 900 }], press: { sel: ".tf-rlocal a", text: "Uncommitted changes" }, leaves: true },
    { id: "wt-commits-uncommitted-pointer", kind: "press-result", strict: true, finding: "working-tree", prep: [{ wait: 900 }, { eval: CHOOSE_WORKING_TREE }, { wait: 900 }], steps: [{ press: { sel: ".tf-rlocal a", text: "Uncommitted changes" } }], leaves: true, check: `() => location.hash.endsWith("/local") && document.querySelector(".tf-kept:not([hidden]) .tf-tabs a[aria-current=page]")?.textContent.trim().startsWith("Local changes") ? null : "the press went to " + location.hash` },
    { id: "wt-commits-fields", kind: "esc-sweep", strict: true, finding: "working-tree", prep: [{ wait: 900 }, { eval: CHOOSE_WORKING_TREE }, { wait: 900 }] },
  ] },
  // Folder renderers (ADR-0044, ux17): every press they brought, twice by key and twice with the pointer still.
  { page: "A folder drawn by a folder renderer", route: `${REPO}/render/folder-listing/src`, trips: [
    { id: "folder-run-key", kind: "enter-twice", strict: true, finding: "folder-render", query: "&fake-folder-render", prep: [{ wait: 2500 }], press: { sel: ".tf-frender-run", text: "Run again" } },
    { id: "folder-run-pointer", kind: "pointer-twice", strict: true, finding: "folder-render", query: "&fake-folder-render", prep: [{ wait: 2500 }], press: { sel: ".tf-frender-run", text: "Run again" }, rows: ".tf-frender-run" },
    // On the working tree (ux29): the folder rendered from the working tree on disk, *Run again* once by key and once
    // with the pointer still.
    { id: "wt-folder-run-key", kind: "enter-twice", strict: true, finding: "folder-render-working-tree", query: "&fake-folder-render", prep: [{ wait: 900 }, { eval: CHOOSE_WORKING_TREE }, { wait: 2500 }], press: { sel: ".tf-frender-run", text: "Run again" } },
    { id: "wt-folder-run-pointer", kind: "pointer-twice", strict: true, finding: "folder-render-working-tree", query: "&fake-folder-render", prep: [{ wait: 900 }, { eval: CHOOSE_WORKING_TREE }, { wait: 2500 }], press: { sel: ".tf-frender-run", text: "Run again" }, rows: ".tf-frender-run" },
  ] },
  // A renderer's long page scrolls in its frame (2026-11-17), whatever it says of its own overflow.
  { page: "A folder renderer's long page", route: `${REPO}/render/folder-listing/src`, trips: [
    ...[["plain", ""], ["hidden", "hidden"], ["vh", "vh"], ["fixed", "fixed"], ["inner", "inner"], ["wide", "wide"]].map(([name, words]) => ({
      id: `frame-scroll-folder-${name}`, kind: "frame-scroll", finding: "frame-scroll", query: `&fake-folder-render=${encodeURIComponent(`page ${words}`.trim())}`,
      frame: ".tf-frender-frame", header: ".tf-frender > header", fits: true, wide: name === "wide",
    })),
  ] },
  { page: "A file renderer's long page", route: `${REPO}/files/docs/report.cat.html`, trips: [
    ...[["plain", ""], ["hidden", "hidden"], ["wide", "wide"]].map(([name, words]) => ({
      id: `frame-scroll-file-${name}`, kind: "frame-scroll", finding: "frame-scroll", query: `&fake-render-page=${encodeURIComponent(words)}`,
      frame: ".tf-rframe", header: ".tf-fileview header", wide: name === "wide",
    })),
  ] },
  { page: "A folder's row on the tree", route: REPO, trips: [
    { id: "folder-glyph-key", kind: "enter-twice", strict: true, finding: "folder-render", query: "&fake-folder-render", prep: [{ wait: 900 }], press: { sel: ".tf-frmark a" }, leaves: true },
  ] },
  // A folder's generated folder (ADR-0046, `?fake-rerender`'s *Low-code site* on `docs`): its glyph grafts the generated
  // rows under a band, focus staying on the glyph; the band brings the real rows back, focus on the folder's row; *Run
  // again* on the made bar is one press.
  { page: "A folder's generated folder", route: REPO, trips: [
    { id: "rerender-glyph", kind: "enter-twice", strict: true, finding: "folder-render", query: "&fake-rerender", prep: [{ wait: 900 }], press: { sel: '.tf-fdir[data-path="docs"] + .tf-frmark a' }, leaves: true },
  ] },
  { page: "A folder's generated folder, grafted", route: `${REPO}/rendered/site/docs`, trips: [
    { id: "rerender-real", kind: "enter-twice", strict: true, finding: "folder-render", query: "&fake-rerender", prep: [{ wait: 2500 }], press: { sel: ".tf-fband" }, leaves: true, returns: "docs" },
    { id: "rerender-run-key", kind: "enter-twice", strict: true, finding: "folder-render", query: "&fake-rerender", prep: [{ wait: 2500 }], press: { sel: ".tf-madebar-run", text: "Run again" } },
    { id: "rerender-run-pointer", kind: "pointer-twice", strict: true, finding: "folder-render", query: "&fake-rerender", prep: [{ wait: 2500 }], press: { sel: ".tf-madebar-run", text: "Run again" }, rows: ".tf-madebar-run" },
  ] },
  // The folder's pick (ux26, ruling 10): two renderers take `docs` and none is picked, so its glyph opens the pick's menu,
  // which stands inside the window with a 60-character name; a choice by key lands on its generated folder.
  { page: "A folder two renderers take", route: REPO, trips: [
    { id: "render-with-pick", kind: "enter-twice", strict: true, finding: "folder-render", query: "&fake-rerender&fake-rerender-two&fake-long-paths", prep: [{ wait: 900 }, { click: '.tf-fdir[data-path="docs"] + .tf-frmark button' }, { wait: 300 }], fits: ".tf-frpick-m", press: { sel: ".tf-frpick-m button", starts: "Docs diagram" }, leaves: true },
  ] },
  // A folder renderer in a comparison (ux26, the decision list's sections 9 and 10): *Rendered* on, `docs` grouped under its
  // bar; *Run* is one press, its words turning to *Stop* in place with focus on it.
  { page: "A folder in a comparison", route: `${REPO}/compare/~working-tree`, trips: [
    { id: "folder-compare-run-key", kind: "enter-twice", strict: true, finding: "folder-render", query: "&fake-rerender", prep: [{ wait: 900 }, { click: ".tf-drendered" }, { wait: 600 }], press: { sel: ".tf-dfolder .tf-madebar-run", text: "Run" } },
    { id: "folder-compare-run-pointer", kind: "pointer-twice", strict: true, finding: "folder-render", query: "&fake-rerender", prep: [{ wait: 900 }, { click: ".tf-drendered" }, { wait: 600 }], press: { sel: ".tf-dfolder .tf-madebar-run", text: "Run" }, rows: ".tf-dfolder .tf-madebar-run" },
  ] },
  { page: "Settings, adding a folder renderer", route: "settings/renderers/folder/add", trips: [
    { id: "folder-try-key", kind: "enter-twice", strict: true, finding: "folder-render", query: "&fake-folder-render", prep: FOLDER_TYPED, press: { sel: ".tf-apppage button", text: "Try it" } },
    { id: "folder-try-pointer", kind: "pointer-twice", strict: true, finding: "folder-render", query: "&fake-folder-render", once: true, prep: FOLDER_TYPED, press: { sel: ".tf-apppage button", text: "Try it" } },
    { id: "folder-add-key", kind: "enter-twice", strict: true, finding: "folder-render", query: "&fake-folder-render", prep: FOLDER_TYPED, press: { sel: ".tf-apppage button", text: "Add folder renderer" }, leaves: true },
    { id: "folder-add-pointer", kind: "pointer-twice", strict: true, finding: "folder-render", query: "&fake-folder-render", once: true, prep: FOLDER_TYPED, press: { sel: ".tf-apppage button", text: "Add folder renderer" } },
  ] },
  // Settings' nav is a tree (2026-11-21): Appearance's row opens its page and its fold, a second click toggles, the chevron folds alone.
  // Folded first by its chevron, since every fold starts open.
  { page: "Settings, the nav's tree", route: "settings/time", trips: [
    { id: "settings-tree-click", kind: "tree-click", strict: true, finding: "2026-11-21", size: [1440, 900], fold: "Appearance", rail: ".tf-stg-nav", folds: "details.tf-stg-fold", rowsOf: ".tf-stg-tree > a, .tf-stg-fold > summary, .tf-stg-fold > a", under: ":scope > a.tf-stg-child", chevronX: 5,
      prep: [{ wait: 300 }, { eval: "document.querySelector('.tf-stg-fold > summary').dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true, detail: 1 })), 1" }, { wait: 200 }] },
  ] },
  { page: "Settings, Code colours", route: "settings/appearance/code-colours", trips: [
    ...["tanstack", "shiki"].map((engine) => ({ id: `hl-radio-${engine}`, kind: "press-result", finding: "themes", before: PREVIEW_BEFORE, steps: [{ press: { sel: ".tf-thm-eng label", starts: ENGINE_NAMES[engine] } }], check: ENGINE_CHECK(engine) })),
    { id: "hl-radio-pierre", kind: "press-result", finding: "themes", store: { "knobas.highlighter": "shiki" }, before: PREVIEW_BEFORE, steps: [{ press: { sel: ".tf-thm-eng label", starts: ENGINE_NAMES.pierre } }], check: ENGINE_CHECK("pierre") },
    // An arrow key in the group chooses at once, as the re-ruling asks: Pierre Highlights to TanStack Highlight.
    { id: "hl-radio-key", kind: "press-result", finding: "themes", before: `() => (document.querySelector("input[name=tf-thm-engine]:checked").focus(), (${PREVIEW_BEFORE})())`, steps: [{ key: "ArrowRight" }], check: ENGINE_CHECK("tanstack") },
    { id: "theme-duplicate", kind: "press-result", finding: "themes", before: THEMES_BEFORE, steps: [{ press: { sel: ".tf-thm-edh button", text: "Duplicate and edit" } }], check: DUPLICATE_CHECK },
    { id: "theme-import-vscode", kind: "press-result", finding: "themes", before: THEMES_BEFORE, steps: [{ press: { sel: ".tf-thm-top button", text: "Import…" }, file: "themes/vscode-fixture.json" }], check: IMPORT_CHECK(["Trip Harbour Light"], "Imported Trip Harbour Light as a light theme.", { kw: "#B3123B", str: "#0B5A2E" }) },
    { id: "theme-import-zed", kind: "press-result", finding: "themes", before: THEMES_BEFORE, steps: [{ press: { sel: ".tf-thm-top button", text: "Import…" }, file: "themes/zed-fixture.json" }], check: IMPORT_CHECK(["Trip Lantern Light", "Trip Lantern Dark"], "Imported 2 themes.", { kw: "#7A3EB1", str: "#1F7444" }) },
    // The light slot by its own select: focused, then "gi" typed, knobas Light to GitHub Light. Not a press and ↓: on macOS
    // both open the native menu, which headless Chrome can't drive; typing a name's first letters chooses on every platform.
    { id: "theme-slot", kind: "press-result", finding: "themes", before: SLOT_BEFORE, steps: [{ focus: ".tf-thm-slot[data-slot=light] select" }, { chars: "gi" }], check: SLOT_CHECK },
    // GitHub Light for code while the window draws knobas Light (#2516): Edit on the slot, and the line on its window
    // colours spans the editor, the list and the preview in their columns.
    { id: "theme-banner", kind: "press-result", finding: "themes", store: { "knobas.theme.light": "github-light" }, before: THEMES_BEFORE, steps: [{ press: { sel: ".tf-thm-slot[data-slot=light] button", text: "Edit" } }], check: BANNER_CHECK },
    // A colour of one's own theme changed through its swatch's hex field, and its contrast row with it.
    { id: "theme-swatch", kind: "press-result", finding: "themes", store: OWN_THEME_STORE, before: SWATCH_BEFORE, steps: [{ press: { sel: "button.tf-thm-sw[aria-label='Change Keyword']" } }, { press: { sel: "input[aria-label='Keyword as hex']" } }, { selectAll: true }, { type: "#C8B6E2" }], check: SWATCH_CHECK },
  ] },
  // Settings › Appearance › Theme (ADR-0055): the window's slots, a swatch, a row's fix, Tell them apart, import and its band,
  // the pick mode and Put knobas's themes back with its Undo.
  { page: "Settings, Theme", route: "settings/appearance/theme", trips: [
    { id: "app-slot", kind: "press-result", finding: "themes", before: APP_BEFORE, steps: [{ focus: ".tf-thm-slot[data-slot=light] select" }, { chars: "gi" }], check: APP_SLOT_CHECK },
    { id: "app-duplicate", kind: "press-result", finding: "themes", before: APP_BEFORE, steps: [{ press: { sel: ".tf-thm-edh button", text: "Duplicate and edit" } }], check: APP_DUPLICATE_CHECK },
    { id: "app-swatch", kind: "press-result", finding: "themes", store: APP_OWN_STORE, before: APP_BEFORE, steps: [{ press: { sel: "button.tf-thm-sw[aria-label='Change Passed']" } }, { press: { sel: "input[aria-label='Passed as hex']" } }, { selectAll: true }, { type: "#7BC67E" }], check: APP_SWATCH_CHECK },
    { id: "app-fix", kind: "press-result", finding: "themes", store: APP_LOW_STORE, before: APP_BEFORE, steps: [{ press: { sel: ".tf-thm-role[data-row=ok] .tf-thm-why button" } }], check: APP_FIX_CHECK },
    { id: "app-apart", kind: "press-result", finding: "themes", store: APP_OWN_STORE, before: APP_BEFORE, steps: [{ press: { sel: ".tf-apt-told button", text: "Tell them apart" } }], check: APP_APART_CHECK },
    ...[["The app and code", "follows"], ["The app only", "apart"], ["Code only", "code"]].map(([text, how]) => ({ id: `app-import-${how}`, kind: "press-result", finding: "themes", before: APP_BEFORE, steps: [{ press: { sel: ".tf-thm-top button", text: "Import…" }, file: "themes/vscode-app-fixture.json" }, { press: { sel: ".tf-apt-use button", text } }], check: APP_BAND_CHECK(how) })),
    { id: "app-pick", kind: "press-result", finding: "themes", store: APP_OWN_STORE, before: APP_BEFORE, steps: [{ press: { sel: ".tf-apt-pvh button", text: "Pick from the preview" } }, { press: { sel: ".tf-apt-stage .tf-chip.tf-warn[data-keys^='warn ']" } }, { press: { sel: "[role=menuitem]", starts: "Attention's tint" } }], check: APP_PICK_CHECK },
    { id: "app-reset", kind: "press-result", finding: "themes", store: APP_KEPT_STORE, before: APP_BEFORE, steps: [{ press: { sel: ".tf-apt-foot button", text: "Put knobas's themes back" } }], check: APP_RESET_CHECK(false) },
    { id: "app-reset-undo", kind: "press-result", finding: "themes", store: APP_KEPT_STORE, before: APP_BEFORE, steps: [{ press: { sel: ".tf-apt-foot button", text: "Put knobas's themes back" } }, { press: { sel: ".tf-toasts .tf-toast button", text: "Undo" } }], check: APP_RESET_CHECK(true) },
  ] },
  { page: "Settings, Appearance", route: "settings/appearance", trips: [
    { id: "rail-top", kind: "rail-top", strict: true, finding: "F-650", scroller: ".tf-stg-scroll", press: { sel: ".tf-stg-nav a", text: "Keyboard" }, size: [1440, 620] },
  ] },
  { page: "Settings, Monitoring", route: "settings/monitoring", trips: [
    { id: "monitoring-fields", kind: "esc-sweep", finding: "F-641", typed: ["#tf-mon-slow", "#tf-mon-keep"] },
  ] },
  { page: "Settings, Performance", route: "settings/performance", trips: [
    { id: "performance-fields", kind: "esc-sweep", finding: "F-641", typed: ["#tf-perf-kept", "#tf-perf-sites"] },
  ] },
  // Settings › Updates (ADR-0052): the forward button pressed twice by key, which must open nothing, send no second
  // write and keep its place. (No Esc sweep: the page has no field.)
  { page: "Settings, Updates", route: "settings/updates", trips: [
    { id: "updates-download-twice", kind: "enter-twice", strict: true, finding: "updates", query: "&fake-update=available", press: { sel: ".tf-upd-b2", starts: "Download" } },
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
  // A site's saved sign-in (ux29): *Fill sign-in* pressed twice by key fills once (the fake's Confluence has one saved),
  // and Settings' *Save one…* pressed twice opens its dialog once and saves nothing.
  { page: "A site, Confluence", route: "site/confluence", trips: [
    { id: "site-fill-twice", kind: "enter-twice", strict: true, finding: "site-signin", prep: [{ wait: 400 }, { click: ".tf-site button.tf-primary", starts: "Open " }, { wait: 900 }], press: { sel: ".tf-sitebar button", text: "Fill sign-in" } },
  ] },
  { page: "A source's site sign-in", route: "settings/source/jira", trips: [
    { id: "site-signin-save-opens", kind: "enter-twice", strict: true, finding: "site-signin", press: { sel: ".tf-stg-row button", text: "Save one…" }, opens: true },
  ] },
  { page: "A source", route: "settings/source/gitea", trips: [
    { id: "syncing-undo", kind: "undo-stale", strict: true, finding: "F-648", changes: [{ sel: '.tf-main button[aria-label="Syncing"]' }, { sel: '.tf-main button[aria-label="Syncing"]' }], value: "document.querySelector('.tf-main button[aria-label=\"Syncing\"]')?.getAttribute('aria-checked') ?? null" },
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
    { id: "monitors-ack", kind: "enter-twice", finding: "F-351", press: { sel: ".tf-mon-ack button" }, rows: MONITOR_ROWS },
    { id: "monitors-ack-pointer", kind: "pointer-twice", strict: true, once: true, finding: "F-351", press: { sel: ".tf-mon-ack button" } },
  ] },
  // The open monitor's actions (the monitors-page pass, 2026-11-19): beside the roster, on one monitor, so *Hide* turns into
  // *Unhide* under the press and holds as *Acked* does. With no list of *Hide*s to walk, `enter-walks` has nothing to walk,
  // and with no actions in a row, `row-room` has no row to fit them in.
  { page: "Monitors, an open monitor", route: "monitors/at/kuma%3A1", trips: [
    { id: "monitors-hide", kind: "enter-twice", finding: "F-340", press: { sel: ".tf-mon-pacts button", text: "Hide" }, rows: MONITOR_ROWS },
    { id: "monitors-pause", kind: "enter-twice", finding: "F-340", press: { sel: ".tf-mon-pacts button", text: "Pause" }, rows: MONITOR_ROWS },
    { id: "monitors-hide-fast", kind: "enter-fast", strict: true, finding: "F-156", press: { sel: ".tf-mon-pacts button", text: "Hide" }, rows: MONITOR_ROWS },
    { id: "monitors-hide-inbox", kind: "leave-back", finding: "F-356", press: { sel: ".tf-mon-pacts button", text: "Hide" }, rows: MONITOR_ROWS, via: "inbox" },
    { id: "monitors-pause-pointer", kind: "pointer-twice", strict: true, once: true, finding: "F-340", press: { sel: ".tf-mon-pacts button", text: "Pause" } },
    { id: "monitors-hide-pointer", kind: "pointer-twice", strict: true, once: true, finding: "F-340", press: { sel: ".tf-mon-pacts button", text: "Hide" } },
    // What it watches (2026-11-19, Assets back in the nav): the picker's *Link to …* and the new asset's *Create* close their
    // dialog onto the button that opened it, and a second Enter opens nothing.
    { id: "monitors-link-asset", kind: "enter-twice", strict: true, finding: "F-545", prep: [{ click: ".tf-mon-pwacts button", text: "Link an asset…" }, { wait: 700 }, { click: ".tf-dialog .opts button.opt" }, { wait: 400 }], press: { sel: ".tf-dialog footer .tf-primary", starts: "Link to" }, returns: "Link an asset…" },
    { id: "monitors-new-asset", kind: "enter-twice", strict: true, finding: "F-545", prep: [{ click: ".tf-mon-pwacts button", text: "New asset for it…" }, { wait: 700 }], press: { sel: ".tf-dialog footer .tf-primary", text: "Create" }, returns: "New asset for it…" },
  ] },
  { page: "Builds, a run", route: RUN, trips: [
    { id: "builds-rerun", kind: "enter-twice", finding: "F-338", press: { sel: ".tf-bhead .tf-primary" } },
    { id: "show-in-log", kind: "enter-twice", finding: "F-355", press: { sel: ".tf-problem a.tf-btn", text: "Show in log" }, leaves: true },
  ] },
  { page: "A repository's branches", route: `${REPO}/branches`, trips: [
    { id: "new-pr-branches", kind: "enter-twice", finding: "F-241", press: { sel: ".tf-main a.tf-btn", text: "New pull request" }, leaves: true },
    // The default branch's row offers one too, into another branch (2026-11-13: "I cannot make a pullrequest into any branch FROM master").
    { id: "new-pr-default-row", kind: "enter-twice", finding: "from-default", press: { sel: ".tf-main a.tf-btn", text: "New pull request", row: "main" }, leaves: true },
    { id: "new-pr-default-row-pointer", kind: "pointer-twice", strict: true, once: true, finding: "from-default", press: { sel: ".tf-main a.tf-btn", text: "New pull request", row: "main" }, rows: ".tf-rbranches tbody tr" },
  ] },
  { page: "Compare from the default branch", route: `${REPO}/compare/main`, trips: [
    // *Into* is the newest other branch; *Create pull request* sends head main and lands on *Pull requests* (2026-11-13).
    { id: "create-pr-from-default", kind: "enter-twice", finding: "from-default", prep: [{ wait: 900 }], press: { sel: ".tf-cgo .tf-primary", text: "Create pull request" }, leaves: true },
    { id: "create-pr-from-default-pointer", kind: "pointer-twice", strict: true, once: true, finding: "from-default", prep: [{ wait: 900 }], press: { sel: ".tf-cgo .tf-primary", text: "Create pull request" } },
    { id: "create-pr-from-default-back", kind: "leave-back", strict: true, finding: "from-default", prep: [{ wait: 900 }], press: { sel: ".tf-cgo .tf-primary", text: "Create pull request" }, rows: ".tf-main tbody tr", rowsStay: true, via: "inbox", backTo: `${REPO}/prs` },
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
  // A rendered diff (ux26's stage, 2026-11-20): a card's own *Rendered* pressed twice by pointer draws its stage once and
  // keeps it. (The band's *Rendered* is a toggle, whose second press is meant to change it back: vitest holds it.)
  { page: "A commit's rendered diff", route: `${REPO}/commits/mock%3Atidewater%2Fpayout-service%40c90d11e4a7b2`, trips: [
    { id: "rendered-switch-pointer", kind: "pointer-twice", strict: true, finding: "compare-rendered", query: "&fake-rendered-diff", prep: [{ wait: 800 }], press: { sel: '.tf-diff[data-file="README.md"] .tf-fh .tf-seg button', text: "Rendered" }, rows: '.tf-diff[data-file="README.md"] .tf-rstage' },
  ] },
  { page: "A pull request's diff", route: "pr/mock%3Apayout-service%23142/diff", trips: [
    { id: "reply-twice", kind: "enter-twice", strict: true, finding: "F-261", prep: [{ click: ".tf-thr-reply", text: "Reply…" }, { wait: 300 }, { fill: ".tf-reply textarea", text: "A cap at five tries." }, { wait: 150 }], press: { sel: ".tf-reply .tf-primary", text: "Reply" }, returns: "Reply…" },
    { id: "reply-opener-twice", kind: "enter-twice", strict: true, finding: "F-261", press: { sel: ".tf-thr-reply", text: "Reply…" } },
    { id: "resolve-twice", kind: "enter-twice", strict: true, finding: "F-261", press: { sel: ".tf-thread button", text: "Resolve" } },
    { id: "unresolve-twice", kind: "enter-twice", strict: true, finding: "F-261", press: { sel: ".tf-thread button", text: "Unresolve" } },
  ] },
  // A ticket's fields as controls (ADR-0054, 2026-11-22): a double Enter on a picker's control opens it once and writes nothing (the
  // priority's menu and the summary's field take a second Enter as their own pick and save, as every menu and inline editor does).
  { page: "A ticket's fields", route: "ticket/mock%3APAY-231", trips: [
    { id: "field-assignee-open", kind: "enter-twice", strict: true, opens: true, finding: "ADR-0054", press: { sel: "button[aria-label='Assignee: change it']" } },
    { id: "field-labels-open", kind: "enter-twice", strict: true, opens: true, finding: "ADR-0054", press: { sel: "button[aria-label='Labels: change it']" } },
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
    { id: "kept-docs-back", kind: "kept-opening", strict: true, finding: "F-454", then: "back", scroller: ".tf-rd-main", other: "page/confluence%3Apsp-codes" },
    { id: "kept-docs-continue", kind: "kept-opening", strict: true, finding: "F-431", then: "continue", scroller: ".tf-rd-main", press: { sel: "section[aria-label='Continue reading'] a.tf-dh-row" } },
    { id: "kept-docs-continue-away", kind: "kept-opening", strict: true, finding: "F-431", then: "continue", scroller: ".tf-rd-main", via: "inbox", press: { sel: "section[aria-label='Continue reading'] a.tf-dh-row" } },
  ] },
  // `Esc` goes up the page's own section (#2559): from a page to its space, which comes back as it was left.
  { page: "Opening a kept page, Docs", route: "docs/space/ENG", trips: [
    { id: "kept-docs-esc", kind: "kept-opening", strict: true, finding: "F-454", then: "esc", scroller: ".tf-rd-main", other: "page/confluence%3Asepa" },
  ] },
  { page: "Opening a kept page, Docs", route: "page/confluence%3Apsp-codes", trips: [
    { id: "kept-docs-section", kind: "kept-opening", strict: true, finding: "F-454", then: "landing", scroller: ".tf-rd-main", via: "inbox", other: "page/confluence%3Asepa/2", to: "#s-2", top: true, at: 300 },
    { id: "kept-docs-mention", kind: "kept-opening", strict: true, finding: "F-431", then: "landing", scroller: ".tf-rd-main", via: "inbox", other: "page/confluence%3Asepa/mention", to: ".tf-msg.tf-mention", at: 300 },
    { id: "kept-docs-for-you", kind: "kept-opening", strict: true, finding: "F-431", then: "landing", scroller: ".tf-rd-main", via: "inbox", other: "page/confluence%3Asepa/mention", press: { sel: ".tf-dl-for a", starts: "SEPA payout retry design" }, to: ".tf-msg.tf-mention", at: 300 },
  ] },
  { page: "Opening a kept page, an asset", route: "asset/asset%3Aknobas-gitea", trips: [
    { id: "kept-asset-other", kind: "kept-opening", strict: true, finding: "F-454", then: "other", scroller: "aside.pane", via: "inbox", other: "asset/asset%3Aknobas-jira" },
    { id: "kept-asset-same", kind: "kept-opening", strict: true, finding: "F-454", then: "same", scroller: "aside.pane", via: "inbox" },
    // No `esc` trip here since #2559: `Esc` on the ticket an asset opened goes to Tickets, never back; Back is the way back.
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
    { id: "nav-reopen-pr", kind: "nav-reopen", strict: true, finding: "F-274", left: "pr/mock%3Apayout-service%23142/diff", bare: "pr/mock%3Apayout-service%23142", section: "Code", esc: REPO, scroller: ".tf-main", at: 600, row: { sel: ".tf-main a.tf-rowt[href='#/pr/mock%3Apayout-service%23142']" } },
  ] },
  { page: "Way back, a repository", route: "code/repos", trips: [
    { id: "nav-reopen-repo", kind: "nav-reopen", strict: true, finding: "F-274", left: `${REPO}/files/README.md`, bare: REPO, section: "Code", esc: REPO, scroller: ".tf-main", at: 600, row: { sel: `.tf-main tr a[href='#/${REPO}']` } },
  ] },
  // Coming back by the entry to a place deeper than the section's own page (2026-11-13).
  { page: "Way back, a Docs page", route: "page/confluence%3Asepa", trips: [
    { id: "nav-place-docs-page", kind: "nav-place", strict: true, finding: "docs-return", section: "Docs", top: "docs", scroller: ".tf-rd-main", at: 300 },
    { id: "nav-place-docs-far", kind: "nav-place", strict: true, finding: "docs-return", section: "Docs", top: "docs", scroller: ".tf-rd-main", at: 300, how: ["pointer"], far: ["Tickets", "Code", "Builds", "Monitors", "Assets", "Link sets", "Time"] },
    { id: "nav-place-docs-comment", kind: "nav-place", strict: true, finding: "docs-return", section: "Docs", top: "docs", scroller: ".tf-rd-main", at: 300, how: ["pointer"], toast: { prep: [{ fill: "textarea[aria-label='Comment on this page']", text: "Looks right to me." }, { wait: 150 }], press: { sel: ".tf-compose .tf-primary", text: "Comment" }, rows: ".tf-kept:not([hidden]) .tf-msg" } },
  ] },
  { page: "Way back, a Docs space", route: "docs/space/ENG", trips: [
    { id: "nav-place-docs-space", kind: "nav-place", strict: true, finding: "docs-return", section: "Docs", top: "docs" },
  ] },
  { page: "Way back, a ticket", route: "ticket/mock%3APAY-231", trips: [
    { id: "nav-place-ticket", kind: "nav-place", strict: true, finding: "docs-return", section: "Tickets", top: "tickets" },
  ] },
  { page: "Way back, a pull request's tab", route: "pr/mock%3Apayout-service%23142/diff", trips: [
    { id: "nav-place-pr", kind: "nav-place", strict: true, finding: "docs-return", section: "Code", top: "code", scroller: ".tf-main", at: 600 },
  ] },
  { page: "Way back, a repository's file", route: `${REPO}/files/README.md`, trips: [
    { id: "nav-place-repo", kind: "nav-place", strict: true, finding: "docs-return", section: "Code", top: "code", scroller: ".tf-main", at: 600 },
  ] },
  { page: "Way back, a run's log", route: `${RUN}/log`, trips: [
    { id: "nav-place-run", kind: "nav-place", strict: true, finding: "docs-return", section: "Builds", top: "builds", scroller: ".tf-bmain", at: 500 },
  ] },
  { page: "Way back, a marked monitor", route: "monitors/at/kuma%3A3", trips: [
    { id: "nav-place-monitor", kind: "nav-place", strict: true, finding: "docs-return", section: "Monitors", top: "monitors" },
    { id: "nav-place-monitor-hide", kind: "nav-place", strict: true, finding: "docs-return", section: "Monitors", top: "monitors", how: ["pointer"], toast: { press: { sel: ".tf-mon-pacts button", text: "Hide" }, rows: MONITOR_ROWS, hides: true } },
  ] },
  { page: "Way back, an asset", route: "asset/asset%3Aknobas-gitea", trips: [
    { id: "nav-place-asset", kind: "nav-place", strict: true, finding: "docs-return", section: "Assets", top: "assets", scroller: "aside.pane", at: 200 },
  ] },
  { page: "Way back, a link set", route: "sets/payout-stack", trips: [
    { id: "nav-place-set", kind: "nav-place", strict: true, finding: "docs-return", section: "Link sets", top: "sets" },
  ] },
  { page: "Way back, a source's page", route: "settings/source/jira", trips: [
    { id: "nav-place-source", kind: "nav-place", strict: true, finding: "docs-return", section: "Settings", top: "settings/sources" },
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
    { id: "nav-return-run", kind: "nav-return", strict: true, deeper: true, finding: "F-370", section: "Builds", scroll: { ".tf-bmain": 300, ".tf-pl-list": 115 }, steps: [{ press: { sel: ".tf-bmain a[href^='#/build/']:not(svg a)" } }, { scroll: { ".tf-pl-list": 415 } }] },
    { id: "nav-return-same", kind: "nav-return", strict: true, finding: "F-370", section: "Builds", same: true, scroll: { ".tf-bmain": 300, ".tf-pl-list": 115 } },
    { id: "nav-return-card", kind: "nav-return", strict: true, finding: "F-370", section: "Builds", scroll: { ".tf-bmain": 300 }, steps: [{ go: "ticket/mock%3APAY-231" }, { eval: "[...document.querySelectorAll('.tf-tksecs a')].find((a) => a.textContent.trim().startsWith('Builds')).click()" }] },
  ] },
  { page: "Nav return, Builds", route: "builds", trips: [
    { id: "nav-return-builds", kind: "nav-return", strict: true, finding: "F-370", section: "Builds", scroll: { ".tf-bmain": 300, ".tf-pl-list": 115 } },
    // The entry from a run goes to the pipeline's own address, and bare #/builds is that same page (reconcile, round 16).
    { id: "nav-return-builds-run", kind: "nav-return", strict: true, deeper: true, finding: "F-370", section: "Builds", scroll: { ".tf-bmain": 300, ".tf-pl-list": 115 }, steps: [{ press: { sel: ".tf-bmain a[href^='#/build/']:not(svg a)" } }] },
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
    { id: "nav-return-ticket", kind: "nav-return", strict: true, deeper: true, finding: "F-071", section: "Tickets", scroll: { ".tf-cols": 200 }, steps: [{ go: "ticket/mock%3APAY-231" }] },
  ] },
  { page: "Nav return, Code", route: "code", trips: [
    { id: "nav-return-code", kind: "nav-return", strict: true, finding: "F-071", section: "Code", scroll: { ".tf-main": 150 } },
  ] },
  { page: "Nav return, Pull requests", route: "code/prs", trips: [
    { id: "nav-return-prs", kind: "nav-return", strict: true, finding: "F-071", section: "Code", scroll: { ".tf-main": 300 } },
    { id: "nav-return-pr", kind: "nav-return", strict: true, deeper: true, finding: "F-071", section: "Code", scroll: { ".tf-main": 300 }, steps: [{ press: { sel: ".tf-main a[href^='#/pr/']" } }] },
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
const WRITE = /^(set_|delete_|create_|save_|add_|remove_|rename_|update_|install_|uninstall_|apply_|publish_|start_timer$|stop_timer$|start_work_(amend|retry|run|skip)$|submit_write$|amend_write$|discard_write$|flush_writes$|ack_alert$|unack_alert$|complete_inbox_item$|snooze_inbox_item$|dismiss_|accept_suggestion$|ad_hoc_block$|update_block$|log_work$|unlink$|move_|edit_|clone_repo$|hide_|order_dep_layers$|place_dep_kind$|restore_backup$|backup_now$|bind_|clear_tone_overrides$|docs_create_page$|docs_edit_section$|follow_merges$|sync_now$|sync_all$|clone_all$|site_fill$|site_sign_in_save$|site_sign_in_remove$)/;

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
      // Through the app's own invoke (app/src/lib/ipc/invoke.ts): the invoke on Tauri's bridge is read-only.
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
        // An opening's chip (ux21): what a press that hands something to another program says, in place of a toast.
        acks: [...document.querySelectorAll(".tf-ack")].filter(seen).map((t) => say(t.textContent)),
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
const [partK, partM] = flag("part", "1/1").split("/").map(Number);
if (!(partK >= 1 && partK <= partM)) {
  console.error("--part is k/m with 1 <= k <= m");
  process.exit(2);
}
const gapFlag = flag("gap", null);
const strict = argv.includes("--strict");
const tripLimit = Number(flag("trip-limit", 180));
mkdirSync(outDir, { recursive: true });
// Every line also goes to <out dir>/trips.log as it happens: a run piped through `tail` shows nothing until it ends, and
// twice on 2026-11-20 a 26-minute run that way looked hung to the machine's watch.
const logFile = join(outDir, "trips.log");
writeFileSync(logFile, "");
const log = (line) => {
  console.log(line);
  try { appendFileSync(logFile, `${line}\n`); } catch {}
};

const table = PAGES.flatMap((p) => p.trips.map((t) => ({ ...t, page: p.page, route: p.route })));
const every = table.filter((t) => strict || !t.strict);
const wanted = (t) => !only.length || only.some((o) => [t.id, t.kind, t.finding, t.page, t.route].some((v) => v?.includes(o)));

/** One browser for the runner (`trip-runner.mjs`): a fresh headless Chrome and the trips' kinds bound to it. */
async function startBrowser() {
  const chrome = await launch("knobas-trips");
  const { send, evaluate, key, stop, listen } = chrome;
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
    const misfit = t.fits ? await evaluate(FITS(t.fits)) : null;
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
    const reasons = misfit ? [misfit] : [];
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
    // `ack`: the press hands something to another program, which a chip says for a second (ux21), so no toast spans the leave.
    if (t.ack && !after.acks.length) throw new Error(`"${pressed}" raised no chip`);
    if (!t.ack && !after.toasts.length) throw new Error(`"${pressed}" raised no toast`);
    if (!t.rowsStay && after.rows.length >= before.rows.length) throw new Error(`"${pressed}" hid no row (${before.rows.length} then ${after.rows.length})`);
    const toast = t.ack ? after.acks.at(-1) : after.toasts.at(-1);
    await go(t.via);
    const away = await snap();
    await go(t.backTo ?? t.route);
    await sleep(150);
    if (t.reopen) {
      if (!(await evaluate(`window.__trip.click(${JSON.stringify({ sel: t.reopen.click, text: t.reopen.text, starts: t.reopen.starts })})`))) throw new Error(`can't find ${t.reopen.click} on the return`);
      await sleep(700);
    }
    const back = await snap(t.rows);
    if (!t.ack && !back.toasts.includes(toast)) throw new Error(`the toast "${toast}" had gone by the return, so nothing was proved`);
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
    return { reasons, note: `"${pressed}": ${before.rows.length} rows, ${after.rows.length} after the press, ${back.rows.length} after ${t.via} (${away.hash}) and back${t.ack ? "" : ", toast still up"}` };
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
        const deeper = t.deeper ? await evaluate(`location.hash`) : null;
        if (!t.same) { await pressEntry(t.via ?? "Docs"); await opener.settleAfterChange(); await sleep(100); }
        await pressEntry(t.section);
        if (deeper !== null) {
          await opener.settleAfterChange();
          await sleep(300);
          const back = await evaluate(`location.hash`);
          if (back !== deeper) reasons.push(`at ${w} x ${h}, the entry from another section went to ${back}, not back to ${deeper}`);
          await sleep(2200);
          await pressEntry(t.section);
        }
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
          // A pipeline sits under Builds' own page (#2559), so its Esc goes there; every other place come back to stays.
          const up = /^#\/builds\/[^/]+/.test(hash) ? "#/builds" : hash;
          if (after !== up) reasons.push(`at ${w} x ${h}, Esc after a nav return went to ${after}, not ${up}`);
          if (up === hash) for (const sel of Object.keys(wants)) if (Math.abs((await scrollTop(sel)) - reads[sel]) > 2) reasons.push(`at ${w} x ${h}, Esc moved ${sel}`);
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
          if (t.via !== null || t.far?.length) {
            await toEntry(t.section);
            await sleep(300);
            if ((await hashNow()) !== left) reasons.push(`${where}, the entry from another section went to ${await hashNow()}, not back to ${left}`);
            else if (Math.abs((await scrollTop(t.scroller)) - want) > 2) reasons.push(`${where}, the entry came back to ${left} at ${await scrollTop(t.scroller)}, left at ${want}`);
            await sleep(2100);
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
          // `esc`: where `Esc` goes up the page's own section, when that is not the list (#2559); the list is then gone to by address.
          const up = t.esc ? `#/${t.esc}` : list;
          if (out !== up) reasons.push(`${where}, Esc after the reopen went to ${out}, not ${up}`);
          if (out !== list) { await go(t.route); await sleep(300); }
          await press1(t.row);
          await sleep(500);
          const again = await hashNow();
          const top = await scrollTop(t.scroller);
          if (again !== bare) reasons.push(`${where}, a press after Esc went to ${again}, not ${bare}: it is used once`);
          // Where Esc itself landed on `bare`, that page was left at its scroll there, which a return keeps (F-454).
          if (top > 1 && up !== `#/${t.bare}`) reasons.push(`${where}, a press after Esc opened at ${top}, not the top`);
          notes.push(`${w} ${how}: ${reads} of ${want}, focus ${focus}${t.fill ? `, ${t.fill.sel} reads "${kept}"` : ""}`);
        }
      }
    } finally {
      await fit(1440, 900);
    }
    return { reasons, note: notes.join("; ") };
  }

  async function treeWalk(t) {
    const reasons = [];
    const notes = [];
    const RAIL = ".tf-kept:not([hidden]) .tf-rd-tree";
    const read = async () => JSON.parse(await evaluate(`JSON.stringify((() => {
      const rail = document.querySelector(${JSON.stringify(RAIL)});
      const body = rail.querySelector(".tf-dl-body");
      const rows = [...rail.querySelectorAll("summary, a, input, h4")].filter((e) => e.getClientRects().length);
      return { picker: rail.querySelector("select[aria-label='Space']")?.value ?? null, top: body.scrollTop, rows: rows.map((e) => [e.textContent.trim().replace(/\\s+/g, " ").slice(0, 60), Math.round(e.getBoundingClientRect().top)]) };
    })())`));
    const head = (words) => `[...document.querySelectorAll(${JSON.stringify(`${RAIL} .tf-dl-body details > summary`)})].find((s) => s.textContent.trim().startsWith(${JSON.stringify(words)}))`;
    if (t.size) await fit(...t.size);
    try {
      await open(t);
      await sleep(400);
      let pressed = false;
      // What a press made vanish is the rail moving under the walk: a failure once a press has been made, not a trip that can't run.
      const gone = (what) => {
        if (!pressed) throw new Error(`no ${what} in the rail`);
        reasons.push(`after a press the rail no longer holds ${what}`);
      };
      for (const step of t.steps) {
        if (reasons.length > 0) break;
        if (step.fold) {
          if (!(await evaluate(`!!${head(step.fold)}`))) { gone(`the fold "${step.fold}"`); break; }
          await evaluate(`(() => { const s = ${head(step.fold)}; if (!s.parentElement.open) s.click(); })()`);
          await sleep(250);
          continue;
        }
        if (step.scroll !== undefined) {
          const at = await evaluate(`(() => { const b = document.querySelector(${JSON.stringify(`${RAIL} .tf-dl-body`)}); b.scrollTop = ${step.scroll}; return b.scrollTop; })()`);
          if (at < step.scroll - 1) throw new Error(`the rail has nowhere to scroll to ${step.scroll} (${at})`);
          await sleep(150);
          continue;
        }
        // Brought into view and under the pointer first, as a person would scroll to it: what the press moves is measured from there.
        const row = { sel: `${RAIL} .tf-dl-body a[href^="#/page/"]`, starts: step.press };
        if (!(await evaluate(`window.__trip.find(${JSON.stringify(row)}) !== null`))) { gone(`the row "${step.press}"`); break; }
        const at = await stillAt(row);
        await sleep(150);
        const before = await read();
        const was = await hashNow();
        await pressAt(at.x, at.y);
        pressed = true;
        await opener.settleAfterChange();
        await sleep(600);
        const after = await read();
        const now = await hashNow();
        if (now === was) { reasons.push(`"${step.press}" changed no address, so it proves nothing`); continue; }
        if (after.picker !== before.picker) reasons.push(`"${step.press}" turned the Space picker from ${before.picker} to ${after.picker}`);
        if (Math.abs(after.top - before.top) > 1) reasons.push(`"${step.press}" scrolled the rail from ${before.top} to ${after.top}`);
        const moved = before.rows.filter(([words, top], i) => after.rows[i]?.[0] !== words || Math.abs(after.rows[i][1] - top) > 1);
        if (after.rows.length !== before.rows.length) reasons.push(`"${step.press}" took the rail from ${before.rows.length} rows to ${after.rows.length}`);
        else if (moved.length > 0) reasons.push(`"${step.press}" moved ${moved.length} rows of the rail, the first "${moved[0][0]}"`);
        notes.push(`${step.press}: ${after.rows.length} rows, picker ${after.picker}, scroll ${after.top}`);
      }
    } finally {
      if (t.size) await fit(1440, 900);
    }
    return { reasons, note: notes.join("; ") };
  }

  async function treeClick(t) {
    const reasons = [];
    const notes = [];
    const RAIL = t.rail ?? ".tf-kept:not([hidden]) .tf-rd-tree";
    const FOLDS = t.folds ?? ".tf-dl-body details";
    const ROWS_OF = t.rowsOf ?? ".tf-dl-body summary, .tf-dl-body a";
    const UNDER = t.under ?? ":scope > .tf-fold-in a";
    const fold = (words) => `[...document.querySelectorAll(${JSON.stringify(`${RAIL} ${FOLDS}`)})].find((d) => d.querySelector(":scope > summary > a")?.textContent.trim() === ${JSON.stringify(words)})`;
    // Every row of the rail shown, with where it stands, and the fold's own state.
    const read = async (words) => JSON.parse(await evaluate(`JSON.stringify((() => {
      const rail = document.querySelector(${JSON.stringify(RAIL)});
      const rows = [...rail.querySelectorAll(${JSON.stringify(ROWS_OF)})].filter((e) => e.getClientRects().length && !(e.tagName === "A" && e.parentElement.tagName === "SUMMARY"));
      const d = ${fold(words)};
      const at = rows.indexOf(d.querySelector(":scope > summary"));
      return { at, open: d.open, under: [...d.querySelectorAll(${JSON.stringify(UNDER)})].filter((e) => e.getClientRects().length).length, rows: rows.map((e) => [e.textContent.trim().replace(/\\s+/g, " ").slice(0, 60), Math.round(e.getBoundingClientRect().top)]) };
    })())`));
    // Each frame from the press on: whether the fold is open, and how many of its rows show. Open with none is a frame drawn before its rows.
    const watch = (words) => evaluate(`(() => {
      const frames = (window.__tcFrames = []);
      const d = ${fold(words)};
      const end = performance.now() + 900;
      const tick = () => { frames.push([d.open, [...d.querySelectorAll(${JSON.stringify(UNDER)})].filter((e) => e.getClientRects().length).length]); if (performance.now() < end) requestAnimationFrame(tick); };
      requestAnimationFrame(tick);
      return true;
    })()`);
    const empty = async () => JSON.parse(await evaluate(`JSON.stringify(window.__tcFrames.filter(([open, n]) => open && n === 0).length)`));
    const titleAt = async (words) => JSON.parse(await evaluate(`(() => { const a = ${fold(words)}.querySelector(":scope > summary > a"); a.scrollIntoView({ block: "nearest" }); const r = a.getBoundingClientRect(); return JSON.stringify({ x: r.left + Math.min(r.width / 2, 40), y: r.top + r.height / 2 }); })()`));
    const chevronAt = async (words) => JSON.parse(await evaluate(`(() => { const s = ${fold(words)}.querySelector(":scope > summary"); const r = s.getBoundingClientRect(); return JSON.stringify({ x: r.left + ${t.chevronX ?? 10}, y: r.top + r.height / 2 }); })()`));
    // Nothing above the pressed row, nor the row itself, moves: the fold opens below it.
    const still = (what, before, after) => {
      const upTo = before.rows.slice(0, before.at + 1);
      const moved = upTo.filter(([words, top], i) => after.rows[i]?.[0] !== words || Math.abs(after.rows[i][1] - top) > 1);
      if (moved.length > 0) reasons.push(`${what} moved ${moved.length} rows at or above the pressed one, the first "${moved[0][0]}"`);
    };
    const press = async (what, at, want) => {
      const before = await read(t.fold);
      const was = await hashNow();
      await watch(t.fold);
      await mouseTo(at.x, at.y);
      await pressAt(at.x, at.y);
      await opener.settleAfterChange();
      await sleep(950);
      const after = await read(t.fold);
      const now = await hashNow();
      if (want.page === true && now === was) reasons.push(`${what} opened no page`);
      if (want.page === false && now !== was) reasons.push(`${what} went to ${now}`);
      if (after.open !== want.open) reasons.push(`${what} left the fold ${after.open ? "open" : "closed"}`);
      if (want.open && after.under === 0) reasons.push(`${what} opened the fold with none of its pages shown`);
      const late = await empty();
      if (late > 0) reasons.push(`${what} drew the fold open for ${late} frames before its pages`);
      still(what, before, after);
      notes.push(`${what}: ${after.open ? "open" : "closed"}, ${after.under} under, ${late} late frames`);
    };
    if (t.size) await fit(...t.size);
    try {
      await open(t);
      await sleep(400);
      if (!(await evaluate(`!!${fold(t.fold)}`))) throw new Error(`no fold "${t.fold}" in the rail`);
      if (await evaluate(`${fold(t.fold)}.open`)) throw new Error(`the fold "${t.fold}" starts open`);
      await press("the first click", await titleAt(t.fold), { page: true, open: true });
      await press("a second click on the open page", await titleAt(t.fold), { page: false, open: false });
      await press("a third", await titleAt(t.fold), { page: false, open: true });
      await press("the chevron", await chevronAt(t.fold), { page: false, open: false });
      await press("the chevron again", await chevronAt(t.fold), { page: false, open: true });
      // Enter on another closed fold's row opens its page and no fold, as before.
      if (t.keyFold) {
        if (await evaluate(`${fold(t.keyFold)}.open`)) throw new Error(`the fold "${t.keyFold}" starts open`);
        const was = await hashNow();
        await sleep(500); // The press guard, as in nav-reopen.
        if (!(await evaluate(`(() => { const a = ${fold(t.keyFold)}.querySelector(":scope > summary > a"); a.focus(); return document.activeElement === a; })()`))) throw new Error(`can't focus "${t.keyFold}"`);
        await key("Enter");
        await opener.settleAfterChange();
        await sleep(600);
        if ((await hashNow()) === was) reasons.push(`Enter on "${t.keyFold}" opened no page`);
        if (await evaluate(`${fold(t.keyFold)}.open`)) reasons.push(`Enter on "${t.keyFold}" opened its fold`);
        notes.push(`Enter on ${t.keyFold}: ${await hashNow()}`);
      }
    } finally {
      if (t.size) await fit(1440, 900);
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
          // The entry comes back to the space it was left on (2026-11-13); pressed again, once its hold has let go, the home.
          if ((await hashNow()) !== "#/docs/space/ENG") reasons.push(`at ${w}, the entry from Inbox went to ${await hashNow()}, not back to the space`);
          await sleep(2100);
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

  async function navPlace(t) {
    const reasons = [];
    const notes = [];
    const here = `#/${t.route}`;
    const top = `#/${t.top}`;
    const gearSel = ".tf-top a[aria-label='Settings']";
    const entryOf = (name) => (name === "Settings" ? `document.querySelector(${JSON.stringify(gearSel)})` : `[...document.querySelectorAll('.tf-nav a')].find((e) => e.textContent.trim().startsWith(${JSON.stringify(name)}))`);
    const at = async (name) => JSON.parse(await evaluate(`(() => { const r = ${entryOf(name)}.getBoundingClientRect(); return JSON.stringify({ x: r.left + r.width / 2, y: r.top + r.height / 2 }); })()`));
    const mark = `document.querySelector('.tf-kept:not([hidden])')`;
    for (const how of t.how ?? ["pointer", "key"]) {
      await open(t);
      await sleep(500);
      let want = null;
      if (t.scroller) {
        want = await setScroll(t.scroller, t.at ?? 300);
        if (!want || want < 50) throw new Error(`${t.scroller} has nowhere to scroll (${want})`);
      }
      let pressed = null;
      if (t.toast) {
        for (const s of t.toast.prep ?? []) {
          if (s.wait) await sleep(s.wait);
          if (s.fill && !(await evaluate(`window.__trip.fill(${JSON.stringify(s.fill)}, ${JSON.stringify(s.text)})`))) throw new Error(`can't find ${s.fill}`);
        }
        const before = await snap(t.toast.rows);
        // By key, as the leave-back trips press: a row's buttons fade in under a resting pointer.
        await focusPress(t.toast);
        await key("Enter");
        await sleep(500);
        pressed = await snap(t.toast.rows);
        if (!pressed.toasts.length) throw new Error(`"${t.toast.press.text}" raised no toast`);
        if (t.toast.hides && pressed.rows.length >= before.rows.length) throw new Error(`"${t.toast.press.text}" hid no row`);
        if (t.scroller) want = await scrollTop(t.scroller);
      }
      await evaluate(`${mark}.dataset.tripKept = "1"`);
      for (const via of t.far ?? []) await toEntry(via);
      await toEntry(t.via ?? "Inbox");
      const steps = await evaluate(`history.length`);
      if (how === "pointer") {
        const p = await at(t.section);
        await mouseTo(p.x, p.y);
        await sleep(60);
        await pressAt(p.x, p.y);
        await sleep(120);
        await pressAt(p.x, p.y);
      } else {
        await sleep(500); // The press guard's wait on a key press on another control after the pointer's press on `via`.
        await evaluate(`${entryOf(t.section)}.focus()`);
        await key("Enter");
        await sleep(120);
        await key("Enter");
      }
      await opener.settleAfterChange();
      await sleep(700);
      const hash = await hashNow();
      const pushed = (await evaluate(`history.length`)) - steps;
      const kept = await evaluate(`${mark}?.dataset.tripKept === "1"`);
      const reads = t.scroller ? await scrollTop(t.scroller) : null;
      const focus = await evaluate(`(document.activeElement?.closest('.tf-nav a')?.textContent.trim() ?? document.activeElement?.getAttribute('aria-label') ?? document.activeElement?.tagName) || 'nothing'`);
      if (hash !== here) reasons.push(`by ${how}, the entry twice from ${t.via ?? "Inbox"} went to ${hash}, not back to ${here}`);
      if (steps < 50 && pushed !== 1) reasons.push(`by ${how}, the entry twice made ${pushed} history entries, not 1`);
      if (!kept && !t.far) reasons.push(`by ${how}, the page came back drawn anew, not as it was kept`);
      if (want !== null && (reads === null || Math.abs(reads - want) > 2)) reasons.push(`by ${how}, ${t.scroller} was left at ${want} and reads ${reads}`);
      // Focus stays with the press: on the entry after a pointer's, on the page's heading after a key's (F-370, F-071).
      if (how === "pointer" ? !focus.startsWith(t.section) : focus !== "H1" && !focus.startsWith(t.section)) reasons.push(`by ${how}, focus went to ${focus}, not the ${t.section} entry or the page's heading`);
      if (t.toast) {
        const back = await snap(t.toast.rows);
        const toast = pressed.toasts.at(-1);
        if (!t.ack && !back.toasts.includes(toast)) throw new Error(`the toast "${toast}" had gone by the return, so nothing was proved`);
        const same = (r) => r.replace(/\d+/g, "#");
        if (back.rows.map(same).join("\n") !== pressed.rows.map(same).join("\n")) reasons.push(`by ${how}, the rows came back as ${back.rows.length}, ${pressed.rows.length} after the press, under the toast "${toast.slice(0, 50)}"`);
      }
      await sleep(2100);
      const p = await at(t.section);
      await mouseTo(p.x, p.y);
      await sleep(60);
      await pressAt(p.x, p.y);
      await opener.settleAfterChange();
      await sleep(400);
      const again = await hashNow();
      if (!again.startsWith(top)) reasons.push(`by ${how}, a press once the hold let go went to ${again}, not ${top}`);
      notes.push(`${how}: ${hash}${reads === null ? "" : ` at ${reads} of ${want}`}, ${pushed} entry, kept ${kept}, focus ${focus}, then ${again}`);
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
        // A section of Settings goes nowhere on Esc, wherever the gear was pressed (#2559); Back is the way out.
        await key("Escape");
        await opener.settleAfterChange();
        await sleep(400);
        if ((await hashNow()) !== here) reasons.push(`at ${w}, Esc from the gear went to ${await hashNow()}, not nowhere`);
        await toEntry("Inbox");
        await keyOf(",", 188, 4);
        await opener.settleAfterChange();
        await sleep(800);
        if ((await hashNow()) !== here) reasons.push(`at ${w}, ⌘, went to ${await hashNow()}, not ${here}`);
        if (Math.abs((await scrollTop(pane)) - want) > 2) reasons.push(`at ${w}, ⌘, read ${await scrollTop(pane)}, left at ${want}`);
        // On the section: nothing moves, whichever way it's asked, Esc included (#2559).
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
        if ((await hashNow()) !== here) reasons.push(`at ${w}, Esc after the gear on its own section went to ${await hashNow()}`);
        // A source's page: from another section the gear comes back to it (2026-11-13), and on it the gear goes to Sources.
        await go(t.source);
        await sleep(300);
        await toEntry("Inbox");
        await press1(gear);
        await sleep(300);
        if ((await hashNow()) !== `#/${t.source}`) reasons.push(`at ${w}, the gear after a source's page went to ${await hashNow()}, not back to it`);
        await sleep(2100);
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
    const ways = t.ways ?? ["first", "return", "sync", "shown", "launcher"];
    if (t.touch) {
      await send("Emulation.setTouchEmulationEnabled", { enabled: true, maxTouchPoints: 1 });
      try { return await rowsStillOn(t, ways); } finally { await send("Emulation.setTouchEmulationEnabled", { enabled: false }); }
    }
    return rowsStillOn(t, ways);
  }

  async function rowsStillOn(t, ways) {
    const reasons = [];
    const notes = [];
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

  async function appPage(t) {
    await open(t);
    const reasons = [];
    const click = (sel, words) => evaluate(`window.__trip.click(${JSON.stringify({ sel, ...words })})`);
    const field = (sel, text) => evaluate(`window.__trip.fill(${JSON.stringify(sel)}, ${JSON.stringify(text)})`);
    const menu = () => evaluate(`JSON.stringify([...document.querySelectorAll('[role=menuitem]')].map((item) => item.textContent.trim()))`).then(JSON.parse);
    const from = await evaluate("location.hash");
    if (!(await click(".tf-openin > button", { starts: "Open in" }))) throw new Error("no Open in… on the repository");
    await sleep(400);
    if (!(await click("[role=menuitem]", { text: "Add an application" }))) throw new Error("Open in… has no Add an application");
    await opener.settleAfterChange();
    const at = await evaluate("location.hash");
    if (!at.startsWith("#/settings/checkouts/add/")) throw new Error(`Add an application went to ${at}`);
    await sleep(600);
    await field("#tf-app-name", "Trip App");
    await field("#tf-app-command", "trip-app {path}");
    const picked = await evaluate(`(() => { const s = document.querySelector('#tf-app-repo'); if (!s || s.options.length < 2) return null; s.value = s.options[1].value; s.dispatchEvent(new Event('change', { bubbles: true })); return s.options[1].textContent.split('/').pop(); })()`);
    if (picked === null) throw new Error("Try it on offers fewer than two repositories");
    await sleep(400);
    const line = await evaluate(`document.querySelector('.tf-apprun code')?.textContent ?? ''`);
    if (!line.includes(`/${picked}`)) reasons.push(`It will run reads "${line}", not ${picked}'s checkout`);
    await click(".tf-apprun button", { text: "Try it" });
    await sleep(700);
    // The chip names the repository it was tried on (ux21); a toast would mean it did not start.
    const said = JSON.parse(await evaluate(`JSON.stringify([...document.querySelectorAll('.tf-ack, .tf-toast')].map((x) => x.textContent.trim()))`));
    if (!said.includes(`Opening ${picked} in Trip App`)) reasons.push(`Try it said ${JSON.stringify(said)}, not that it is opening ${picked}`);
    await click(".tf-apppage button", { text: "Add application" });
    await opener.settleAfterChange();
    const back = await evaluate("location.hash");
    if (back !== from) reasons.push(`Add application went to ${back}, not back to ${from}`);
    await click(".tf-openin > button", { starts: "Open in" });
    await sleep(400);
    const items = await menu();
    if (!items.some((item) => item.startsWith("Trip App"))) reasons.push(`Open in… lists ${JSON.stringify(items)} after the save, without Trip App`);
    return { reasons, note: `${from} → ${at}, tried on ${picked}, saved, back at ${back}, menu ${items.length} items` };
  }

  // 2026-11-17 (Björn: "After adding a new application for open in it opens another page instead of going back to the checkouts and open…").
  async function listReturn(t) {
    await open(t);
    const reasons = [];
    const notes = [];
    const from = await evaluate("location.hash");
    await go(t.list);
    await sleep(400);
    // A real press on the first `sel` whose row (or itself) holds `row`'s words.
    const press = async (sel, row) => {
      const at = JSON.parse(await evaluate(`JSON.stringify((() => {
        const el = [...document.querySelectorAll(${JSON.stringify(sel)})].find((e) => e.getClientRects().length && (e.closest("tr") ?? e).textContent.includes(${JSON.stringify(row)}));
        if (!el) return null;
        el.scrollIntoView({ block: "nearest" });
        const b = el.getBoundingClientRect();
        return { x: Math.round(b.left + b.width / 2), y: Math.round(b.top + b.height / 2) };
      })())`));
      if (at === null) return false;
      await mouseTo(at.x, at.y);
      await sleep(60);
      await pressAt(at.x, at.y);
      return true;
    };
    for (const step of t.steps) {
      if (!(await press(step.open.sel, step.open.row))) throw new Error(`${step.name}: no ${step.open.sel} on ${step.open.row}'s row`);
      await opener.settleAfterChange();
      await sleep(600);
      const page = await evaluate("location.hash");
      if (page === `#/${t.list}`) throw new Error(`${step.name}: the press opened no page`);
      for (const [sel, text] of step.fill ?? []) if (!(await evaluate(`window.__trip.fill(${JSON.stringify(sel)}, ${JSON.stringify(text)})`))) throw new Error(`${step.name}: no ${sel}`);
      await sleep(200);
      if (!(await press(".tf-apppage button", step.press))) throw new Error(`${step.name}: no ${step.press} on ${page}`);
      await opener.settleAfterChange();
      await sleep(600);
      const back = await evaluate("location.hash");
      if (back !== `#/${t.list}`) {
        reasons.push(`${step.name}: ${step.press} went to ${back}, not back to #/${t.list}`);
        await go(t.list);
        continue;
      }
      const focus = JSON.parse(await evaluate(`JSON.stringify((() => {
        const a = document.activeElement;
        const b = a?.getBoundingClientRect();
        // Its row's words, and the link's own name. (In this template literal a backslash must be doubled to reach the page.)
        return { row: ((a?.closest("tr") ?? a)?.textContent ?? "").replace(/\\s+/g, " ").trim().slice(0, 60), name: a?.getAttribute("aria-label") ?? "", control: !!a?.matches("a[href], button, input, select, textarea, [role=button]"), inView: !!b && b.height > 0 && b.top >= 0 && b.bottom <= innerHeight };
      })())`));
      if (!focus.row.includes(step.lands) && !focus.name.includes(step.lands)) reasons.push(`${step.name}: focus is on "${focus.row}", not on ${step.lands}`);
      else if (!focus.inView) reasons.push(`${step.name}: ${step.lands} has focus out of view`);
      // The press rule: a second press on the way back must land on nothing it would press (a row's *Edit* opens the page again).
      if (focus.control) reasons.push(`${step.name}: focus landed on a control ("${focus.name || focus.row}"), which a second press would press`);
      notes.push(`${step.name}: "${focus.row.slice(0, 24)}"`);
    }
    // Back from the list: the page Settings was opened from, never one of its pages again.
    await evaluate("history.back(), 1");
    await opener.settleAfterChange();
    await sleep(300);
    const after = await evaluate("location.hash");
    if (after !== from) reasons.push(`Back from the list went to ${after}, not ${from}`);
    return { reasons, note: `${notes.join("; ")}; Back to ${after}` };
  }

  async function frameScroll(t) {
    const reasons = [];
    const notes = [];
    const [w, h] = t.size ?? [1440, 900];
    await fit(w, h);
    const attached = [];
    const off = chrome.listen((m) => { if (m.method === "Target.attachedToTarget") attached.push(m.params); });
    try {
      await open(t);
      const box = (sel) => `(() => { const e = document.querySelector(${JSON.stringify(sel)}); if (!e) return null; const r = e.getBoundingClientRect(); return { x: r.x, y: r.y, w: r.width, h: r.height, top: r.top, bottom: r.bottom }; })()`;
      let frame = null;
      for (let i = 0; i < 80 && !frame; i++) {
        frame = await evaluate(box(t.frame));
        if (!frame) await sleep(100);
      }
      if (!frame) throw new Error(`no ${t.frame} drawn`);
      await sleep(600);
      frame = await evaluate(box(t.frame));
      const sandbox = await evaluate(`document.querySelector(${JSON.stringify(t.frame)}).getAttribute("sandbox")`);
      if (sandbox !== "") reasons.push(`the frame's sandbox is ${JSON.stringify(sandbox)}, not ""`);
      const pane = await evaluate(`(() => { const m = document.querySelector(".tf-main"); const r = m.getBoundingClientRect(); return { top: r.top, bottom: r.top + m.clientHeight }; })()`);
      const header = await evaluate(box(t.header));
      if (!header || header.top < pane.top) reasons.push(`at rest the header ${t.header} is out of the pane (${header?.top} above ${pane.top})`);
      if (t.fits && frame.bottom > pane.bottom + 1) reasons.push(`at rest the frame ends at ${Math.round(frame.bottom)}, under the pane's bottom at ${Math.round(pane.bottom)}, so its scrollbar is out of sight`);

      // The frame's document: a target of its own (a sandboxed frame is out of process in Chrome), else a world in the page's frame tree.
      await send("Target.setAutoAttach", { autoAttach: true, waitForDebuggerOnStart: false, flatten: true });
      await sleep(200);
      const target = attached.filter((a) => a.targetInfo.type === "iframe").at(-1);
      let inFrame;
      if (target) {
        inFrame = async (expression) => (await send("Runtime.evaluate", { expression, returnByValue: true }, target.sessionId)).result?.result?.value;
      } else {
        const tree = (await send("Page.getFrameTree")).result.frameTree;
        const child = (tree.childFrames ?? []).map((c) => c.frame).at(-1);
        if (!child) throw new Error("the frame's document can't be reached");
        const world = (await send("Page.createIsolatedWorld", { frameId: child.id, worldName: "frame-scroll" })).result;
        inFrame = async (expression) => (await send("Runtime.evaluate", { expression, returnByValue: true, contextId: world.executionContextId })).result?.result?.value;
      }
      const at = async () => JSON.parse(await inFrame(`JSON.stringify((() => { const s = document.scrollingElement; let inner = 0; for (const e of document.querySelectorAll("body *")) inner = Math.max(inner, e.scrollTop); return { y: Math.max(s.scrollTop, inner), x: s.scrollLeft }; })())`));
      const paneAt = () => evaluate(`document.querySelector(".tf-main").scrollTop`);
      const cx = frame.x + frame.w / 2;
      const cy = Math.min(frame.y + frame.h / 2, (frame.top + pane.bottom) / 2);
      const wheel = async (deltaX, deltaY, modifiers = 0) => {
        await send("Input.dispatchMouseEvent", { type: "mouseWheel", x: cx, y: cy, deltaX, deltaY, modifiers });
        await sleep(150);
      };

      await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: cx, y: cy });
      const start = await at();
      const paneStart = await paneAt();
      for (let i = 0; i < 3; i++) await wheel(0, 100);
      await sleep(300);
      const wheeled = await at();
      const paneWheeled = await paneAt();
      if (wheeled.y <= start.y) reasons.push(`a wheel over the page left it at ${wheeled.y}`);
      if (paneWheeled !== paneStart) reasons.push(`a wheel over the page scrolled the pane from ${paneStart} to ${paneWheeled}`);
      notes.push(`wheel ${start.y} to ${wheeled.y}, pane ${paneStart} to ${paneWheeled}`);
      if (t.wide) {
        await wheel(200, 0);
        await sleep(300);
        const side = await at();
        if (side.x <= wheeled.x) reasons.push(`a sideways wheel over the wide page left it at ${side.x}`);
        notes.push(`sideways ${wheeled.x} to ${side.x}`);
      }

      // ⌘ or Ctrl held from the app, the pointer on the page: the wheel zooms the window and scrolls nothing.
      const level = () => evaluate(`localStorage.getItem("knobas.zoom")`);
      const zoomed = async (label) => {
        const before = await level();
        const from = await at();
        await send("Input.dispatchMouseEvent", { type: "mouseWheel", x: cx, y: cy, deltaX: 0, deltaY: -100, modifiers: 2 });
        await sleep(400);
        const after = await level();
        const to = await at();
        if (after === before) reasons.push(`${label}, Ctrl and the wheel over the page left the zoom at ${before ?? "100 %"}`);
        if (to.y !== from.y) reasons.push(`${label}, Ctrl and the wheel over the page scrolled it from ${from.y} to ${to.y}`);
        notes.push(`${label}: zoom ${before ?? "1"} to ${after}`);
      };
      await send("Input.dispatchKeyEvent", { type: "rawKeyDown", key: "Control", code: "ControlLeft", windowsVirtualKeyCode: 17, modifiers: 2 });
      await zoomed("held from the app");
      await send("Input.dispatchKeyEvent", { type: "keyUp", key: "Control", code: "ControlLeft", windowsVirtualKeyCode: 17 });
      // A plain wheel after the key's release scrolls again.
      const released = await at();
      await wheel(0, 100);
      await wheel(0, 100);
      await sleep(300);
      if ((await at()).y <= released.y) reasons.push(`after Ctrl was let go, the wheel no longer scrolls the page`);

      // A press in the page, then the keys.
      const keys = [["PageDown", "PageDown", 34], ["ArrowDown", "ArrowDown", 40], [" ", "Space", 32]];
      await send("Input.dispatchMouseEvent", { type: "mousePressed", x: cx, y: cy, button: "left", clickCount: 1 });
      await send("Input.dispatchMouseEvent", { type: "mouseReleased", x: cx, y: cy, button: "left", clickCount: 1 });
      await sleep(200);
      for (const [name, code, vk] of keys) {
        const from = await at();
        await send("Input.dispatchKeyEvent", { type: "keyDown", key: name, code, windowsVirtualKeyCode: vk, ...(name === " " ? { text: " " } : {}) });
        await send("Input.dispatchKeyEvent", { type: "keyUp", key: name, code, windowsVirtualKeyCode: vk });
        await sleep(350);
        const to = await at();
        if (to.y <= from.y) reasons.push(`after a press in the page, ${code} left it at ${from.y}`);
        notes.push(`${code} ${from.y} to ${to.y}`);
      }

      // A press on the header takes focus back from the page: Ctrl and the wheel over it zoom the window again.
      const hp = await evaluate(box(`${t.header} .tf-p`));
      if (!hp) throw new Error(`no ${t.header} .tf-p to press`);
      await send("Input.dispatchMouseEvent", { type: "mousePressed", x: hp.x + 4, y: hp.y + hp.h / 2, button: "left", clickCount: 1 });
      await send("Input.dispatchMouseEvent", { type: "mouseReleased", x: hp.x + 4, y: hp.y + hp.h / 2, button: "left", clickCount: 1 });
      await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: cx, y: cy });
      await sleep(150);
      await send("Input.dispatchKeyEvent", { type: "rawKeyDown", key: "Control", code: "ControlLeft", windowsVirtualKeyCode: 17, modifiers: 2 });
      await zoomed("after a press on the header");
      await send("Input.dispatchKeyEvent", { type: "keyUp", key: "Control", code: "ControlLeft", windowsVirtualKeyCode: 17 });
      notes.push(`frame ${Math.round(frame.top)} to ${Math.round(frame.bottom)}, pane to ${Math.round(pane.bottom)}`);
    } finally {
      off?.();
      await send("Target.setAutoAttach", { autoAttach: false, waitForDebuggerOnStart: false, flatten: true }).catch(() => {});
      // Back to 100 %, from the app's own key, with focus in the app.
      await evaluate(`document.querySelector(".tf-main")?.focus?.(); document.activeElement?.blur?.(); 1`).catch(() => {});
      await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: 5, y: 5 }).catch(() => {});
      await key("Control+0").catch(() => {});
      await evaluate(`localStorage.removeItem("knobas.zoom"); 1`).catch(() => {});
      await fit(1440, 900);
    }
    return { reasons, note: notes.join("; ") };
  }

  async function coldStart(t) {
    const reasons = [];
    // Before the app's own scripts: the storage as a restart finds it, and a recorder of each frame from the shell's first.
    const source = String.raw`(() => {
      if (window.top !== window) return;
      ${t.chosen ? `localStorage.setItem("knobas.job.chosen", ${JSON.stringify(t.chosen)});` : `localStorage.removeItem("knobas.job.chosen");`}
      const never = ${JSON.stringify(t.never ?? [])};
      const rec = (window.__cold = { frames: [], painted: [] });
      const look = () => {
        if (document.querySelector(".tf")) {
          const h1 = [...document.querySelectorAll(".tf-main h1")].find((e) => e.getClientRects().length > 0);
          const frame = { hash: location.hash, h1: h1 ? h1.textContent.trim().replace(/\s+/g, " ").slice(0, 40) : null };
          const last = rec.frames.at(-1);
          if (!last || last.hash !== frame.hash || last.h1 !== frame.h1) rec.frames.push(frame);
          const text = document.querySelector(".tf-main")?.innerText ?? "";
          for (const words of never) if (text.includes(words) && !rec.painted.includes(words)) rec.painted.push(words);
        }
        if (rec.frames.length < 50) requestAnimationFrame(look);
      };
      requestAnimationFrame(look);
    })()`;
    const added = await send("Page.addScriptToEvaluateOnNewDocument", { source });
    try {
      await opener.openScreen({ name: t.id, route: t.route, query: t.query, job: false });
      if (t.drawn) {
        for (let i = 0; i < 100 && !(await evaluate(`!!document.querySelector(${JSON.stringify(`.tf-kept:not([hidden]) ${t.drawn}`)})`)); i++) await sleep(50);
      }
      await sleep(400);
    } finally {
      await send("Page.removeScriptToEvaluateOnNewDocument", { identifier: added.result.identifier });
    }
    const rec = JSON.parse(await evaluate(`JSON.stringify(window.__cold ?? null)`));
    if (!rec || rec.frames.length === 0) throw new Error("the shell never drew");
    const first = rec.frames[0];
    for (const words of rec.painted) reasons.push(`"${words}" painted`);
    if (t.first && first.hash !== t.first) reasons.push(`the first frame was at ${first.hash || "no address"}, not ${t.first}`);
    if (t.first && t.heading && !(first.h1 ?? "").startsWith(t.heading)) reasons.push(`the first frame's heading was ${first.h1 === null ? "none" : `"${first.h1}"`}, not "${t.heading}"`);
    const last = rec.frames.at(-1);
    const hash = await evaluate(`location.hash`);
    if (hash !== t.lands) reasons.push(`it ended at ${hash || "no address"}, not ${t.lands}`);
    if (t.heading && !(last.h1 ?? "").startsWith(t.heading)) reasons.push(`it ended under "${last.h1}", not "${t.heading}"`);
    if (t.drawn && !(await evaluate(`!!document.querySelector(${JSON.stringify(`.tf-kept:not([hidden]) ${t.drawn}`)})`))) reasons.push(`${t.drawn} never drew`);
    if (t.chosen && t.lands === "#/tickets" && (await evaluate(`localStorage.getItem("knobas.job.chosen")`)) !== null) reasons.push("the gone job is still chosen");
    return { reasons, note: rec.frames.map((f) => `${f.hash} "${f.h1 ?? ""}"`).join(" → ") };
  }

  async function rightClick(t) {
    await open(t);
    const reasons = [];
    const ids = () => evaluate(`JSON.stringify([...document.querySelectorAll('.tf-ctx-m')].slice(0, 1).flatMap((m) => [...m.querySelectorAll('[role=menuitem]')].map((i) => i.dataset.item)))`).then(JSON.parse);
    const menus = () => evaluate(`document.querySelectorAll('.tf-ctx-m').length`);
    // The page's own clipboard stub, and a listener after the app's that reads what it did with the event.
    await evaluate(`(() => {
      window.__copied = [];
      try { Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText: (t) => (window.__copied.push(t), Promise.resolve()), write: () => Promise.resolve() } }); } catch {}
      window.__ctx = null;
      window.addEventListener("contextmenu", (e) => {
        const at = performance.now();
        window.__ctx = { prevented: e.defaultPrevented, frame: null };
        requestAnimationFrame(() => { window.__ctx.frame = document.querySelectorAll(".tf-ctx-m").length; window.__ctx.ms = performance.now() - at; });
      });
      return 1;
    })()`);
    const raw = await evaluate(`window.__trip.point(${JSON.stringify(t.on)})`);
    if (!raw) throw new Error(`can't find ${t.on.sel}`);
    const at = JSON.parse(raw);
    if (t.select) await evaluate(`(() => { const r = document.createRange(); r.selectNodeContents(window.__pressed); const s = getSelection(); s.removeAllRanges(); s.addRange(r); return s.toString(); })()`);
    const before = await snap();
    const right = async () => {
      await mouseTo(at.x, at.y);
      await send("Input.dispatchMouseEvent", { type: "mousePressed", x: at.x, y: at.y, button: "right", buttons: 2, clickCount: 1 });
      await send("Input.dispatchMouseEvent", { type: "mouseReleased", x: at.x, y: at.y, button: "right", buttons: 0, clickCount: 1 });
    };
    if (t.key) {
      if (!(await evaluate(`(window.__pressed.focus(), document.activeElement === window.__pressed)`))) throw new Error(`${t.on.sel} takes no focus`);
      await keyOf("F10", 121, 8);
    } else await right();
    await sleep(80);
    const ctx = JSON.parse(await evaluate(`JSON.stringify(window.__ctx)`));
    if (t.platform) {
      if (ctx?.prevented) reasons.push("a field's web view menu was stopped");
      if ((await menus()) > 0) reasons.push("knobas drew a menu on a field");
      return { reasons, note: "the field keeps the web view's own menu" };
    }
    if (!t.key && !ctx) throw new Error("no contextmenu reached the page");
    if (!t.key && !ctx.prevented) reasons.push("the web view's own menu was not stopped");
    if (t.none) {
      if ((await menus()) > 0) reasons.push(`a menu opened on nothing: ${JSON.stringify(await ids())}`);
      return { reasons, note: "nothing drawn, the web view's menu stopped" };
    }
    if (!t.key && ctx.frame !== 1) reasons.push(`the menu wasn't up by the next frame (${ctx.frame} menus)`);
    const got = await ids();
    if (t.items && JSON.stringify(got) !== JSON.stringify(t.items)) reasons.push(`its rows are ${JSON.stringify(got)}, not ${JSON.stringify(t.items)}`);
    if (t.starts && JSON.stringify(got.slice(0, t.starts.length)) !== JSON.stringify(t.starts)) reasons.push(`its rows start ${JSON.stringify(got.slice(0, t.starts.length))}, not ${JSON.stringify(t.starts)}`);
    const opened = await snap();
    if (opened.hash !== before.hash) reasons.push(`opening it moved the address to ${opened.hash}`);
    if (added(opened, before).length) reasons.push(`opening it wrote ${added(opened, before).join(", ")}`);
    if (t.key) {
      const first = await evaluate(`document.activeElement?.dataset?.item ?? null`);
      if (first !== got[0]) reasons.push(`by key, focus is on ${first}, not the first row`);
    } else {
      await right();
      await sleep(80);
      if ((await menus()) !== 1 || JSON.stringify(await ids()) !== JSON.stringify(got)) reasons.push("a second right press there didn't draw it again in place");
    }
    await key("Escape");
    await sleep(150);
    if ((await menus()) > 0) reasons.push("Esc left it open");
    if (t.key && !(await evaluate(`document.activeElement === window.__pressed`))) reasons.push("Esc didn't give focus back to what had it");
    if (!t.pick) return { reasons, note: `${got.length} rows, up by the next frame, Esc closes` };
    if (t.select) await evaluate(`(() => { const r = document.createRange(); r.selectNodeContents(window.__pressed); const s = getSelection(); s.removeAllRanges(); s.addRange(r); return 1; })()`);
    if (t.key) await keyOf("F10", 121, 8); else await right();
    await sleep(120);
    const row = async (id, sub) => {
      const p = await evaluate(`(() => { const m = [...document.querySelectorAll('.tf-ctx-m')][${sub ? 1 : 0}]; const e = m?.querySelector('[data-item="${id}"]'); if (!e) return null; const r = e.getBoundingClientRect(); return JSON.stringify({ x: r.left + r.width / 2, y: r.top + r.height / 2 }); })()`);
      if (!p) throw new Error(`no row ${id}${sub ? " in Open in…" : ""}`);
      return JSON.parse(p);
    };
    const pick = await row(t.pick, false);
    await mouseTo(pick.x, pick.y);
    await sleep(100);
    if (t.sub) {
      const sub = await row(t.sub, true);
      // Along the row first, then down into the submenu, as a hand goes.
      await mouseTo(sub.x, pick.y);
      await mouseTo(sub.x, sub.y);
      await sleep(80);
      await pressAt(sub.x, sub.y);
    } else await pressAt(pick.x, pick.y);
    await sleep(400);
    const after = await snap();
    if ((await menus()) > 0) reasons.push("the pick left the menu open");
    if (t.toast && !after.toasts.some((x) => x.startsWith(t.toast))) reasons.push(`no toast starting "${t.toast}": ${tidy(after.toasts)}`);
    if (t.ack && !after.acks.some((x) => x.startsWith(t.ack))) reasons.push(`no chip starting "${t.ack}": ${tidy(after.acks)}`);
    if (t.hash && !after.hash.startsWith(t.hash)) reasons.push(`the pick went to ${after.hash}, not ${t.hash}…`);
    if (t.launcher) {
      const box = await evaluate(`document.querySelector('[role=dialog] input')?.value ?? null`);
      if (!box) reasons.push("the launcher didn't open with the words");
    }
    return { reasons, note: `${got.length} rows, up by the next frame (${Math.round(ctx?.ms ?? 0)} ms), Esc closes, ${t.sub ? `${t.pick} › ${t.sub}` : t.pick}: ${(after.toasts.at(-1) ?? after.hash).slice(0, 48)}` };
  }

  async function rightSpam(t) {
    await open(t);
    const reasons = [];
    // What stands: every menu node, the right-click menus among them, rows still marked, and where each menu row is.
    // `owned`: the menus an open menu accounts for, itself and the submenu its *Open in… ▸* says is open. Any other is left over.
    const state = async () => JSON.parse(await evaluate(`JSON.stringify((() => { const ctx = [...document.querySelectorAll('.tf-ctx-m[aria-label="Actions"]')]; return { menus: document.querySelectorAll(".tf-menu").length, ctx: ctx.length, owned: ctx.length + ctx.reduce((n, m) => n + m.querySelectorAll('[aria-expanded="true"]').length, 0), marked: document.querySelectorAll("[data-ctx-on]").length, hash: location.hash }; })())`));
    const rowAt = async (id) => {
      const raw = await evaluate(`(() => { const m = document.querySelector('.tf-ctx-m[aria-label="Actions"]'); const e = m && (${JSON.stringify(id)} ? m.querySelector('[data-item="' + ${JSON.stringify(id)} + '"]') : [...m.querySelectorAll('[role=menuitem]')][Math.floor(Math.random() * m.querySelectorAll('[role=menuitem]').length)]); if (!e) return null; const r = e.getBoundingClientRect(); return JSON.stringify({ x: Math.round(r.left + Math.min(24, r.width / 2)), y: Math.round(r.top + r.height / 2) }); })()`);
      return raw ? JSON.parse(raw) : null;
    };
    const spots = async () => JSON.parse(await evaluate(`JSON.stringify([...document.querySelectorAll(${JSON.stringify(t.on.sel)})].filter((e) => e.getClientRects().length && !e.closest("[hidden], .tf-menu")).map((e) => e.getBoundingClientRect()).filter((r) => r.top > 0 && r.bottom < innerHeight).slice(0, 6).map((r) => ({ x: Math.round(r.left + Math.min(40, r.width / 2)), y: Math.round(r.top + r.height / 2) })))`));
    const press = async (at, button) => {
      await mouseTo(at.x, at.y);
      await send("Input.dispatchMouseEvent", { type: "mousePressed", x: at.x, y: at.y, button, buttons: button === "right" ? 2 : 1, clickCount: 1 });
      await sleep(Math.random() * 15);
      await send("Input.dispatchMouseEvent", { type: "mouseReleased", x: at.x, y: at.y, button, buttons: 0, clickCount: 1 });
    };
    let most = 0;
    let leftOver = null;
    const notes = [];
    for (let round = 0; round < 3; round++) {
      if (round > 0) await open(t);
      const list = await spots();
      if (!list.length) throw new Error(`can't find ${t.on.sel}`);
      let at = list[0];
      for (let i = 0; i < 24; i++) {
        if (i % 6 === 5) {
          // The hand comes to rest on the open menu for a moment, as it does between presses.
          const row = await rowAt(null);
          if (row) await mouseTo(row.x, row.y);
          await sleep(30);
        }
        if (Math.random() < 0.25) at = list[Math.floor(Math.random() * list.length)];
        await press({ x: at.x + Math.round(Math.random() * 8 - 4), y: at.y + Math.round(Math.random() * 8 - 4) }, "right");
        await sleep(10 + Math.random() * 40);
        const now = await state();
        most = Math.max(most, now.ctx);
        if (now.menus > now.owned && !leftOver) leftOver = `round ${round + 1}, press ${i + 1}: ${now.menus - now.owned} menu left over that no open menu accounts for`;
      }
      await sleep(150);
      if ((await state()).ctx === 0) await press(list[0], "right");
      await sleep(120);
      if (t.rest) {
        const row = await rowAt(t.rest);
        if (!row) throw new Error(`no row ${t.rest} in the menu on ${t.on.sel}`);
        await mouseTo(row.x - 6, row.y);
        await mouseTo(row.x, row.y);
        await sleep(200);
      }
      const before = await state();
      most = Math.max(most, before.ctx);
      if (before.ctx === 0) {
        reasons.push(`round ${round + 1}: no menu stood after the spam`);
        continue;
      }
      if (t.close === "esc") await key("Escape");
      else if (t.close === "blur") await evaluate(`window.dispatchEvent(new Event("blur")), 1`);
      else if (t.close === "hash") await evaluate(`location.hash = "#/time", 1`);
      else {
        const away = JSON.parse(await evaluate(`JSON.stringify((() => { const all = [...document.querySelectorAll(".tf-menu")].map((m) => m.getBoundingClientRect()); const right = Math.max(...all.map((r) => r.right)); const left = Math.min(...all.map((r) => r.left)); const top = Math.min(...all.map((r) => r.top)); return left > 120 ? { x: left - 60, y: top + 4 } : { x: Math.min(innerWidth - 8, right + 60), y: top + 4 }; })())`));
        await press(away, "left");
      }
      await sleep(300);
      const after = await state();
      notes.push(`${before.menus} → ${after.menus}`);
      if (after.menus > 0) reasons.push(`round ${round + 1}: ${t.close} left ${after.menus} of ${before.menus} menus standing`);
      if (after.marked > 0) reasons.push(`round ${round + 1}: ${t.close} left the row marked as the menu's`);
      if (after.menus > 0) {
        // Whether anything still closes it: a left press away and Esc.
        await press({ x: 8, y: 8 }, "left");
        await key("Escape");
        await sleep(200);
        if ((await state()).menus > 0) reasons.push(`round ${round + 1}: nothing closes it now, not a left press away nor Esc`);
      }
    }
    if (most > 1) reasons.push(`${most} right-click menus stood at once`);
    if (leftOver) reasons.push(leftOver);
    return { reasons, note: `3 rounds of 24 right presses, resting on ${t.rest ?? "a row"}, then ${t.close}: menus ${notes.join(", ")}` };
  }

  async function pressResult(t) {
    await open(t);
    if (t.store) {
      await evaluate(`(Object.entries(${JSON.stringify(t.store)}).forEach(([k, v]) => localStorage.setItem(k, v)), 1)`);
      await send("Page.reload", {});
      await sleep(100);
      for (let i = 0; i < 400 && !(await evaluate(`document.readyState === "complete" && !!document.querySelector(".tf")`).catch(() => false)); i++) await sleep(25);
      await opener.settleAfterChange();
      if (!(await evaluate(`window.__trip.wrap()`))) throw new Error("no __TAURI_INTERNALS__ to wrap after the reload");
    }
    const before = await evaluate(`JSON.stringify((${t.before ?? "() => null"})())`);
    const first = await snap();
    const did = [];
    for (const s of t.steps) {
      if (s.press) {
        const raw = await evaluate(`window.__trip.point(${JSON.stringify(s.press)})`);
        if (!raw) throw new Error(`can't find ${s.press.sel} ${s.press.text ?? s.press.starts ?? ""}`.trim());
        const p = JSON.parse(raw);
        if (!p.under) throw new Error(`something else is at 15% of "${p.words}"`);
        let chosen = null;
        let unlisten = () => {};
        if (s.file) {
          await send("Page.setInterceptFileChooserDialog", { enabled: true });
          chosen = new Promise((resolve) => { unlisten = listen((m) => { if (m.method === "Page.fileChooserOpened") resolve(m.params); }); });
        }
        await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: p.x, y: p.y });
        await sleep(60);
        await send("Input.dispatchMouseEvent", { type: "mousePressed", x: p.x, y: p.y, button: "left", clickCount: 1 });
        await send("Input.dispatchMouseEvent", { type: "mouseReleased", x: p.x, y: p.y, button: "left", clickCount: 1 });
        if (s.file) {
          const opened = await Promise.race([chosen, sleep(3000).then(() => null)]);
          unlisten();
          await send("Page.setInterceptFileChooserDialog", { enabled: false });
          if (!opened) throw new Error(`the press on "${p.words}" opened no file chooser`);
          const path = new URL(s.file, import.meta.url).pathname;
          await send("DOM.setFileInputFiles", { files: [path], backendNodeId: opened.backendNodeId });
          did.push(`pressed "${p.words}" and chose ${s.file}`);
        } else did.push(`pressed "${p.words.slice(0, 48)}"`);
      } else if (s.key) {
        await key(s.key);
        did.push(s.key);
      } else if (s.focus) {
        const found = await evaluate(`(() => { const e = document.querySelector(".tf-kept:not([hidden]) " + ${JSON.stringify(s.focus)}) ?? document.querySelector(${JSON.stringify(s.focus)}); e?.focus(); return !!e && document.activeElement === e; })()`);
        if (!found) throw new Error(`can't focus ${s.focus}`);
        did.push(`focused ${s.focus}`);
      } else if (s.chars) {
        for (const ch of s.chars) {
          const code = /[a-z]/i.test(ch) ? `Key${ch.toUpperCase()}` : "";
          await send("Input.dispatchKeyEvent", { type: "keyDown", key: ch, code, text: ch, unmodifiedText: ch, windowsVirtualKeyCode: ch.toUpperCase().charCodeAt(0) });
          await send("Input.dispatchKeyEvent", { type: "keyUp", key: ch, code, windowsVirtualKeyCode: ch.toUpperCase().charCodeAt(0) });
        }
        did.push(`pressed ${s.chars}`);
      } else if (s.selectAll) {
        const modifiers = process.platform === "darwin" ? 4 : 2;
        await send("Input.dispatchKeyEvent", { type: "keyDown", key: "a", code: "KeyA", windowsVirtualKeyCode: 65, modifiers, commands: ["selectAll"] });
        await send("Input.dispatchKeyEvent", { type: "keyUp", key: "a", code: "KeyA", windowsVirtualKeyCode: 65, modifiers });
        did.push(process.platform === "darwin" ? "⌘A" : "Ctrl+A");
      } else if (s.type) {
        await send("Input.insertText", { text: s.type });
        did.push(`typed ${s.type}`);
      }
      await sleep(s.wait ?? 150);
    }
    const until = Date.now() + (t.within ?? 4000);
    let why;
    for (;;) {
      why = await evaluate(`(${t.check})(${before})`);
      if (!why || Date.now() > until) break;
      await sleep(100);
    }
    const after = await snap();
    const reasons = why ? [why] : [];
    if (after.hash !== first.hash && !t.leaves) reasons.push(`the address moved to ${after.hash}`);
    if (after.dialogs > first.dialogs) reasons.push(`a dialog opened: ${after.dialog}`);
    return { reasons, note: `${did.join(", then ")}: ${why ? "not as asked" : "as asked"}` };
  }

  async function cardDrag(t) {
    const reasons = [];
    // Each drawn column by its status, with its standing and its cards; whether a card is held; what stands in the float host.
    const read = async () => JSON.parse(await evaluate(`JSON.stringify((() => {
      const cols = [...document.querySelectorAll(".tf-col[data-place]")].filter((c) => c.getClientRects().length);
      return {
        cols: cols.map((c) => ({ status: c.dataset.place.split("\\u0001")[1], drop: c.dataset.drop ?? null, cards: [...c.querySelectorAll(".tf-tcard[data-entity]")].map((n) => [n.dataset.entity, Math.round(n.getBoundingClientRect().top), Math.round(n.getBoundingClientRect().left)]) })),
        held: !!document.querySelector(".tf-board-wrap[data-held]"),
        floats: document.querySelectorAll(".tf-floats .tf-tghost, .tf-floats .tf-tline, .tf-floats .tf-twhy").length,
      };
    })())`));
    const col = (state, status) => state.cols.find((c) => c.status?.toLowerCase() === status.toLowerCase());
    const pointOf = async (sel) => JSON.parse(await evaluate(`(() => { const n = ${sel}; if (!n) return "null"; const r = n.getBoundingClientRect(); return JSON.stringify({ x: r.left + Math.min(40, r.width / 2), y: r.top + Math.min(20, r.height / 2) }); })()`));
    const colSel = (status) => `[...document.querySelectorAll(".tf-col[data-place]")].find((c) => c.dataset.place.split("\\u0001")[1]?.toLowerCase() === ${JSON.stringify(status.toLowerCase())})`;
    const firstCard = (status, skip = "") => `${colSel(status)}?.querySelector(".tf-tcard[data-entity]${skip ? `:not([data-entity='${skip}'])` : ""}")`;
    const atRest = (what, state) => {
      if (state.held) reasons.push(`${what}: the board still says a card is held`);
      const lit = state.cols.filter((c) => c.drop !== null).map((c) => c.status);
      if (lit.length) reasons.push(`${what}: ${lit.join(", ")} kept data-drop`);
      if (state.floats) reasons.push(`${what}: the float host kept ${state.floats} of the drag's floats`);
    };
    // Every card but the moved one stands where it stood.
    const stayed = (what, before, after, moved) => {
      const where = (state) => new Map(state.cols.flatMap((c) => c.cards.filter(([id]) => id !== moved).map(([id, top, left]) => [id, `${c.status} ${top} ${left}`])));
      const was = where(before);
      const now = where(after);
      const shifted = [...was].filter(([id, at]) => now.has(id) && now.get(id) !== at && !col(after, t.to)?.cards.some(([other]) => other === id) && !col(after, t.from)?.cards.some(([other]) => other === id));
      if (shifted.length) reasons.push(`${what}: ${shifted.length} cards outside the two columns moved, the first ${shifted[0][0]}`);
    };
    const dragTo = async (card, status) => {
      const from = await pointOf(card);
      if (!from) throw new Error(`no card in ${t.from}`);
      const to = await pointOf(`${colSel(status)}?.querySelector(".tf-stack")`);
      if (!to) throw new Error(`no column ${status}`);
      await mouseTo(from.x, from.y);
      await sleep(250); // The rest asks where it can go.
      await send("Input.dispatchMouseEvent", { type: "mousePressed", x: from.x, y: from.y, button: "left", clickCount: 1 });
      await glide(from, { x: to.x + 60, y: to.y + 40 }, 300);
      await sleep(120);
      const held = await read();
      await send("Input.dispatchMouseEvent", { type: "mouseReleased", x: to.x + 60, y: to.y + 40, button: "left", clickCount: 1 });
      await sleep(900);
      return held;
    };
    if (t.size) await fit(...t.size);
    await open(t);
    await sleep(800);
    const start = await read();
    if (!col(start, t.from) || !col(start, t.to) || !col(start, t.no)) throw new Error(`the board has no ${t.from}, ${t.to} or ${t.no} column: ${start.cols.map((c) => c.status).join(", ")}`);
    atRest("before", start);
    // A drag to a column it can reach.
    const moving = await evaluate(`${firstCard(t.from)}?.dataset.entity ?? null`);
    if (!moving) throw new Error(`no card in ${t.from}`);
    const beforeWrites = await snap();
    const lit = await dragTo(firstCard(t.from), t.to);
    if (!lit.held) reasons.push("while held the board did not say a card is held");
    if (col(lit, t.to)?.drop !== "yes") reasons.push(`while held ${t.to} said ${col(lit, t.to)?.drop ?? "nothing"}, not yes`);
    if (col(lit, t.no)?.drop !== "no") reasons.push(`while held ${t.no} said ${col(lit, t.no)?.drop ?? "nothing"}, not no`);
    if (col(lit, t.from)?.drop !== null) reasons.push(`while held its own column said ${col(lit, t.from)?.drop}`);
    const landed = await read();
    atRest("after the drop", landed);
    if (!col(landed, t.to)?.cards.some(([id]) => id === moving)) reasons.push(`${moving} is not in ${t.to} after the drop`);
    if (col(landed, t.from)?.cards.some(([id]) => id === moving)) reasons.push(`${moving} is still in ${t.from}`);
    const chip = await evaluate(`document.querySelector('.tf-tcard[data-entity="${moving}"] .tf-twrite .tf-won')?.textContent ?? ""`);
    const writes = added(await snap(), beforeWrites).filter((w) => w === "Transition");
    if (writes.length !== 1) reasons.push(`the drop queued ${writes.length} writes`);
    stayed("the drop", start, landed, moving);
    // A drag to a column it cannot reach.
    const stays = await evaluate(`${firstCard(t.from, moving)}?.dataset.entity ?? null`);
    let refusedNote = "no second card";
    if (stays) {
      const quiet = await snap();
      const shut = await dragTo(firstCard(t.from, moving), t.no);
      if (col(shut, t.no)?.drop !== "no") reasons.push(`the second drag found ${t.no} saying ${col(shut, t.no)?.drop ?? "nothing"}`);
      const back = await read();
      atRest("after the refused drop", back);
      if (!col(back, t.from)?.cards.some(([id]) => id === stays)) reasons.push(`${stays} left ${t.from} on a drop where it cannot go`);
      const more = added(await snap(), quiet).filter((w) => w === "Transition");
      if (more.length) reasons.push(`the refused drop queued ${more.length} writes`);
      refusedNote = `${stays} stayed in ${t.from}`;
    }
    return { reasons, note: `${moving} dragged to ${t.to} (chip "${chip}", ${writes.length} write), ${refusedNote}` };
  }

  const KINDS = { "card-drag": cardDrag, "press-result": pressResult, "cold-start": coldStart, "frame-scroll": frameScroll, "right-click": rightClick, "right-spam": rightSpam, "app-page": appPage, "list-return": listReturn, "rows-still": rowsStill, "find-meet": findMeet, "find-ladder": findLadder, "scroller-tab": scrollerTab, "pane-esc": paneEsc, "field-in-view": fieldInView, "press-holds": pressHolds, "nav-reopen": navReopen, "nav-place": navPlace, "nav-docs": navDocs, "tree-walk": treeWalk, "tree-click": treeClick, "gear-return": gearReturn, "walk-place": walkPlace, "scroller-keys": scrollerKeys, "scroller-select": scrollerSelect, "pane-keys": paneKeys, "comment-view": commentView, "nav-return": navReturn, "hold-to-undo": holdToUndo, "hold-gutter": holdGutter, "hold-leave": holdLeave, "find-key": findKey, "add-source-fold": addSourceFold, "strip-press": stripPress, "rail-top": railTop, "aimed-after-close": aimedAfterClose, "enter-twice": enterTwice, "leave-back": leaveBack, "esc-sweep": escSweep, "enter-fast": enterFast, "enter-walks": enterWalks, "pointer-twice": pointerTwice, "undo-stale": undoStale, "column-filter": columnFilter, "row-room": rowRoom, "walk-done": walkDone, "dialog-typing": dialogTyping, "kept-opening": keptOpening, "strip-walk": stripWalk };
  return { run: (t) => KINDS[t.kind](t), stop };
}

const queue = every.filter(wanted).filter((_, at) => at % partM === partK - 1);
if (!queue.length) {
  console.error(`no trip matches --only ${only.join(",")}${table.some((t) => t.strict && wanted(t)) ? ". Some strict ones do: add --strict" : ""}`);
  process.exit(2);
}
const total = queue.length;
log(`${total} trips with ${workers} workers, each trip at most ${tripLimit} s`);
const leftOut = strict ? 0 : table.filter((t) => t.strict && wanted(t)).length;
if (leftOut) log(`${leftOut} strict trips left out. --strict runs them, and they fail until Round 14's fixes are in`);
const results = [];
const startedAt = Date.now();
await runQueue({ queue, results, workers, limitMs: tripLimit * 1000, startBrowser, log });
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
log(`${total} trips in ${took.toFixed(0)} s: ${fails.length} failing, ${cant.length} can't run, ${passes.length} passing. See ${join(outDir, "trips.md")}`);
process.exit(fails.length || cant.length ? 1 : 0);
