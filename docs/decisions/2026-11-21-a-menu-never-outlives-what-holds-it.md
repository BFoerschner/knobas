# 2026-11-21: A menu never outlives what holds it

**Found.** Björn: "When I spam right click, I can make it to the right click, context menu stays open indefinitely and
only is closeable when I close knobas and open it again." On the Code page, and he asked for every page to be checked
and for the cause, not the symptom, to be fixed.

What stayed was the right-click menu's *Open in… ▸*, the submenu of applications. Reproduced three ways before the fix:

- In the app's own window on the Mac (WKWebView, the debug app on the mock corpus, real CGEvent input through
  `testenv/ui-checks/real-window.sh`, whose `rw-events` now sends right presses: `rclick`, `rspam`): right-click a folder,
  rest the pointer on *Open in…*, switch to Finder. The menu closed and *Open in…* stayed. No left press, `Esc` or move
  to another page closed it afterwards.
- In headless Chrome (the press trips' new `right-spam` kind, on the WSL box): real right presses 10 to 50 ms apart with
  the hand drifting over the menu. The spam alone left submenus behind, up to four at once (`rs-repo-blur`: "blur left 4
  of 5 menus standing"), and so did a left press away and a move to another address while *Open in…* stood. 5 of the 12
  spam trips failed.
- In a unit test with the shell's float host present: the window losing focus, or a press that went down on the menu and
  came up on another thing (WebView2 sends `contextmenu` as the button comes up), left the submenu in the host.

**Cause.** A menu draws in the float host at the shell's root (`floats.ts`), so its node is moved out of the run of nodes
its owner holds. When the owner goes while the menu stands, Svelte removes the owner's run and destroys what is nested
in it without touching the DOM (`destroy_effect` with `remove_dom` false): it counts on the nested nodes being inside the
run. `inFloatHost` hands back the function that takes the node out of the host for exactly this, but `TfMenu`'s `place`
ignored it. The right-click menu's submenu is drawn under `{#if sub !== null}`, and closing the menu clears `sub` in an
effect. Whenever the menu closed before the submenu's own close had flushed, that block went first and took the
submenu's listeners, its layer and its focus handling with it, and left its node standing in the host, where nothing
could reach it. In a real browser that is the usual order: between two listeners of one press the browser runs
microtasks, so the menu's outside-press listener closes the menu, Svelte flushes, and the submenu's block is gone before
the submenu's own listener runs. A blur and a move to another address do the same. The jsdom tests dispatch every
listener in one go, and draw without a float host, so they never saw it. Any other `TfMenu` whose owner goes while it
stands (a page's menu as the address moves) had the same hole.

A second, smaller fault turned up in the guard: the layer stack (`layers.svelte.ts`) filtered its `$state.raw` array on
a pop. Read inside an effect's teardown during a flush, the signal can still give the value from before that flush's
writes, so a menu closing in the same flush as another opened took the new one's layer with it (and a push could bring
back a layer already gone).

**Fixed in the shared places, so every menu has it.**

- `TfMenu` owns its node to the end: `place` keeps `inFloatHost`'s remover and calls it in its teardown, so a menu
  leaves the host however it goes.
- One menu stands at a time: a menu opening closes every other `TfMenu` but its own submenu or the menu it belongs to.
- The layer stack keeps a plain array as the truth and only publishes it to the signal, so a push and a pop in one flush
  both hold.
- The right-click menu is a menu of its own per opening (`{#key shown}`), so one that opens while another stands
  replaces it, at its own point, with its own listeners. Before, it kept the first opening's place.

knobas opens no native menu of its own: WebView2's page menu in a field or a frame is drawn by WebView2, trimmed
synchronously in `webview_menu.rs` and `knobas-site`'s `win.rs`, and neither waits or reenters. Nothing there changed.

**Every right-click menu checked.** knobas has one, for the whole window (`context/TfContextMenu.svelte`), on every
page. The spam trips cover the Code page (a repository row, three ways to close), a repository's files (a folder, a
file), a pull request's commits, Tickets, Inbox, Monitors, a run and Docs: 12 of 12 pass after the fix, 5 failed before.
The other `TfMenu`s (*Open in…* buttons, *Render as*, a file's and a Docs page's *More*, the PR picker, Tickets' view
menu, State colours' chip, Builds' controls) share the fix through `TfMenu`.

**Guards.** `TfContextMenu.test.svelte.ts`: in the float host, the submenu goes with its menu on a blur and on a press
that came up elsewhere, `Esc` walks back a level at a time, and 60 seeded spam runs (real press orders for the Mac and
Windows, rests on rows, `Esc`, blur, address moves, with no gap, a microtask, a tick or a task between events) keep at
most a menu and its *Open in…*, after which one press away closes everything and empties the layer stack. 7 of these
failed before the fix. `TfMenu.test.svelte.ts`: a menu whose owner goes while it stands leaves no node and no layer,
also in one flush with a move to another address; one menu stands at a time; and every order of three steps (open,
open another, close, owner goes, owner comes back, address moves, press away, `Esc`), each flushed or not, ends with
what is drawn equal to what stands, never more than one, and a press away closing it. Mutants: without the remover 56
of these fail, without `{#key}` 1, without the one-at-a-time rule 1, with the old layer stack 1.
