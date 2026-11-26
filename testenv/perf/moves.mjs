// Nothing moves once drawn (the nothing-moves pass of PR #2479, 2026-11-17; Björn: "Check if any other page moves after
// it's drawn. That's not good behavior."). Opens every place a person can open, by the press a person makes, on its
// first open after the app started and on its return, and records for 5 s after its first useful frame every row,
// heading, button, link, field, tab, option and the focused element that moves or changes size after it was drawn
// (`moves-probe.js`, the page half). What a running animation moves, and what input after the press moved, is left out.
//
//   node testenv/perf/moves.mjs --app <url> --out <file.json> [--throttle 1,4] [--size WxH@s] [--only a,b]
//                               [--rounds first,return] [--sync] [--px 2] [--ms 5000] [--check] [--attach <port>]
//
// --app    a built app: `?fake-ipc&fake-real=<port>` on the saved scale database (examples/headless_ipc.rs), or the
//          fixture (`?fake-ipc&fake-scale&fake-hooks&fake-late=…`, each read landing after the first draw)
// --sync   every source ends a run that changed something every 1.5 s while each place records (`sync:state`, as the
//          backend tells the window; on the fixture `__fake.endRuns()` and a grown corpus), so what a sync re-reads lands
//          while the place is in view
// --px     what counts as a move, in CSS pixels (2 by default: a fractional scale rounds a box by a pixel)
// --check  exit 1 when any place has a mover over `--px`
//
// A place with `tabs` is followed by each of its tabs (the shown page's `.tf-tabs` links), opened by their press.
//
// The highlighter-switch places (`hl-`, the themes follow-up, 2026-11-21): each code area opened, the highlighter changed on
// Settings › Appearance › Code colours by its radio (`before`, and another one in `beforeReturn`), and the area opened again
// by its address: its lines (`track`) must hold their boxes while the new engine's colours land. `hl-preview-` presses a radio
// with the preview in view (`once`: a second press of the same radio changes nothing, so there is no return).
import { writeFileSync, readFileSync } from "node:fs";
import { attach, failedLine, isStalled, launch, runCase, sizeFrom, unlessStalled, viewport, sleep } from "./cdp.mjs";

