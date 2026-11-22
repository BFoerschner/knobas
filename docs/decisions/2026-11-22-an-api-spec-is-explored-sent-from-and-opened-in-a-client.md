# 2026-11-22: An API spec is explored, sent from and opened in a client (ux31)

**Asked (Björn, on PR #2479):** "i'd like you to have a look on the swagger/openapi renderer, it doesn't look right. Use fable to redesign if necessary. It should be really nice to explore api's with it, possibly even sending requests out from there, that'd be incredible. Also, an open in bruno would be nice where it automatically creates a scratch folder, imports the openapi into bruno and then opens bruno pointed at that (if possible). But have it just as a normal open in application, since bruno might not be installed or someone else might just not use it or has something else."

The judgment calls went to Fable (xhigh) under Björn's standing instruction, and Opus built them. The architecture is [ADR-0057](../adr/0057-knobas-sends-a-request-a-person-composed-and-keeps-its-secret-for-one-origin.md); this records the rulings. It replaces two of [2026-11-17's](2026-11-17-an-api-spec-is-drawn-as-its-endpoints.md): "no *Try it out*" and "not listed under *Open in…*".

## What was there

One windowed list in the page's scroller: groups, operation rows, and each opened operation and schema in place under its row. On nine real specs: four did not open as a spec at all, their version key standing past the sniff's first 256 KB (Stripe's as JSON and YAML, Kubernetes', Jira Cloud's); TeamCity's first endpoint stood 2,788 px down under its description; GitHub's *List repository issues* opened to 3,754 px with nothing on screen naming it; at 2560 px half the window was empty; and Stripe's 612 untagged operations would have been one group. The worker read no security scheme, no server variable and no per-operation server. Nothing called a server, and a spec was offered to no application.

## The rulings

1. **The explorer fits the pane**, as a PDF does, in columns that each scroll on their own: a rail and a page under 1,480 px of width, a request column from there, an answer column from 2,500.
2. **The rail lists by tag, by path or by schema.** A spec of more than 60 operations opens with every fold shut, as its table of contents. Operations with no tag are grouped by the first part of their path that tells them apart. A row is the method and the path from its group's shared start on, and a long path keeps its first and last parts.
2a. **A spec is recognised wherever its version key stands**: past the sniff's head by one native search, with the worker's parse as the judge.
3. **With words typed the rail is flat and ranked**, by the launcher's rules, over a list the picker holds for the spec. ⌘F goes to the filter.
4. **An operation's page** shows one response and one media type at a time, picked in a segmented control. **A schema** is drawn merged (`allOf`), chosen among (`oneOf`, `anyOf`) and followed (a ref's name goes to the schema's page, which lists the operations that use it).
5. **The overview** draws the servers with their variables and the security schemes in words.
6. **A followed link pushes the window's history; a pick in the rail replaces the place.** The address is the file's with a place, as before, plus `tags/<name>`.
7. **Small, a spec is the inline form**: in a comparison the operations and schemas that differ, marked, on the stage's two sides; on a Docs page a code block's *API*, in a box of fixed height, the code first. Its filter is every word typed, not the launcher's ranking: a page may hold several forms, and each would hold a list in the picker. A Docs block whose version key lies past the sniff's head is not offered *API*.
8. **A request starts as the spec says** and is one model with three writers: what Rust sends, the curl text, and a Bruno request.
9. **A server is the spec's or one a person saved** (one settings row). No environments, no variables but a server's own.
10. **A secret is kept for one origin, in memory only** (Björn: "I'd just type it into the openapi viewer page itself. not using stored credentials"): typed on the explorer's page, handed to the app once, held there until knobas quits; never in the keychain or a settings row. Rust attaches it to its origin only. No source's credential is used.
11. **Rust sends once**: no retry, 10 s to connect, 60 s in all unless changed, 64 MiB of answer kept, `http` and `https` only.
12. **Nothing is sent unasked.** A method other than `GET`, `HEAD` and `OPTIONS` to a host that is not this computer is asked about in place, once a host until quit, and Rust refuses it without that yes. Redirects are followed only for `GET` and `HEAD` on the same origin, five at most. Certificates are the platform's, plus a CA file a saved server names. No switch skips verification.
13. **An answer's status takes a tone**, its body is drawn as Source draws a file, HTML is never rendered.
14. **Answers and what was sent are kept in memory only**, 50 lines and 64 MiB, and are in no backup or share export.
15. **An application may have an API spec command**, with `{file}`, `{collection}` and `{name}`. It runs on files knobas writes, so it needs no checkout.
16. **The collection is written by knobas** as `.bru` files from each operation's starting request, with no secret in it, and Bruno's own parser reads it back in the tests.
17. **The scratch folder** is in the profile, one a spec file, written again when the spec's content changes (compared by hash, Björn), else opened as it is, removed after 30 days unopened.
18. **Bruno ships as an entry.** On macOS it opens the collection. On Windows and Linux it cannot be pointed at a folder, and knobas says so with the path ready (*Open Collection*, Björn: yes). Not found on the computer, the entry is not drawn (Björn).
19. **A program that exits with an error within 1.5 s of its start is a failed open**, for every application.
20. **A comparison reads a YAML or JSON file's side to know it for a spec** (as built): a renderer that knows its files by content cannot be picked by a name alone, so the side the stage would read is read once as the card is planned, its first MiB sniffed. *Lines* stays the default. The marks come from the worker, by JSON with keys sorted and refs left as refs; a schema that changed marks the operations that reach it as *Changed through* the nearest changed schema.
21. **Zen mode** (Björn's ask 3, as built): *Zen* in the file header and `Z` outside a field, on a file drawn as Markdown, PDF, JAR, an image, an API spec, a diagram (Mermaid, D2, Graphviz) or a page. It hides the repository's crumbs, header, tabs and tree; the file's own header stays. `Esc` leaves it, leaving the file view ends it, it holds while the reader moves between files, and it is never kept. Its key and renderers live in `tf/zen.svelte.ts`, which the tooltip and Settings › Keyboard read.

## Not in this pass

OAuth flows. Variables, scripts, tests and a saved history. Cookies between sends, client certificates, files in a multipart body. A form built from a body's schema. Operations in the launcher. A breaking-change verdict. AsyncAPI and GraphQL.

## Measured

On a prototype with the app's stylesheets in headless Chrome on the WSL box, GitHub's spec of 1,281 operations, median ms at 1x / 4x / 6x at 2560 x 1440: a filter key 16 / 67 / 85, an operation's page from the rail 14 / 56 / 82, a ref followed 15 / 57 / 95, a fold of 200 16 / 38 / 53; at 1440 x 868 each is lower. GitHub's 14.3 MB parses in 20 to 45 ms. The collection writer made GitHub's 1,330 files in 29 ms and Bruno's parser read every request back. Bruno 4.3.0 opened a handed folder on macOS only while running, and never on Linux. 

## Björn's answers and asks (2026-11-22, after the design)

1. **Credentials**: typed on the page, memory only; never stored, never a source's own. The design's keychain item for a saved server is dropped; a saved server holds a name, an address and a CA file.
2. **History**: memory only, as designed.
3. **Fill the window** becomes **Zen mode**, key `z` (free on the Code page; the one place it is bound is the file view), a file-view feature for every renderer where it applies (Markdown, PDF, JAR, images, the explorer): it hides the repository's header and tree. A visible button with its key in the tooltip, a row in Settings › Keyboard.
4. **Bruno on Windows and Linux**: the *Open Collection* hint.
5. **Bruno not installed**: the entry is hidden.
6. **Scratch folders**: 30 days unopened, and rebuilt when the spec's content changed (by hash).
7. **The question before a change**: once a host until knobas restarts.
8. **Operations in the launcher**: no separate index. An Everything hit inside a spec opens the explorer at the operation or schema that holds the hit's line (`openapi-line.ts`, a scan of the text up to it).
9. **Schemas** (Fable ruled the treatment): the whole row toggles; arrow keys; Alt-click for everything below; *Expand all* / *Collapse all*; names, types and *required* in columns a list shares; indent guides; the words under the name in the second ink; the facts one quiet line.
10. **Examples**: the spec's own where present, else made from the schema everywhere (bodies, answers, parameters, a schema's page), deterministic, labelled *From the spec* or *Made from the schema*; realistic by `@faker-js/faker` (MIT, lazy, in the worker) and `randexp` (MIT) for patterns, chosen in the order example or default, enum or const, format, pattern, the property's name (any case style, German names too), the type with its limits.

## Built

Step by step on the branch `ux31-openapi` (section 32 of the design): the sniff past its head and the worker's model (1), the explorer (2), its filter by the launcher's rules over a held list (3), the timing (4), sending in Rust (5), the request and the answer (6), this note and ADR-0057 (7), *Open in* and Bruno (8), a spec in a comparison (10), a spec on a Docs page (11) and Zen mode (12). Step 9 (Bruno on Linux and Windows) waits for the PC.

## Reversing it

Each part stands alone. The explorer: `TfOpenApi` as it was at `87b4e568`, with `openapi.css`. Sending: the request column is drawn only where `api_send` is registered, so taking the command out takes the column out. *Open in*: remove `api_command` from the shipped Bruno entry and no menu offers a client. Zen mode: drop the button and the key in `TfRepoFiles` and the two classes; `tf/zen.svelte.ts` holds nothing else. A spec in a comparison: `OPENAPI.diff = false`, and the diff's sniff reads (`BY_CONTENT` in `TfDiffView`) stop with it. The settings row and the scratch folder can stay; nothing reads them.
