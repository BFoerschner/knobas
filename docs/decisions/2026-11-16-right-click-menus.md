# 2026-11-16: Right-click menus of knobas's own, on every kind of thing

**Asked (Björn, on PR #2479):** "did, somewhere, we check if rightclick-options make sense?" And, the same day: "on rightclick maybe make open in... aswell for files, or also folders. especially for folders that have the open in... as if i was using open in... on a repo".

Nobody had. The app had no `contextmenu` handler anywhere in `app/src`, so every right-click showed the web view's own menu.

## What a right-click showed

Read from what each web view draws by default (WebView2's `CoreWebView2ContextMenuItem` kinds; WebKit's default menu on macOS) and from the app's code, since headless Chrome draws no native menu. Release builds have no *Inspect*: knobas enables no `devtools` feature, so devtools are a debug build's only.

| Thing | WebView2 (Windows) | WKWebView (macOS) | What makes sense |
|---|---|---|---|
| The page, a heading, a nav entry, empty space | Back, Refresh, Save as, Print (Share, Read aloud on some versions); Inspect in a debug build | Reload; Inspect Element in a debug build | Nothing. *Refresh* reloads the whole app and drops every kept page; *Save as* saves a hash address; *Back* walks the app's history behind its back. |
| A row that is a link (a ticket in a list, a pull request, a repository, a commit, a run, a Docs page, a file in the tree) | Open link in new window, Copy link, Inspect | Open Link, Open Link in New Window, Download Linked File, Copy Link, Share… | The thing's own actions. *Open link in new window* opens a bare WebView window with no shell around it; *Copy link* copies `#/…`, which nothing outside the app opens. |
| A row that is no link (a ticket card on the board, a branch, a monitor, a test, an Inbox line, a tree folder, a diff's file) | The page's menu | The page's menu | The thing's own actions. |
| A link to the web in synced text | Open link in new window, Copy link | Open Link, Open Link in New Window, Copy Link | Open in the browser, copy it, and open it in knobas when knobas mirrors that source. |
| Selected words | Copy, Search the web for "…", Print | Look Up, Translate, Search with Google, Copy, Share, Speech, Services | Copy, and search knobas for them. |
| A field (input, text area) | Emoji, Undo, Redo, Cut, Copy, Paste, Paste as plain text, Select all, spelling | Spelling and Grammar, Substitutions, Transformations, Speech, Cut, Copy, Paste, AutoFill | The platform's own: its spelling, emoji and paste are the point, and none of its items harms the app. |
| An image | Save image as, Copy image, Copy image link | Open Image in New Window, Download Image, Copy Image | Nothing yet (see below). |
| A frame knobas draws (a Docs embed, a renderer's output) | The page's menu, in the frame | Reload, in the frame | Owed on Windows: both frames are sandboxed without same-origin, so the app can't listen inside them. |

Tauri 2 offers no setting to turn WebView2's default menu off (that is `AreDefaultContextMenusEnabled` on the platform webview, which would take the fields' spelling with it), and a native popup menu (`Menu.popup()`) draws in the system's look, not knobas's. `preventDefault()` on the document's `contextmenu` stops the default menu on both web views, and that is the one switch used.

## The ruling

The judgment calls went to Fable (xhigh), whose rulings are followed here.

- **One menu for the window**, `tf/context/TfContextMenu.svelte`, drawn in `TfMenu` (still the one `role="menu"`) at the pointer, kept inside the window at every size and scale, in the float host. Installed once by the shell.
- **The web view's own menu stays in a field only**: an input that takes typing, a text area, a select, a contenteditable. A checkbox's is knobas's. In a development build Shift+right-click falls through too, so *Inspect* stays reachable; a release has no such door.
- **Nothing on nothing.** The page, a heading of no thing, a nav entry: the web view's menu is stopped and nothing is drawn. Back, Search and Sync have keys, and a quiet right-click is how a desktop app behaves.
- **What it is on is read from the page, never from the backend** (`thing.ts`). A row's link names it (`#/ticket/…`, `#/repo/<id>/files/<path>`, `#/build/<pipeline>/<run>`); a table's row is the thing its first link names, so a button in the actions' cell gives the row's menu; a row that is no link carries a tag (`menuThing`); an Inbox line's tag claims its whole line, links included; a tree folder, a file view's line and a repository image are read from where they stand on a repository's page; a page's heading is the page's thing, with no *Open*.
- **It opens in the frame it is asked for**, with no read first. As it opens it starts the reads its items may want (the item's head for its title and page, the repository's checkout, the file's page on the forge) and waits for none. A read only changes a label (*Open in Jira*, *Clone* in the default application's slot), never how many rows there are or their order. A copy whose text is still being read starts its write inside the press with the text to come (`ClipboardItem`), as WebKit asks.
- **The order is the same on every kind**: *Open*; its pages (*Files changed*, *Log*); what it does (*Work on this*, *Start timer*, *Run*, *Re-run*); *Link to ⟨job⟩*; a rule; *Open in ⟨default application⟩*, *Open in… ▸*, *Open in ⟨source⟩*; a rule; the copies, each showing on its right what it copies. *Open* is first, so the release of the press that opened the menu never lands on a write.
- **One table** (`items.ts`), built on the launcher sheet's own actions (`sheet.ts`) by their ids and labels, one-press ones only. A flow (*Merge…*, *Create pull request…*, *Add to a set…*, *Snooze…*) stays where it asks. *Approve* and *Merge* stay off: others see them and one press doesn't take them back. *Favourite* stays off too: its label needs the launcher's read of the favourites, and an open menu doesn't change its rows.
- **Writes are in where a wrong press is cheap**: *Run*, *Re-run* (queued, with the app's toast), *Link to ⟨job⟩* (the default relation, *related*), *Acknowledge* only where the row shows an open alert (a monitor's goes through the row's own *Ack*, so *Acked* and its *Undo* follow), an Inbox line's *Done*.
- **Open in…, for code things** (Björn's addition). *Open in ⟨default application⟩* is flat, so the daily one is one press. *Open in… ▸* lists every application that opens the thing, then *Add an application* and *Manage applications*, as every *Open in…* ends; it opens on a resting pointer at once, on →, Enter or a click, and ← or Esc goes back. No *Browser* in it: *Open in ⟨source⟩* is the next row. Not cloned: *Clone ⟨repository⟩* stands in the default's slot and *Open in…* is greyed with why.
  - **A folder** opens in each application exactly as its repository does: the application's own command with the folder in the checkout's place, so `{path}`, `{file}` and `{name}` are the folder's. A new command, `open_folder`, beside `open_object` through the same runner; the folder must be inside the checkout on disk by its words and where it really is, and never git's own.
  - **A file** opens with the application's file command. **An application with no file command opens the file's folder**, as it opens a folder, and the checkout for a file at its top. One application does one thing to a file wherever it is pressed, so the file view's own *Open in…* changed to match: it used to open the checkout's top, and its line now says the folder.
- **Keyboard**: the context menu key and Shift+F10 open the same menu on the focused thing, under it, with its first row focused (a pointer's opening focuses the menu, so ↓ starts the walk). ↑ ↓ Home End move. Esc, a press anywhere else (the row too), a scroll and the window losing focus close it, and focus goes back to what had it. The row keeps its hover look while the menu stands.
- **Selected words**, when the press is inside the selection: *Copy* (the document's own copy, so a page's `copy` handler still shapes what lands, the file view's lines) and *Search knobas for "…"* for a line of 80 characters or fewer, then the thing's own below a rule. WebKit selects the word under a right press before `contextmenu` fires, so the selection that counts is the one when the press went down, and the word WebKit picked is put back.
- **A web link**: *Open in knobas* first when it starts with a configured source's base URL (resolved when chosen; a miss says so and offers the browser), *Open in browser*, *Copy link address*.
- **Images: nothing yet.** The renderer loads images as `data:` and `blob:`, so an image's address is rarely a web page; what a person wants is *Copy image*, which needs a PNG `ClipboardItem` the app doesn't build yet. An image that is a file of the repository gets the file's menu.

## What each kind offers

| Thing | Menu |
|---|---|
| Ticket (a card, a list row, a link, the ticket page's heading) | Open, Work on this, Start timer, Link to ⟨job⟩ · Open in Jira · Copy key, Copy key and title, Copy link |
| Pull request | Open, Review the files, Conversation, Link to ⟨job⟩ · Open in Gitea · Copy title, Copy link |
| Repository | Open, Pull requests, Branches, Commits · Open in ⟨default⟩, Open in… ▸, Open in Gitea · Copy name, Copy full path, Copy link |
| Branch (its page's link) | Open · Open in ⟨default⟩, Open in… ▸, Open in Gitea · Copy name, Copy link |
| Branch (a row of Branches) | Compare, Builds · Copy name |
| Folder (the tree) | Open in ⟨default⟩ or Clone, Open in… ▸, Open in browser · Copy path, Copy full path, Copy link |
| File (the tree, a link, a diff's header or list) | Open, History · Open in ⟨default⟩ or Clone, Open in… ▸, Open in browser · Copy path, Copy full path, Copy link |
| A line of the file view | Open, History · Open in ⟨default⟩ at the line, Open in… ▸, Open in browser · Copy path and line, Copy full path, Copy link to line |
| Commit | Open · Open in ⟨an application with a commit command⟩, Open in… ▸, Open in Gitea · Copy hash, Copy short hash, Copy link |
| Run | Open, Log, Tests, Re-run · Open in TeamCity · Copy link |
| Pipeline | Open, Run · Open in TeamCity · Copy name, Copy link |
| Monitor | Open, Acknowledge (open alert only) · Open in Uptime Kuma · Copy name, Copy link |
| Docs page, asset | Open · Open in ⟨source⟩ · Copy title or name, Copy link |
| Inbox line | Open, Re-run, Acknowledge, Done (as the line offers them) |
| Test | Copy name |
| Web link | Open in knobas (a mirrored source), Open in browser · Copy link address |
| Selected words | Copy, Search knobas for "…" · then the thing's menu |
| A field | The web view's own |
| Anything else | Nothing |

## Tested

- Vitest: `context/thing.test.ts` (every address, a tag, a row's first link, a claiming tag, a folder, a line, a repository image, a heading, a field, the selection), `context/items.test.ts` (each kind's rows and order, no *Approve* or *Merge*, not cloned, the read changes labels only, the commit's applications, the web link, the words), `context/TfContextMenu.test.svelte.ts` (opens at the pointer in the same frame and stops the web view's, a field keeps it, nothing on nothing, the development build's Shift, the keys, Shift+F10 and the context menu key, every way it closes, a right-click inside it, each copy and what it reads, *Open in… ▸* by → and by a resting pointer, a folder's and a file's application, not cloned, the words, a monitor's own *Ack*, a web link), `floats.test.ts` (a point and a submenu at the window's edges, a window smaller than the menu), `TfOpenIn.test.svelte.ts` (a file's application with no file command opens its folder). Each of 39 mutants of the new code fails a test.
- Rust: `checkout_ipc.rs`, a folder opens with the repository's command at the folder (a space in its name is one argument), a folder outside the checkout (by `..`, a root, a symlink out, a file, one not there, the checkout itself, `.git`) runs nothing, and a repository with no clone refuses it by name.
- Press trips (`right-click`): a real right press on a ticket card, a ticket by Shift+F10, a field, a repository row, a tree folder (and *Open in… › Terminal* by pointer), a tree file, a commit, an Inbox line, selected words (and the launcher with them), a heading of nothing, a monitor, a run's heading.

- Timing, on the Mac, against the windowless backend on a copy of the saved scale database: `testenv/perf/right-click.mjs`, a real right press on a ticket card, an Inbox line, a monitor row, a repository row and a Docs tree page, 7 each, from the press to the end of the frame that draws the menu: medians 21 to 38 ms, worst 40 ms, at 1x, 4x and 6x, at 1440x868@1 and 2560x1440@1.25 (the Mac's lines are 50, 50 and 100 ms). `just open-budget --real` on the pages whose rows took a tag (Inbox, the Tickets board, Monitors, Code): every open within its line at both sizes; worst useful frame 34, 68 and 100 ms at 1440x868@1, 82 and 112 ms at 2560x1440@1.25 at 4x and 6x.

## Owed on Windows

WebView2's own menu: that `preventDefault()` stops it on the page, a link and selected words in the installed app; that a field's is the platform's with spelling; what a Docs embed or a renderer's frame shows inside it (no *Refresh* or *Back* that reloads the app); the context menu key on a Windows keyboard, which WebView2 also answers with a `contextmenu` of its own.

## Reversing it

Take `TfContextMenu` out of `TfShell` and the web view's menu comes back everywhere. The `open_folder` command, `TfMenu`'s point anchor and the file view's folder rule stand on their own.