const args = process.argv.slice(2);
const size = sizeFrom(args);
const opt = (name, fallback = null) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : fallback; };
const app = opt("--app");
const out = opt("--out") ?? "moves.json";
const throttles = (opt("--throttle") ?? "1").split(",").map(Number);
const only = opt("--only")?.split(",") ?? null;
const skip = opt("--skip")?.split(",") ?? [];
const rounds = (opt("--rounds") ?? "first,return").split(",");
const sync = args.includes("--sync");
const px = Number(opt("--px", "2"));
const ms = Number(opt("--ms", "5000"));
const check = args.includes("--check");
/** `--trace <selector>`: what each match's children are, each frame they change (finding a mover's cause). */
const trace = opt("--trace");
/** `--shard i/n`: every n-th place from the i-th, so n runs share the places. */
const shard = (opt("--shard") ?? "1/1").split("/").map(Number);
const firstAfter = Number(opt("--first-after", "3000"));
if (!app) { console.log("moves.mjs: --app <url> is required"); process.exit(2); }
setTimeout(() => { console.log("watchdog: gave up"); process.exit(3); }, Number(opt("--max-minutes", "28")) * 60000).unref();
const real = /[?&]fake-real=/.test(app);
const base = app.replace(/#.*$/, "");

const shown = ".tf-kept:not([hidden])";
/** A To Do ticket of the fixture's mock source, whose workflow offers In Progress (`fake/entity.ts`). */
const MOVE_TICKET = "#/ticket/mock%3APAY-240";
/** The Tickets page's List switch, pressed. */
const TO_LIST = `(() => { const b = [...document.querySelectorAll('${shown} [aria-label="Layout"] button')].find((x) => x.textContent === "List"); b?.click(); return !!b; })()`;
/** The first To Do card, focused, for `m`. */
const SELECT_TODO = `(() => { const col = [...document.querySelectorAll('${shown} .tf-col[data-place]')].find((c) => c.dataset.place.endsWith("To Do")); const n = col?.querySelector(".tf-tcard"); n?.focus(); return !!n; })()`;
const nav = (section) => `.tf-nav a[data-section="${section}"], .tf-nav a[href="#/${section}"], .tf-nav a[href^="#/${section}/"]`;
const GEAR = `a.tf-iconbtn[aria-label="Settings"]`;
const ESC = { key: "Escape", code: "Escape", windowsVirtualKeyCode: 27 };
const CMD_K = { key: "k", code: "KeyK", modifiers: process.platform === "darwin" ? 4 : 2, windowsVirtualKeyCode: 75 };
/** A repository with a checkout and pipelines on each corpus. */
const REPO = real ? "live-gitea%3Ascale-data%2Fbi-dashboards" : "mock%3Atidewater%2Fpayout-service";
const SPACE = real ? "SCLENG" : "ENG";
const SETTINGS = ["sources", "checkouts", "time", "notifications", "standup", "monitoring", "docs", "search", "notes", "backup", "appearance", "performance", "renderers", "keyboard", "logs", "updates"];

/**
 * The places. `start`: the address a first open starts at; one of `nav` (a section's entry), `gear`, `press` (the first
 * visible match, `text` its words), `right` (a right press), `key` (a key, from anywhere), `go` (the address a link in the
 * page would set); `hash`: where it lands (a prefix, null for an overlay); `ready`: what says an overlay is drawn;
 * `away`: how a return leaves first (a section, `"Escape"`, an address, or `"clear"`: the field `type` typed into, emptied);
 * `tabs`: follow each of its tabs. `type`: a key typed into a field (`into`), as real key input; with `still`, what the key
 * must leave as it was until its answer draws, and `changes`, what says the answer drew (its words change), the place is
 * drawn at the answer's frame, and anything of `still` on screen that changes before it is a move (the ux19 overs pass).
 * `late`: on the fixture only, reads of the place's own that land later than the corpus's (`fake-late`'s form, added to
 * the address's), and `fake`, more of the fixture's switches (`fake-jar=oldjava`): a read the real backend answers
 * after the page draws, which the fixture answers at once (the ux19 gate, 2026-11-19). `forget`: `localStorage` keys
 * removed before the first open (the image and diagram sizes kept from an earlier place or run), so it is the first ever.
 * `scroll`: a scroller pressed with its end in view (the asset pane at 1440x868, whose lower sections are below the
 * window otherwise). `store`: `localStorage` keys set before the first open (the launcher's scope). `setupKey`: a key
 * pressed before each open, as `setup` presses. `fill`: words put into a field (`into`) at once, before the place's `type`
 * key is pressed: that key's answer is the one recorded, the words before it answered unrecorded.
 */
/** Where a run's answer lands and what says it did: the tree's generated rows (`answer` above). */
const RUN_ANSWER = { in: `${shown} .tf-ftree`, changes: `${shown} .tf-ftree a[data-made]`, hold: '.tf-fdir[data-path="docs"], .tf-fband' };

/**
 * A folder's generated compare's answer in a comparison (ux26): the cards column and the rail take what it brings anew in
 * its frame (the rows under the pressed folder's group move down for its rows, by design), the pressed folder's bar and
 * its row and band in the rail held to their boxes; `changes` says the answer drew.
 */
const FOLDER_ANSWER = (changes) => ({ in: `${shown} .tf-dgrid`, changes, hold: '.tf-dfolder[data-folder="docs"] .tf-dfbar, .tf-dgdir[data-folder="docs"], .tf-dgband[data-folder="docs"]' });

/** Settings › Appearance › Code colours' radio for an engine, pressed by its words. */
const CODE_COLOURS = "#/settings/appearance/code-colours";
const RADIO = (name) => ({ press: `${shown} .tf-thm-eng label`, text: name, starts: true });
const ENGINE = { pierre: "Pierre Highlights", tanstack: "TanStack Highlight", shiki: "Shiki" };
/** What a code area's reader holds: a file's lines, a diff's lines, rows and hunks, a Docs block. */
const CODE_LINES = ".tf-l, .tf-ln, .tf-srow, .tf-hunk, .tf-pre pre, .tf-doc pre";
/**
 * A code area at `at` after a change of highlighter: opened, then Code colours, Shiki chosen (the slowest, its colours
 * following the text furthest), then the area again by its address; the return chooses TanStack Highlight the same way.
 */
const SWITCHED = (name, at, more = {}) => ({
  name: `hl-${name}`, fixture: true, start: at, before: [{ go: CODE_COLOURS }, RADIO(ENGINE.shiki)], beforeReturn: [RADIO(ENGINE.tanstack)],
  go: at, hash: at, away: CODE_COLOURS, track: CODE_LINES, ...more,
});
const COMMIT = `#/repo/${REPO}/commits/${REPO}%40c90d11e4a7b2`;
/**
 * Code's line spacing changed in Settings › Fonts, then a code area again by its address (ux29, 2026-11-21): *Open* pressed
 * before the first open, *Joined* before the return, so each open draws rows of the other height and nothing may move after.
 */
const FONTS = "#/settings/appearance/fonts";
const SPACING = (words) => ({ press: `${shown} .tf-fr-seg button`, text: words });
const SPACED = (name, at, more = {}) => ({
  name: `lines-${name}`, fixture: true, start: at, before: [{ go: FONTS }, SPACING("Open")], beforeReturn: [SPACING("Joined")],
  go: at, hash: at, away: FONTS, track: CODE_LINES, ...more,
});
const FILE = `#/repo/${REPO}/files/src/payout/retry/RetryQueue.java`;
/** The branch box's *Working tree* picked as its change event would: a scripted pick is no input, so it is a step, not the press. */
const CHOOSE_WORKING_TREE = `(() => { const box = document.querySelector("${shown} .tf-bsel select"); box.value = "~working-tree"; box.dispatchEvent(new Event("change", { bubbles: true })); })()`;
const HL_PLACES = [
  // Commits on the working tree (the themes follow-up, 2026-11-21): *Uncommitted changes* and the commit to push draw with
  // the forge's commits in one frame, the checkout's read landing 400 ms late, so none comes in above another.
  { name: "wt-commits", fixture: true, late: "entity_range_files:400", start: `#/repo/${REPO}`, before: [{ eval: CHOOSE_WORKING_TREE }], beforeReturn: [{ eval: CHOOSE_WORKING_TREE }], press: `${shown} .tf-tabs a[href$="/commits"]`, hash: `#/repo/${REPO}/commits`, away: `#/repo/${REPO}` },
  SWITCHED("file", `#/repo/${REPO}/files/src/payout/retry/RetryQueue.java`),
  ...["full", "quiet", "grey"].map((mode) => SWITCHED(`diff-${mode}`, COMMIT, { store: { "knobas.diff.unchanged": mode } })),
  SWITCHED("diff-split", COMMIT, { store: { "knobas.diff.layout": "split" } }),
  SWITCHED("pr", "#/pr/mock%3Apayout-service%23142/diff"),
  SWITCHED("compare", `#/repo/${REPO}/compare/~working-tree`),
  SWITCHED("docs-code", "#/page/confluence%3Asepa"),
  SPACED("file", FILE),
  SPACED("diff", COMMIT),
  SPACED("docs-code", "#/page/confluence%3Asepa"),
  // The release that is not the default kept (ux29; 2009.22 is the default since #2516, so the newer one is the kept
  // case): it loads as the file draws, at the default's metrics, so nothing moves when it lands.
  { name: "newer-cascadia-file", fixture: true, store: { "knobas.code.release": "2407.24" }, start: "#/time", go: FILE, hash: FILE, away: "#/time", track: CODE_LINES },
  ...["tanstack", "shiki", "pierre"].map((engine) => ({
    name: `hl-preview-${engine}`, fixture: true, once: true, start: CODE_COLOURS, ...(engine === "pierre" ? { store: { "knobas.highlighter": "shiki" } } : {}),
    ...RADIO(ENGINE[engine]), hash: null, ready: `${shown} input[name=tf-thm-engine][value=${engine}]:checked`, track: CODE_LINES, away: "#/settings",
  })),
];

/** D1's Docs page of XML, JSON and schema blocks, another page to open it from, and *Source* pressed on its block. */
const EDI_PAGE = "#/page/confluence%3Aedi-formats";
const OTHER_PAGE = "#/page/confluence%3Asepa";
const SOURCE = { press: `${shown} .tf-pre-bar button`, text: "Source" };
/** D1's ticket whose attachments are XML and JSON (`fake/corpus.ts`). */
const ATTACHED = "#/ticket/mock%3APAY-228";
/** The fixture's Jira-like ticket whose fields the dev fake answers (`fake/ticket-fields.ts`). */
const TICKET = "#/ticket/mock%3APAY-231";
const PLACES = [
  { name: "tickets", start: "#/time", nav: "tickets", hash: "#/tickets", away: "time", tabs: true },
  { name: "tickets-list", start: "#/tickets", press: `${shown} button, ${shown} [role=tab]`, text: "List", hash: "#/tickets", away: "time", returnBy: { nav: "tickets" } },
  // A card's press opens its ticket's page (Tickets has no preview since ux18).
  // A letter on the board (the ux19 overs pass): nothing on screen changes until its answer draws, nothing moves after.
  { name: "tickets-key", start: "#/tickets", type: { into: `${shown} input[placeholder="Filter tickets"]`, key: "e" }, still: `${shown} .tf-tcard`, changes: `${shown} .tf-col h3 .tf-count`, hash: "#/tickets", away: "clear" },
  { name: "ticket", start: "#/tickets", press: `${shown} .tf-tcard`, hash: "#/ticket/", away: "Escape", tabs: true },
  // A ticket's fields as controls (ADR-0054, 2026-11-22): the page moves nothing when its fields land late, when its
  // adapter offers no edits (the line under the facts is held with the sections) or when a credential is refused; and
  // each control opens over the page without moving it.
  ...[
    ["late", { late: "ticket_fields:400" }],
    ["none", { fake: "fake-noedits" }],
    ["readonly", { fake: "fake-readonly", late: "refused_writes:300" }],
  ].map(([how, more]) => ({ name: `ticket-fields-${how}`, fixture: true, ...more, start: "#/tickets", go: TICKET, hash: TICKET, away: "#/tickets" })),
  ...[
    ["assignee", `${shown} button[aria-label="Assignee: change it"]`, ".tf-fpick .tf-fpick-opt"],
    ["priority", `${shown} button[aria-label="Priority: change it"]`, "[role=menu] [role=menuitemradio]"],
    ["labels", `${shown} button[aria-label="Labels: change it"]`, ".tf-fpick .tf-fpick-opt"],
    ["title", `${shown} .tf-tktitlepen`, `${shown} .tf-tktitleedit input`],
    // The summary's field takes two Escs, the first leaving the field (`inlineEditor`): its `hash` asks `leave` for the second.
  ].map(([field, press, ready]) => ({ name: `ticket-edit-${field}`, fixture: true, start: TICKET, press, hash: field === "title" ? TICKET : null, ready, away: "Escape" })),
  { name: "inbox", start: "#/time", nav: "inbox", hash: "#/inbox", away: "time", tabs: true },
  { name: "code", start: "#/time", nav: "code", hash: "#/code", away: "time", tabs: true },
  { name: "repo", start: "#/code", press: `${shown} a[href^="#/repo/"]`, hash: "#/repo/", away: "Escape", tabs: true },
  // Compare pressed once the landing has been looked at (its tab above, built ahead while the reader is still), and
  // pressed 300 ms after the landing's tree drew, before it is built ahead (Fable's ruling, the compare-first-open pass).
  { name: "repo-compare", start: `#/repo/${REPO}`, press: `${shown} .tf-tabs a[href="#/repo/${REPO}/compare"]`, hash: `#/repo/${REPO}/compare`, away: `#/repo/${REPO}` },
  // The fixture's mirror lists no branch's pull request by default, so only the branch's standing, read 350 ms late here,
  // says #142 is open from the branch Compare proposes, and the form shrank from *New pull request* to *Open PR #142*, on
  // the code before this pass too. With `fake-branch-pulls` the mirror answers as the real one does.
  { name: "repo-compare-soon", fixture: true, fake: "fake-branch-pulls", start: `#/repo/${REPO}`, soon: { drawn: `${shown} .tf-ftree .tf-d0`, ms: 300 }, press: `${shown} .tf-tabs a[href="#/repo/${REPO}/compare"]`, hash: `#/repo/${REPO}/compare`, away: `#/repo/${REPO}` },
  // The checkout read after the README drew (the ux21 pass): *Clone* landed beside *Render as* and *Pipeline* and pushed
  // them 59 px on the live suite at 4x. The views at once and the checkout within the header's 500 ms hold, so the header
  // draws with it as it does there. `ledger`: not cloned, and a README with no image (an image's first load moves the
  // text under it, until its size is kept: `tf/markdown/image-sizes.ts`).
  { name: "repo-checkout-late", fixture: true, late: "entity_view:0,entity_checkout:450", start: "#/code", go: "#/repo/mock%3Atidewater%2Fledger", hash: "#/repo/mock%3Atidewater%2Fledger", away: "Escape" },
  // A README whose images were never seen (the ux21 README-image pass, 2026-11-19): `payout-service`'s logo pushed the
  // text under it 15 px down when its bytes loaded after the text. The bytes 150 ms after the views, and no size kept.
  // Its two Mermaid fences were never seen either (#2510): their drawings moved the fences under them 53 px at 2560 and
  // 5120 wide. The app warmed Mermaid while it stood idle before the press, so what stands under each is held for its
  // drawing, and one that lands after the hold keeps its source's room (`docs/decisions/2026-11-20-a-diagram-keeps-its-room.md`).
  { name: "repo-readme-image", fixture: true, late: "entity_view:0,entity_file_bytes:150", forget: ["knobas.image-sizes", "knobas.diagram-sizes"], start: "#/code", go: "#/repo/mock%3Atidewater%2Fpayout-service", hash: "#/repo/mock%3Atidewater%2Fpayout-service", away: "Escape" },
  // The same README with every drawing 900 ms late, past the hold, as a cold engine or a large diagram lands (Fable's
  // ruling of 2026-11-20): the page under each diagram draws with its source, and the drawing then stands inside the
  // source's room. A return to the page kept in the session gives each the same room again.
  { name: "repo-readme-diagram-late", fixture: true, late: "entity_view:0,entity_file_bytes:150", fake: "fake-diagram-late=900", forget: ["knobas.image-sizes", "knobas.diagram-sizes"], start: "#/code", go: "#/repo/mock%3Atidewater%2Fpayout-service", hash: "#/repo/mock%3Atidewater%2Fpayout-service", away: "Escape" },
  // A run's answer lands in one frame (2026-11-20, `docs/decisions/2026-11-20-a-runs-answer-lands-in-one-frame.md`): the
  // generated rows, in the tree; the folder's row and the band are held to their boxes in that frame too.
  // A folder's generated folder (ADR-0046, `?fake-rerender`'s *Low-code site* on `docs`): its glyph grafts the generated
  // rows in the folder's place under a band, the made bar over the pane; a generated file opens in the file view; *Run
  // again* swaps the tree in one frame. The bar is busy until the generated rows are in the tree.
  { name: "rerender-graft", fixture: true, fake: "fake-rerender", answer: RUN_ANSWER, start: `#/repo/${REPO}`, press: `${shown} .tf-fdir[data-path="docs"] + .tf-frmark a`, hash: `#/repo/${REPO}/rendered/`, away: `#/repo/${REPO}` },
  { name: "rerender-file", fixture: true, fake: "fake-rerender", start: `#/repo/${REPO}/rendered/site/docs`, press: `${shown} .tf-ftree a[data-made][href$="/files/page.html"]`, hash: `#/repo/${REPO}/rendered/site/docs/files/`, away: `#/repo/${REPO}/rendered/site/docs` },
  { name: "rerender-again", fixture: true, fake: "fake-rerender", answer: RUN_ANSWER, start: `#/repo/${REPO}/rendered/site/docs`, press: `${shown} .tf-madebar-run`, hash: `#/repo/${REPO}/rendered/`, away: `#/repo/${REPO}/rendered/site/docs` },
  // The same on the working tree (ux29; Björn: "when I have the working tree selected, it tries to render the folder from
  // the head and not from my local changes"): the glyph runs the renderer on the working tree on disk, the made bar saying
  // "from your working tree, N files changed" where it said the commit, and the run's answer still lands in one frame.
  // (Its *Run again* is not a place: the fake makes the working tree's lane the same files each run, as Compare needs.)
  { name: "wt-rerender-graft", fixture: true, fake: "fake-rerender", answer: RUN_ANSWER, start: `#/repo/${REPO}`, before: [{ eval: CHOOSE_WORKING_TREE }], beforeReturn: [{ eval: CHOOSE_WORKING_TREE }], press: `${shown} .tf-fdir[data-path="docs"] + .tf-frmark a`, hash: `#/repo/${REPO}/rendered/`, away: `#/repo/${REPO}` },
  // A folder renderer in a comparison (ux26, the decision list's sections 9 and 10): with *Rendered* on, Compare with
  // the working tree groups `docs` under its bar; *Run* runs both sides (each 1.5 s, so both land late), and their
  // answer (the generated cards, the rail's generated rows) lands in one frame with the bar and the folder's row and band
  // held. Then a side that fails (its stderr under the bar) and two sides that made the same (the note's words).
  { name: "folder-compare-answer", fixture: true, fake: "fake-rerender", store: { "knobas.diff-rendered": "1" }, answer: FOLDER_ANSWER(`${shown} .tf-dunder .tf-diff .tf-fh`), start: `#/repo/${REPO}/compare/~working-tree`, press: `${shown} .tf-dfolder .tf-madebar-run:not([disabled])`, hash: null, ready: `${shown} .tf-dfolder`, away: `#/repo/${REPO}/compare/~working-tree` },
  { name: "folder-compare-failed", fixture: true, fake: "fake-rerender&fake-rerender-fail", store: { "knobas.diff-rendered": "1" }, answer: FOLDER_ANSWER(`${shown} .tf-dffail`), start: `#/repo/${REPO}/compare/~working-tree`, press: `${shown} .tf-dfolder .tf-madebar-run:not([disabled])`, hash: null, ready: `${shown} .tf-dfolder`, away: `#/repo/${REPO}/compare/~working-tree` },
  { name: "folder-compare-same", fixture: true, fake: "fake-rerender&fake-rerender-same", store: { "knobas.diff-rendered": "1" }, answer: FOLDER_ANSWER(`${shown} .tf-dfnote`), start: `#/repo/${REPO}/compare/~working-tree`, press: `${shown} .tf-dfolder .tf-madebar-run:not([disabled])`, hash: null, ready: `${shown} .tf-dfolder`, away: `#/repo/${REPO}/compare/~working-tree` },
  // The schema explorer (2026-11-24): a set's files landing after its first frame, each read late, must not move a
  // row drawn; and an order checked against its schema, the marks landing on lines that stand.
  { name: "xsd-open", fixture: true, fake: "fake-xsd", late: "entity_file_bytes:300", start: "#/time", go: `#/repo/${REPO}/files/schemas/opentrans/opentrans_2_1.xsd`, hash: `#/repo/${REPO}/files/schemas/opentrans/`, away: "#/time" },
  { name: "xsd-ubl", fixture: true, fake: "fake-xsd", late: "entity_file_bytes:300", start: "#/time", go: `#/repo/${REPO}/files/ubl/xsd/maindoc/UBL-Invoice-2.1.xsd`, hash: `#/repo/${REPO}/files/ubl/`, away: "#/time" },
  // The example (S4): ORDER's page opened by its address, the example beside it from 1,480 px, made in the worker after
  // the page draws; its lines land in the box that holds their room.
  { name: "xsd-example", fixture: true, fake: "fake-xsd", late: "entity_file_bytes:300", start: "#/time", go: `#/repo/${REPO}/at/el%2FORDER/schemas/opentrans/opentrans_2_1.xsd`, hash: `#/repo/${REPO}/at/`, away: "#/time" },
  // The example opened in the XML view (E1), by its address: the explorer lands ORDER underneath, makes the example and
  // draws the whole view over it, checked; the header names the example in the room the schema's own name held.
  { name: "xsd-example-open", fixture: true, fake: "fake-xsd", late: "entity_file_bytes:300", start: "#/time", go: `#/repo/${REPO}/at/example%2FORDER/schemas/opentrans/opentrans_2_1.xsd`, hash: `#/repo/${REPO}/at/`, away: "#/time" },
  { name: "json-export", fixture: true, fake: "fake-xml", late: "entity_file_bytes:300", start: "#/time", go: `#/repo/${REPO}/files/exports/orders-2026-11.json`, hash: `#/repo/${REPO}/files/exports/`, away: "#/time" },
  // jq in the JSON view (J2): its answer 400 ms after the key, its marks and rows landing in the room they hold.
  { name: "json-jq-late", fixture: true, fake: "fake-xml", late: "json_query:400", start: `#/repo/${REPO}/files/exports/orders-2026-11.json`, fill: { into: ".tf-xv-qfield input", text: ".orders[] | select(.total > 2000) | .i" }, type: { into: ".tf-xv-qfield input", key: "d" }, hash: null, ready: `${shown} .tf-xv-rrow`, away: "clear" },
  { name: "xml-plain", fixture: true, fake: "fake-xml", late: "entity_file_bytes:300", start: "#/time", go: `#/repo/${REPO}/files/build/pom.xml`, hash: `#/repo/${REPO}/files/build/`, away: "#/time" },
  { name: "xml-checked", fixture: true, fake: "fake-xml&fake-xsd", late: "entity_file_bytes:300", start: "#/time", go: `#/repo/${REPO}/files/orders/order-4711.xml`, hash: `#/repo/${REPO}/files/orders/`, away: "#/time" },
  { name: "repo-file", optional: "real", start: `#/repo/${REPO}`, press: `${shown} .tf-ftree a[href*="/files/"]:not(.tf-fdir)`, hash: `#/repo/${REPO}/files/`, away: `#/repo/${REPO}` },
  { name: "repo-commit", optional: "real", start: `#/repo/${REPO}/commits`, press: `${shown} .tf-rcommits tbody tr`, hash: `#/repo/${REPO}/commits/`, away: `#/repo/${REPO}/commits` },
  // A rendered side that lands late (the stage pass of ux26, 2026-11-20): *Rendered* on the commit's README draws the
  // caption, the stage at its final height and the foot in the press's frame; each side's bytes 400 ms later land inside
  // the stage, so nothing outside it moves (before, a late side's `Reading…` gave way to the document and pushed every
  // card under it down).
  { name: "diff-rendered-late", fixture: true, fake: "fake-rendered-diff", late: "entity_file_bytes:400", start: `#/repo/${REPO}/commits/${REPO}%40c90d11e4a7b2`, press: `${shown} .tf-diff[data-file="README.md"] .tf-fh .tf-seg button`, text: "Rendered", hash: null, ready: `${shown} .tf-diff[data-file="README.md"] .tf-rstage`, away: `#/repo/${REPO}/commits/${REPO}%40c90d11e4a7b2` },
  // XML and JSON compared by element (C1): the order's list lands in the stage, which holds its height.
  { name: "diff-doc-rendered", fixture: true, fake: "fake-xml&fake-doc-diff", late: "entity_file_bytes:300", start: `#/repo/${REPO}/commits/${REPO}%40c90d11e4a7b2`, setup: `${shown} .tf-flist button[title="orders/order-4711.xml"]`, press: `${shown} .tf-diff[data-file="orders/order-4711.xml"] .tf-fh .tf-seg button`, text: "Rendered", hash: null, ready: `${shown} .tf-diff[data-file="orders/order-4711.xml"] .tf-xsd-crow`, away: `#/repo/${REPO}/commits/${REPO}%40c90d11e4a7b2` },
  // A schema compared by declaration (S7): Rendered on openTRANS's change reads both sets in the worker; the list lands
  // in the stage, which holds its height from the press's frame, so nothing outside it moves.
  { name: "diff-xsd-rendered", fixture: true, fake: "fake-xsd&fake-xsd-diff", late: "entity_file_bytes:300", start: `#/repo/${REPO}/commits/${REPO}%40c90d11e4a7b2`, press: `${shown} .tf-diff[data-file="schemas/opentrans/opentrans_2_1.xsd"] .tf-fh .tf-seg button`, text: "Rendered", hash: null, ready: `${shown} .tf-xsd-crow`, away: `#/repo/${REPO}/commits/${REPO}%40c90d11e4a7b2` },
  // Compare with the working tree as *From* (ux26): the form's short shape from the address, the names first (60 ms
  // late, the PC's 63 ms for 300 changed files) and the counts behind (300 ms late), filling the rows in place.
  { name: "repo-compare-working", fixture: true, late: "entity_range_names:60,entity_range_files:300", start: `#/repo/${REPO}`, go: `#/repo/${REPO}/compare/~working-tree`, hash: `#/repo/${REPO}/compare/`, away: `#/repo/${REPO}` },
  // Local changes' Markdown file pressed *Rendered* (ux26): the index and the working tree read 400 ms late, inside the stage.
  { name: "repo-local-rendered", fixture: true, late: "entity_file_bytes:400", start: `#/repo/${REPO}/local`, press: `${shown} .tf-diff[data-file="docs/retry.md (modified, not staged)"] .tf-fh .tf-seg button`, text: "Rendered", hash: null, ready: `${shown} .tf-rstage`, away: `#/repo/${REPO}/local` },
  { name: "open-in-menu", start: `#/repo/${REPO}`, press: `${shown} .tf-openin > button`, hash: null, ready: "[role=menu] [role=menuitem]", away: "Escape" },
  { name: "app-add", optional: "real", start: `#/repo/${REPO}`, setup: `${shown} .tf-openin > button`, press: "[role=menu] [role=menuitem]", text: "Add an application", hash: "#/settings/checkouts/add/", away: `#/repo/${REPO}` },
  { name: "pull-request", start: "#/code/prs", press: `${shown} a[href^="#/pr/"]`, hash: "#/pr/", away: "Escape", tabs: true },
  { name: "builds", start: "#/time", nav: "builds", hash: "#/builds", away: "time", tabs: true },
  { name: "pipeline", start: "#/builds", press: `${shown} .tf-rail a[href^="#/builds/"], ${shown} a[href^="#/builds/"]`, hash: "#/builds/", away: "Escape" },
  { name: "run", start: "#/builds", press: `${shown} a[href^="#/build/"]`, hash: "#/build/", away: "Escape", tabs: true },
  // Builds' *Recent* (2026-11-25): every pipeline's runs in one table, by its address; a run opened from it and Esc back.
  { name: "builds-recent", start: "#/time", go: "#/builds/recent", hash: "#/builds/recent", away: "#/time" },
  { name: "recent-run", start: "#/builds/recent", press: `${shown} .tf-recent a.tf-run-pno`, hash: "#/build/", away: "Escape" },
  { name: "docs", start: "#/time", nav: "docs", hash: "#/docs", away: "time" },
  { name: "docs-space", start: "#/docs", press: `${shown} a.tf-dh-spacelink[href="#/docs/space/${SPACE}"], ${shown} a.tf-dh-spacelink`, hash: "#/docs/space/", away: "Escape" },
  { name: "docs-page", start: `#/docs/space/${SPACE}`, press: `${shown} a.tf-dh-rowt[href^="#/page/"]`, hash: "#/page/", away: "Escape" },
  // A row of the rail's tree (2026-11-19): the page opens, and the rail it was pressed in holds its space.
  { name: "docs-tree", start: `#/docs/space/${SPACE}`, press: `${shown} .tf-rd-tree .tf-dl-tree a[href^="#/page/"]`, hash: "#/page/", away: "Escape" },
  // D1, a Docs page's XML, JSON and schema blocks (`fake-doc-blocks.ts`'s *EDI message formats*): the page opened from
  // another page, then each block's view pressed, drawn in the box the press's frame gave it; the return presses
  // *Source* first. A declaration opens in place in the schema's box, which holds its 420 px.
  { name: "docs-blocks", fixture: true, start: OTHER_PAGE, go: EDI_PAGE, hash: EDI_PAGE, away: OTHER_PAGE },
  // A page opened from another with its read late (Björn, 2026-11-25): the new page's frame is drawn at once, and its crumb
  // and title hold their boxes as the read lands under them.
  { name: "docs-page-next", fixture: true, late: "docs_page:400", start: OTHER_PAGE, go: EDI_PAGE, hash: EDI_PAGE, away: OTHER_PAGE },
  ...[
    ["schema", "Schema", `${shown} .tf-xsd-irow .tf-xsd-cname`],
    ["xml", "XML view", `${shown} .tf-doc-xv .tf-xv-caret`],
    ["json", "JSON view", `${shown} .tf-doc-xv .tf-xv-caret`],
  ].map(([which, word, ready]) => ({ name: `docs-blocks-${which}`, fixture: true, start: EDI_PAGE, press: `${shown} .tf-pre-bar button`, text: word, hash: null, ready, away: EDI_PAGE, beforeReturn: [SOURCE] })),
  { name: "docs-blocks-decl", fixture: true, start: EDI_PAGE, setup: `${shown} .tf-pre-bar button`, setupOnce: true, press: `${shown} .tf-xsd-ibtn`, hash: null, ready: `${shown} .tf-xsd-iopen .tf-xsd-h`, away: EDI_PAGE, beforeReturn: [{ press: `${shown} .tf-xsd-ibtn[aria-expanded="true"]` }] },
  // D1, the file viewer: PAY-228's junit.xml and JSON answer opened from their rows, their bytes read late, drawn by the
  // XML view's inline form in the stage's height.
  ...[
    ["junit", "junit.xml"],
    ["json", "psp-409-response.json"],
  ].map(([which, name]) => ({ name: `viewer-${which}`, fixture: true, late: "attachment_bytes:300", start: ATTACHED, setup: `${shown} section[aria-labelledby="tf-tkatt"] summary`, setupOnce: true, press: `${shown} .tf-tkatt .tf-r .tf-t`, text: name, starts: true, hash: null, ready: ".tf-vdoc .tf-xv-caret", away: "Escape" })),
  { name: "monitors", start: "#/time", nav: "monitors", hash: "#/monitors", away: "time", tabs: true },
  // A row opens its monitor beside the roster on the page, with no address of its own (the monitors-page pass, 2026-11-19).
  { name: "monitor", start: "#/monitors", press: `${shown} .tf-mon-row .tf-mon-state`, hash: null, ready: ".tf-mon-panel h2", away: "Escape" },
  { name: "sets", start: "#/time", nav: "sets", hash: "#/sets", away: "time" },
  // Passwords (ADR-0056; the ux30 end gate's Passwords fix, 2026-11-23): its first open and a return, on the clickdummy's
  // entries and at 20,000 (`pw-scale`), where a folder's rows are made and sorted as they draw and the filter's index
  // builds in idle slices after the first frame.
  { name: "passwords", start: "#/time", nav: "passwords", hash: "#/passwords", away: "time" },
  { name: "passwords-20k", fixture: true, fake: "pw-scale=20000", start: "#/time", nav: "passwords", hash: "#/passwords", away: "time" },
  { name: "time", start: "#/docs", nav: "time", hash: "#/time", away: "docs", tabs: true },
  // An estate of one root lands on that root, so the address can be an asset's; the tree's rows are buttons (2026-11-19:
  // `asset` had found nothing to press since its rows stopped being links).
  { name: "assets", optional: true, start: "#/time", go: "#/assets", hash: "#/asset", anywhere: true, away: "#/time" },
  // By its nav entry (2026-11-19): an estate of one root lands on that root, so the address is an asset's.
  { name: "assets-nav", optional: true, start: "#/time", nav: "assets", hash: "#/asset", anywhere: true, away: "time" },
  { name: "asset", optional: true, start: "#/assets", press: `${shown} ol.col button.row:not(.on)`, hash: "#/asset/", anywhere: true, away: "#/assets" },
  // *Depends on this* answered after the pane (the ux23 gate, 2026-11-20): its *Reading…* line gave way to the
  // *Everything inside it* fold, and *Linked items*, *History* and the import line under it moved 24 px down on the live
  // suite. `knobas-jira-db` holds one thing and nothing links to it (the gate's shape); `knobas-teamcity` holds one and
  // an agent depends on it (a row above the fold). The pane's own read at once, the panel's 250 ms after it. The pane
  // keeps its scroll across a press, so it is pressed scrolled to its end, where the panel is in view at 1440x868 too.
  // Drawn once the pressed asset's pane is (`ready`): the press's first frame can still hold the asset it came from,
  // which the next frame replaces (WSL's Chrome at 4x).
  { name: "asset-depends-late", fixture: true, scroll: `${shown} .pane`, late: "get_asset:0,depends_on_this:250", start: "#/asset/asset%3Awsl-docker", press: `${shown} ol.col button.row[data-asset="asset:knobas-jira-db"]`, hash: ["#/asset/asset%3Aknobas-jira-db", "#/asset/asset:knobas-jira-db"], ready: `${shown} .pane h2[title$="/ knobas-jira-db"]`, away: "#/asset/asset%3Awsl-docker" },
  { name: "asset-depends-links-late", fixture: true, scroll: `${shown} .pane`, late: "get_asset:0,depends_on_this:250", start: "#/asset/asset%3Awsl-docker", press: `${shown} ol.col button.row[data-asset="asset:knobas-teamcity"]`, hash: ["#/asset/asset%3Aknobas-teamcity", "#/asset/asset:knobas-teamcity"], ready: `${shown} .pane h2[title$="/ knobas-teamcity"]`, away: "#/asset/asset%3Awsl-docker" },
  { name: "settings", start: "#/time", gear: true, hash: "#/settings", away: "Escape" },
  ...SETTINGS.map((s) => ({ name: `settings-${s}`, start: "#/settings", press: `${shown} a[href="#/settings/${s}"]`, hash: `#/settings/${s}`, away: "#/settings" })),
  // Appearance's children (the subtrees ruling, 2026-11-21), pressed in the nav's tree.
  ...["theme", "code-colours", "fonts"].map((s) => ({ name: `settings-${s}`, start: "#/settings", press: `${shown} a[href="#/settings/appearance/${s}"]`, hash: `#/settings/appearance/${s}`, away: "#/settings" })),
  // Settings › Appearance › Theme (ADR-0055): a theme changing moves nothing. A copy taking the slot (a theme change of the
  // whole window), a code slot set apart, and Dracula drawn while dark by its kept slot. `once`: the press changes what it
  // pressed (an own copy offers no duplicate, a slot set apart no Set apart), so there is no return.
  ...[["theme-duplicate", "Duplicate and edit", ".tf-thm-edh button"], ["theme-set-apart", "Set apart", ".tf-apt-code button"]].map(([name, text, sel]) => ({
    name, once: true, start: "#/settings/appearance/theme", press: `${shown} ${sel}`, text, hash: "#/settings/appearance/theme", away: "#/settings",
  })),
  { name: "theme-dracula", fixture: true, store: { "knobas.app-theme.dark": "dracula", "knobas.theme.mode": "dark" }, start: "#/settings", press: `${shown} a[href="#/settings/appearance/theme"]`, hash: "#/settings/appearance/theme", away: "#/settings" },
  // Settings › Updates (ADR-0052): `update:state` lands while the page is drawn, an update found, a download ending, a
  // check failing. The box's rooms keep their size and place.
  ...[["found", "up-to-date", "available"], ["downloaded", "downloading", "ready"], ["check-failed", "checking", "check-failed"]].map(([what, first, then]) => ({
    name: `settings-updates-${what}`, fixture: true, fake: `fake-update=${first}&fake-update-then=${then}&fake-update-after=3500`,
    start: "#/settings", press: `${shown} a[href="#/settings/updates"]`, hash: "#/settings/updates", away: "#/settings",
  })),
  { name: "settings-source", start: "#/settings/sources", press: `${shown} a[href^="#/settings/source/"]`, hash: "#/settings/source/", away: "Escape" },
  { name: "app-edit", start: "#/settings/checkouts", press: `${shown} .tf-stg-apps a`, text: "Edit", hash: "#/settings/checkouts/app/", away: "Escape" },
  // From the list rather than Open in…, so it opens on every corpus: on the saved scale database no repository is cloned,
  // and Try it on's rows gave way to one line once the checkouts answered (2026-11-17).
  { name: "app-new", start: "#/settings/checkouts", press: `${shown} a`, text: "Add an application", hash: "#/settings/checkouts/add", away: "Escape" },
  // The Decompiler's Java read, past the 500 ms hold, with a reason to say: the ux19 gate found its path row moving on
  // the real backend, where the look for a Java answers after the page draws (2026-11-19).
  { name: "settings-renderers-java-late", fixture: true, late: "java_runtime:900", fake: "fake-jar=oldjava", start: "#/settings", press: `${shown} a[href="#/settings/renderers"]`, hash: "#/settings/renderers", away: "#/settings" },
  // Commands (ADR-0060): Settings › Commands and a command's page, and a run's dialog opened from a file's Run…, which
  // streams lines and ends while it records; nothing in it moves but the rows it adds.
  { name: "settings-commands", fixture: true, fake: "fake-oneoff", start: "#/settings", press: `${shown} a[href="#/settings/commands"]`, hash: "#/settings/commands", away: "#/settings" },
  { name: "command-page", fixture: true, fake: "fake-oneoff", start: "#/settings/commands", go: "#/settings/commands/eslint", hash: "#/settings/commands/eslint", away: "#/settings/commands" },
  { name: "run-dialog", fixture: true, fake: "fake-oneoff", start: `#/repo/${REPO}/files/src/main.rs`, before: [{ eval: `document.querySelector('.tf-fileview header .tf-oo-runin button').click()` }], beforeReturn: [{ eval: `document.querySelector('.tf-fileview header .tf-oo-runin button').click()` }], press: ".tf-oo-menu [role=menuitem]", text: "ESLint", starts: true, hash: null, ready: ".tf-oo-dialog .tf-oo-line", away: "Escape" },
  { name: "folder-renderer-new", start: "#/settings/renderers", press: `${shown} a`, text: "Add a folder renderer", hash: "#/settings/renderers/folder/add", away: "Escape" },
  { name: "launcher", start: "#/tickets", key: CMD_K, hash: null, ready: "[role=dialog] [role=option]", anywhere: true, away: "Escape" },
  // Everything's answer to words its rows' text holds (the everything-search decisions, D10): rows of one and two
  // lines, the evidence line drawn with the answer, and nothing moving after.
  { name: "launcher-everything", start: "#/tickets", store: { "knobas.launcher.scope": "everything" }, setupKey: CMD_K, fill: { into: "[role=dialog] input", text: real ? "partner even" : "retr" }, type: { into: "[role=dialog] input", key: real ? "t" : "y" }, hash: null, ready: "[role=dialog] .tf-ln-why", anywhere: true, away: "clear" },
  // A repository its files found, alone under `#r` (D11): the pane draws the answer's file at once and the repository's
  // other files under it when `search_files` lands, so nothing drawn moves.
  { name: "launcher-file-pane", start: "#/tickets", store: { "knobas.launcher.scope": "everything" }, setupKey: CMD_K, fill: { into: "[role=dialog] input", text: real ? "#r packetlin" : "#r settlebridg" }, type: { into: "[role=dialog] input", key: "e" }, hash: null, ready: "[role=dialog] .tf-ln-file", anywhere: true, away: "clear" },
  // A ticket's move (ADR-0054): its status menu, and a move in each state the queue gives it (the fixture's
  // `fake-move`), on the ticket page; the list's status menu; the board lit for a card carried by `s`. The moved ticket's own
  // chip and note come; nothing else may move. `once`: a moved ticket offers another status, so there is no return.
  { name: "ticket-status-menu", fixture: true, start: MOVE_TICKET, press: `${shown} .tf-statebtn`, hash: null, ready: "[role=menu] [role=menuitem]", away: "Escape" },
  { name: "ticket-move-queued", fixture: true, once: true, fake: "fake-move=slow", start: MOVE_TICKET, setup: `${shown} .tf-statebtn`, press: "[role=menu] [role=menuitem]", text: "In progress", starts: true, hash: null, ready: `${shown} .tf-swrite` },
  { name: "ticket-move-sent", fixture: true, once: true, start: MOVE_TICKET, setup: `${shown} .tf-statebtn`, press: "[role=menu] [role=menuitem]", text: "In progress", starts: true, hash: null, ready: `${shown} .tf-swrite` },
  { name: "ticket-move-refused", fixture: true, once: true, fake: "fake-move=refused", start: MOVE_TICKET, setup: `${shown} .tf-statebtn`, press: "[role=menu] [role=menuitem]", text: "In progress", starts: true, hash: null, ready: `${shown} .tf-fact-note` },
  { name: "ticket-move-held", fixture: true, once: true, fake: "fake-move=held", start: MOVE_TICKET, setup: `${shown} .tf-statebtn`, press: "[role=menu] [role=menuitem]", text: "In progress", starts: true, hash: null, ready: `${shown} .tf-fact-note` },
  { name: "list-status-menu", fixture: true, start: "#/tickets", before: [{ eval: TO_LIST }], press: `${shown} .tf-statecell`, hash: null, ready: "[role=menu] [role=menuitem]", away: "Escape" },
  { name: "board-carry", fixture: true, start: "#/tickets", before: [{ eval: SELECT_TODO }], key: { key: "s", code: "KeyS", windowsVirtualKeyCode: 83 }, hash: null, ready: ".tf-board-wrap[data-held]", anywhere: true, away: "Escape" },
  { name: "ticket-menu", start: "#/tickets", right: `${shown} .tf-tcard`, hash: null, ready: "[role=menu] [role=menuitem]", away: "Escape" },
  { name: "create-monitor", optional: true, start: "#/monitors/unmonitored", press: `${shown} button`, text: "Create monitor", starts: true, hash: null, ready: "[role=dialog]", away: "Escape" },
  { name: "link-dialog", start: "#/tickets", setup: `${shown} .tf-tcard`, setupOnce: true, press: `${shown} button`, text: "Link something to it", starts: true, hash: null, away: "Escape" },
  { name: "add-source", start: "#/settings/sources", press: `${shown} button, ${shown} a`, text: "Add a source", starts: true, hash: null, away: "Escape" },
  // A source's site (ADR-0050) on the dev bridge's fake: its page at rest, the view made and shown (the bar's address
  // arrives in place), the bar asking in its row, and the veil under the launcher. Nothing in the bar may move.
  { name: "site", fixture: true, start: "#/time", go: "#/site/confluence", hash: "#/site/confluence", away: "#/time" },
  { name: "site-open", fixture: true, start: "#/site/confluence", press: `${shown} .tf-veil .tf-primary`, hash: null, ready: ".tf-sitebar .tf-site-host", away: "#/time", returnBy: { go: "#/site/confluence", ready: null, hash: "#/site/confluence" } },
  { name: "site-asks", fixture: true, fake: "fake-site-dirty", start: "#/site/confluence", setup: `${shown} .tf-veil .tf-primary`, setupOnce: true, press: `${shown} .tf-sitebar button[aria-label^="Close"]`, hash: null, ready: ".tf-sitebar.tf-asks", away: "Escape" },
  { name: "site-veiled", fixture: true, start: "#/site/confluence", setup: `${shown} .tf-veil .tf-primary`, setupOnce: true, key: CMD_K, hash: null, ready: "[role=dialog] [role=option]", anywhere: true, away: "Escape" },
];
PLACES.push(...HL_PLACES);
const picked = (p) => !(p.fixture && real) && (!only || only.some((w) => p.name === w || p.name.startsWith(w))) && !skip.some((w) => p.name === w || p.name.startsWith(w));

/** Every source ends a run that changed something (`open.mjs`'s `SYNCED`); on the fixture, the corpus grows first. */
const SYNCED = real
  ? `(async () => { const all = await window.__TAURI_INTERNALS__.invoke("sync_status", {}); for (const s of all) window.__fake.emit("sync:state", { ...s, running: false, run_id: (s.run_id ?? 0) + 100000 + Math.floor(Math.random() * 100000), last_outcome: "ok", last_changed: true, last_finished_at: new Date().toISOString() }); return all.length; })()`
  : `(window.__fake?.endRuns?.(), 1)`;

// `--attach <port>`: a Chrome already listening (one on the WSL box through a tunnel, when the Mac's lock is taken).
const attachPort = opt("--attach");
const browser = attachPort ? { port: Number(attachPort), stop() {} } : await launch({ size });
const c = await attach(browser.port);
const js = (code, timeout = 60000) => c.js(code, { timeout });
await c.send("Page.enable");
await c.send("Runtime.enable");
await c.send("Page.addScriptToEvaluateOnNewDocument", { source: readFileSync(new URL("./moves-probe.js", import.meta.url), "utf8") });
await c.send("Emulation.setFocusEmulationEnabled", { enabled: true });
await c.send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-reduced-motion", value: "no-preference" }] });
await viewport(c, size);
const cpu = (rate) => c.send("Emulation.setCPUThrottlingRate", { rate });

async function quiet(q = 1200, limit = 20000) {
  await js(`new Promise((res) => { let last = performance.now(); const o = new MutationObserver(() => { last = performance.now(); }); o.observe(document.body, { subtree: true, childList: true, attributes: true, characterData: true }); const t0 = performance.now(); const tick = () => { if (performance.now() - last >= ${q} || performance.now() - t0 > ${limit}) { o.disconnect(); res(1); } else setTimeout(tick, 50); }; tick(); })`, limit + 5000);
}
/** The app's address for a place: its own late reads joined to the corpus's (a later entry for a read wins), its switches added. */
function addressFor(p) {
  if (!p?.late && !p?.fake) return base;
  const url = new URL(base);
  if (p.late) url.searchParams.set("fake-late", [url.searchParams.get("fake-late"), p.late].filter(Boolean).join(","));
  for (const [k, v] of new URLSearchParams(p.fake ?? "")) url.searchParams.set(k, v);
  // The fixture's switches are bare words (`?fake-ipc&fake-scale`), as `URLSearchParams` writes them with an `=`.
  return url.toString().replace(/=(?=&|$)/g, "");
}
/**
 * `hash` opened on a new page. Each place starts from nothing kept (`clear`): what an earlier place stored (an own theme
 * it duplicated, a highlighter, a moved ticket's queue) is not this place's start (the final suite, 2026-11-22:
 * theme-duplicate found an own copy in the editor, the ticket moves found the ticket moved). A place's own `store` is
 * written after the clear, and the reload that reads it keeps it.
 */
async function fresh(hash, p = null, clear = true) {
  const blank = c.once("Page.loadEventFired");
  await c.send("Page.navigate", { url: "about:blank" });
  await blank;
  if (clear) await c.send("Storage.clearDataForOrigin", { origin: new URL(base).origin, storageTypes: "local_storage,session_storage,indexeddb" });
  const loaded = c.once("Page.loadEventFired");
  await c.send("Page.navigate", { url: `${addressFor(p)}${hash}` });
  await loaded;
  for (let i = 0; i < 300; i++) { if (await js("!!document.querySelector('.tf-nav')").catch(unlessStalled(false))) break; await sleep(50); }
  if (p?.store) {
    await js(`(Object.entries(${JSON.stringify(p.store)}).forEach(([k, v]) => localStorage.setItem(k, v)), 1)`);
    return fresh(hash, { ...p, store: null }, false);
  }
  if (p?.forget?.length) {
    // Removed, then the app loaded again, so nothing it read at start-up holds what was removed.
    await js(`(${JSON.stringify(p.forget)}.forEach((k) => localStorage.removeItem(k)), 1)`);
    return fresh(hash, { ...p, forget: null }, false);
  }
  // `soon`: pressed this soon after what it names is drawn, not once the page has been quiet, so before anything a page
  // builds once the reader is still (Compare built ahead of its press, the compare-first-open pass, 2026-11-20).
  if (p?.soon) {
    for (let i = 0; i < 200; i++) { if (await js(`!!document.querySelector(${JSON.stringify(p.soon.drawn)})`).catch(unlessStalled(false))) break; await sleep(25); }
    await sleep(p.soon.ms);
    return;
  }
  await quiet(1500);
}
async function find(selector, text = null, starts = false) {
  const at = await js(`(() => { const want = ${JSON.stringify(text)}; const e = [...document.querySelectorAll(${JSON.stringify(selector)})].find((e) => { const r = e.getBoundingClientRect(); if (!(r.width > 0 && r.height > 0)) return false; if (want === null) return true; const w = (e.textContent || "").trim(); return ${starts ? "w.startsWith(want)" : "w === want"}; }); if (!e) return null; e.scrollIntoView({ block: "nearest" }); const r = e.getBoundingClientRect(); return { x: r.left + Math.min(r.width / 2, 40), y: r.top + r.height / 2, text: (e.textContent || "").trim().slice(0, 60), href: e.getAttribute("href") }; })()`);
  if (!at) throw new Error(`nothing to press: ${selector}${text ? ` "${text}"` : ""}`);
  return at;
}
async function press(at, button = "left") {
  await c.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: at.x, y: at.y });
  await c.send("Input.dispatchMouseEvent", { type: "mousePressed", x: at.x, y: at.y, button, clickCount: 1 });
  await c.send("Input.dispatchMouseEvent", { type: "mouseReleased", x: at.x, y: at.y, button, clickCount: 1 });
}
async function key({ key, code, modifiers = 0, windowsVirtualKeyCode = 0 }) {
  await c.send("Input.dispatchKeyEvent", { type: "rawKeyDown", key, code, modifiers, windowsVirtualKeyCode });
  await c.send("Input.dispatchKeyEvent", { type: "keyUp", key, code, modifiers, windowsVirtualKeyCode });
}
/** The press, key or address that opens `p`. Returns what was pressed. */
async function act(p) {
  if (p.fill) {
    // No press yet: the probe records from the key below.
    await js(`(() => { const i = [...document.querySelectorAll(${JSON.stringify(p.fill.into)})].find((e) => e.getClientRects().length > 0); i.focus(); Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set.call(i, ${JSON.stringify(p.fill.text)}); i.dispatchEvent(new InputEvent("input", { bubbles: true })); return 1; })()`);
    await quiet(1500);
  }
  if (p.type) {
    await js(`(() => { const i = [...document.querySelectorAll(${JSON.stringify(p.type.into)})].find((e) => e.getClientRects().length > 0); i.focus(); return 1; })()`);
    const { key: k } = p.type;
    const code = `Key${k.toUpperCase()}`, windowsVirtualKeyCode = k.toUpperCase().charCodeAt(0);
    await c.send("Input.dispatchKeyEvent", { type: "keyDown", key: k, text: k, unmodifiedText: k, code, windowsVirtualKeyCode });
    await c.send("Input.dispatchKeyEvent", { type: "keyUp", key: k, code, windowsVirtualKeyCode });
    return p.fill ? `${p.fill.text} then typed ${k}` : `typed ${k}`;
  }
  if (p.go) { await js(`__moves.go(${JSON.stringify(p.go)})`); return p.go; }
  if (p.key) { await key(p.key); return "key"; }
  const sel = p.nav ? nav(p.nav) : p.gear ? GEAR : p.right ?? p.press;
  const at = await find(sel, p.text ?? null, !!p.starts);
  await press(at, p.right ? "right" : "left");
  return at.href ?? at.text;
}
async function leave(p) {
  const away = p.away;
  if (away === "clear") {
    await js(`(() => { const i = [...document.querySelectorAll(${JSON.stringify(p.type.into)})].find((e) => e.getClientRects().length > 0); i.focus(); Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set.call(i, ""); i.dispatchEvent(new InputEvent("input", { bubbles: true })); return 1; })()`);
    await quiet(1500);
    return;
  }
  if (away === "Escape") { await key(ESC); await quiet(900); if (typeof p.hash === "string" && (await js("location.hash")).startsWith(p.hash)) { await key(ESC); await quiet(900); } }
  else if (away.startsWith("#/")) { await js(`location.hash = ${JSON.stringify(away)}, 1`); await quiet(1200); }
  else { await press(await find(away === "settings" ? GEAR : nav(away))); await quiet(1200); }
}
/** `before` / `beforeReturn`: addresses (`go`), page scripts (`eval`) and presses (`press`, `text`, `starts`) made, unrecorded, before a round. */
async function steps(list) {
  for (const s of list ?? []) {
    if (s.go) await js(`(location.hash = ${JSON.stringify(s.go)}, 1)`);
    else if (s.eval) await js(`(${s.eval}, 1)`);
    else await press(await find(s.press, s.text ?? null, !!s.starts));
    await quiet(1200);
  }
}
/** `scroll`: a scroller the place is pressed with scrolled to its end (a reader on what stands at the bottom of it). */
async function scrolled(p, rate) {
  if (!p.scroll) return;
  await cpu(1);
  await js(`(() => { const e = document.querySelector(${JSON.stringify(p.scroll)}); if (e) e.scrollTop = e.scrollHeight; return 1; })()`);
  await quiet(400);
  await cpu(rate);
}
async function record(p) {
  await js(`__moves.arm(${JSON.stringify({ hash: p.hash ?? null, ms, px, ready: p.ready ?? null, anywhere: !!p.anywhere, anyKey: !!(p.key || p.type), still: p.still ?? null, changes: p.changes ?? null, answer: p.answer ?? null, trace, track: p.track ?? null })})`);
  const pressed = await act(p);
  let ticker = null;
  if (sync) ticker = setInterval(() => { js(SYNCED).catch(() => {}); }, 1500);
  try {
    const r = await js("__moves.result()", ms + 30000);
    return { pressed, ...r };
  } finally {
    if (ticker) clearInterval(ticker);
  }
}

/** The moves worth naming: those whose parent did not move with them. */
const movers = (r) => (r?.moves ?? []).filter((m) => !m.inner);
const say = (r) => {
  if (!r) return "no result";
  const m = movers(r);
  const head = `drawn ${r.drawnMs ?? "never"} ms, useful ${r.usefulMs ?? "never"} ms, ${r.frames} frames, shift ${r.shift}`;
  if (!m.length) return `${head}, nothing moved${(r.traced ?? []).map((x) => `\n      trace at ${x.t} ms: ${x.sig}`).join("")}`;
  const tr = (r.traced ?? []).map((x) => `\n      trace at ${x.t} ms: ${x.sig}`).join("");
  return `${head}, ${m.length} moved:${tr}\n${m.slice(0, 8).map((x) => `      at ${x.t} ms (${x.sinceDrawn} ms after it drew) ${x.what} by ${x.dx},${x.dy}${x.dw || x.dh ? ` size ${x.dw},${x.dh}` : ""}${x.redrawn ? " (redrawn)" : ""}${x.cause ? `; ${x.cause}` : ""}`).join("\n")}`;
};

const results = { app, at: new Date().toISOString(), size: size.name, throttles, sync, px, places: [] };
const save = () => writeFileSync(out, JSON.stringify(results, null, 1));

/** One place under the watchdog (cdp.mjs): tried once more on a fresh page when a call never answers. */
async function measure(p, rate) {
  let entry;
  let tabs = [];
  const ran = await runCase(c, { name: p.name, throttle: rate }, async () => {
    entry = { name: p.name, throttle: rate };
    tabs = [];
    try {
      await cpu(1);
      await fresh(p.start, p);
      if (p.setup) { await press(await find(p.setup)); await quiet(1200); }
      if (p.setupKey) { await key(p.setupKey); await quiet(1200); }
      if (!p.soon) await sleep(firstAfter);
      await steps(p.before);
      await scrolled(p, rate);
      await cpu(rate);
      if (rounds.includes("first")) {
        entry.first = await record(p);
        // A place whose answer is a run's (`answer`): a first open whose answer never landed fails.
        if (p.answer && entry.first.answerMs == null) entry.first.moves = [...(entry.first.moves ?? []), { t: 0, sinceDrawn: 0, what: "its answer never landed", dx: 0, dy: 0, dw: 0, dh: 0, inner: false }];
        console.log(`${rate}x ${p.name} first (${entry.first.pressed}): ${say(entry.first)}`);
        await quiet(800);
      } else {
        await act(p);
        await quiet(1500);
      }
      if (p.tabs) {
        const list = await js(`[...document.querySelectorAll(${JSON.stringify(`${shown} .tf-tabs a[href]`)})].filter((a) => a.getBoundingClientRect().width > 0).map((a) => a.getAttribute("href"))`);
        const here = await js("location.hash");
        for (const href of [...new Set(list)]) if (href !== here) tabs.push({ name: `${p.name}-tab${href.slice(href.lastIndexOf("/"))}`, start: here, press: `${shown} .tf-tabs a[href="${href}"]`, hash: href, away: here, fromTab: true });
      }
      if (rounds.includes("return") && !p.once) {
        await leave(p);
        await steps(p.beforeReturn);
        // A setup that opens the place's page is pressed once; one that opens a menu the return closed, again.
        if (p.setup && !p.setupOnce) { await press(await find(p.setup)); await quiet(1200); }
        await scrolled(p, rate);
        entry.return = await record(p.returnBy ? { ...p, ...p.returnBy, press: undefined, text: undefined } : p);
        console.log(`${rate}x ${p.name} return: ${say(entry.return)}`);
        await quiet(800);
      }
    } catch (e) {
      if (isStalled(e)) throw e;
      entry.error = String(e.message || e).slice(0, 300);
      console.log(`${rate}x ${p.name}: ERROR ${entry.error}`);
    }
  });
  if (ran.failedToRun) { entry = { name: p.name, throttle: rate, failedToRun: ran.failedToRun }; console.log(failedLine(entry)); }
  results.places.push(entry);
  save();
  return tabs;
}

for (const rate of throttles) {
  for (const p of PLACES.filter(picked).filter((_, i) => i % shard[1] === shard[0] - 1)) {
    const tabs = await measure(p, rate);
    for (const t of tabs) {
      if (skip.some((w) => t.name.startsWith(w))) continue;
      // A tab's first open starts at the place it is a tab of, which keeps its tabs drawn; its return goes back there.
      await measure(t, rate);
    }
  }
}
await cpu(1).catch(() => {});
save();
let over = 0;
for (const e of results.places) {
  // A place that failed to run showed nothing: a gap in the gate, never a pass.
  if (e.failedToRun) { over++; console.log(failedLine(e)); continue; }
  for (const round of ["first", "return"]) {
    const m = movers(e[round]);
    if (m.length) { over++; console.log(`MOVED ${e.throttle}x ${e.name} ${round}: ${m.length} (${m.slice(0, 3).map((x) => `${x.what} ${x.dx},${x.dy}${x.dw || x.dh ? `/${x.dw},${x.dh}` : ""}`).join("; ")})`); }
  }
  if (e.error) {
    // A place this corpus may not hold (a monitor to create from, an asset) is a note; any other is a gap in the gate.
    // `"real"`: optional on the saved scale database only, where a file and a commit need the live forge and *Add an
    // application* a clone, which the fixture has.
    const mark = PLACES.find((p) => e.name === p.name || e.name.startsWith(`${p.name}-tab`))?.optional;
    const optional = mark === true || (mark === "real" && real);
    if (!optional) over++;
    console.log(`${optional ? "NOTE" : "ERROR"} ${e.throttle}x ${e.name}: ${e.error}`);
  }
}
console.log(over ? `moves: ${over} opens moved something or could not be opened` : "moves: nothing moved");
console.log("wrote", out);
c.close();
browser.stop();
process.exit(check && over ? 1 : 0);
