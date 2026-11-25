// Long filesystem paths held to their boxes, in headless Chrome against a built app under ?fake-ipc (the clone-path pass, PR #2479,
// 2026-11-16: Björn's "cloned at C:\x\y\z\foo\bar can get long in certain places and go out of bounds").
//
// Usage:
//   testenv/ui-checks/build-app.sh . <folder>
//   testenv/ui-checks/with-built.sh <folder> <port> node testenv/ui-checks/long-paths.mjs <out dir> --port <port> [--only a,b] [--part k/m] [--shots]
// `--part k/m`: every m-th screen from the k-th, so m runs share the screens (a run too long for one turn on a machine's
// queue, the final suite, 2026-11-22).
// `--shots` also writes each screen with the long paths as <out dir>/<screen>-<size>.png, and with the fixture's own short ones
// as <screen>-<size>-short.png, to look at or to hold to `pixel-same.py` against another head.
//
// Each screen below opens with `?fake-long-paths`, which makes every path the fixture draws a long Windows one with spaces (a clone,
// the clones root, git, a pull request's worktree) and adds renderers with long names whose programs are long Windows and Mac
// paths (the render-as pass, 2026-11-19: Björn's "the render as orchestra, when you click on it, and the path of the program
// is long it gets out of bounds"), at each size a person uses: 5120x1440@1, 2560x1440@1.25, 1920x1080@2 and the
// window's smallest, 1100x600. It fails when:
//   - a run of a path's text reaches past the window, or past any box that holds it, cut or spilling, unless it is the start of a
//     path drawn by `TfPath` (`.tf-path-head`), which gives up its middle for the folder at its end, whole path on hover;
//   - an ellipsis cuts a path, other than that start (a renderer's long name may end in one: its whole name is its title);
//   - a box scrolls sideways with the long paths and did not with the fixture's short ones;
//   - a Builds header's line under its title is more than one line tall (the build-header pass, 2026-11-20), or its
//     actions are not on the title's line, at 1440x868 too.
// Toasts and an opening's chip are read too (ux21, 2026-11-19: a toast carrying a path grew to three lines and 1,400 px): a
// toast's line gives up its middle (`.tf-toast-head`), as a path's start does.
// A visually hidden run (the sign's description, which the witness and a screen reader read) is left out.
// A screen with `tables` (the renderers-columns pass, 2026-11-20: Björn's "in the renderers view, the command, the files, and so
// on completely explode the columns") also holds every table it names to the shape Code's *Local checkout* column has: each
// cell one line (a command, a pattern list, a path cut rather than wrapped), each column as wide with the long values as with
// the fixture's short ones, and the table no wider than the box that holds it.
// A screen with `under` (the small-fixes sweep, 2026-11-21: *Add a default*'s two dropdowns grew to 1,370 px with long names)
// holds a row of controls to a table's columns: [a control in the row, the table], and each control of that row must stand
// inside the table's column of the same place, its long names cut in their box.
// A screen with `whole` (the ux30 end gate, 2026-11-23: Passwords' *Open web interface W* cut at the entry panel's edge in a
// 1100 px window) holds every button under the selectors it names to the window and to every box that clips it, sideways: a
// control is drawn whole or the screen misses.
//
// Writes <out dir>/long-paths.json and exits 1 on any miss, or if a screen's step finds nothing.
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { launch, screenOpener, sleep } from "./chrome.mjs";

const args = process.argv.slice(2);
const out = args[0];
const port = Number(args[args.indexOf("--port") + 1]);
const only = args.includes("--only") ? args[args.indexOf("--only") + 1].split(",") : null;
const shots = args.includes("--shots");
const [partK, partM] = args.includes("--part") ? args[args.indexOf("--part") + 1].split("/").map(Number) : [1, 1];
if (!out || !port || !(partK >= 1 && partK <= partM)) {
  console.error("usage: long-paths.mjs <out dir> --port <port> [--only a,b] [--part k/m]");
  process.exit(2);
}

const SIZES = [
  [5120, 1440, 1],
  [2048, 1152, 1.25],
  [960, 540, 2],
  [1100, 600, 1],
];
/** The window's smallest alone: the check's screen (the gate's long paths run at 1100 x 600). */
const SMALLEST = [[1100, 600, 1]];
// The Builds screens add 1440x868, the size Björn's report came from (the build-header pass, 2026-11-20).
const BUILD_SIZES = [
  [5120, 1440, 1],
  [2048, 1152, 1.25],
  [1440, 868, 1],
  [1100, 600, 1],
];
// Passwords' screens add the window's smallest at a usual height and 1440x868, the sizes the ux30 end gate read.
const PW_SIZES = [...SIZES, [1100, 800, 1], [1440, 868, 1]];
const REPO = "repo/mock%3Atidewater%2Fpayout-service";
const PIPELINE = "builds/teamcity%3AbuildType%3APayout_Docker";
const RUN = "build/teamcity%3AbuildType%3APayout_Docker/teamcity%3Abuild%3APayout_Docker-611";
// `press`: the control to press first, by its words, `~` and words it holds, or a selector; `then`: more to press after it, in
// order (a menu's item); `focus`: the control to focus (the sign's bubble opens on it); `context`: what to right-click, by a
// selector and the words it holds; `item`: then a menu's row to press, by a selector; `wait`: ms to wait before reading (a
// clone's toast comes when it ends, a folder renderer's run takes 1.5 s in the fixture); `toast`: a toast's button to press.
const SCREENS = [
  { id: "repo-files", route: REPO },
  { id: "repo-sign-focus", route: REPO, focus: ".tf-signbtn" },
  { id: "repo-sign-hover", route: REPO, hover: ".tf-signbtn" },
  { id: "repo-open-in", route: REPO, press: "Open in…" },
  { id: "repo-local", route: `${REPO}/local` },
  { id: "repo-branches", route: `${REPO}/branches` },
  { id: "repo-commits", route: `${REPO}/commits` },
  { id: "repo-file", route: `${REPO}/files/src/sepa/retry.rs` },
  { id: "repo-file-open-in", route: `${REPO}/files/src/sepa/retry.rs`, press: "Open in…" },
  { id: "code", route: "code" },
  { id: "pr", route: "pr/mock%3Apayout-service%23142" },
  { id: "pr-open-in", route: "pr/mock%3Apayout-service%23142", press: "Open in…" },
  { id: "settings-checkouts", route: "settings/checkouts" },
  { id: "settings-remove-worktree", route: "settings/checkouts", press: "Remove…" },
  // Settings › Appearance › Code colours (2026-11-21): the slots, the role list and the preview held to their boxes.
  { id: "settings-code-colours", route: "settings/appearance/code-colours" },
  // Settings › Appearance › Theme (ADR-0055): the slots, the role list with its long names and the preview, held to their boxes.
  { id: "settings-theme", route: "settings/appearance/theme" },
  { id: "job", route: "job/PAY-231" },
  { id: "toast-cloned", route: "repo/mock%3Atidewater%2Fledger", press: "Clone", wait: 3800 },
  { id: "toast-cloned-open", route: "repo/mock%3Atidewater%2Fledger", press: "Clone", wait: 3800, toast: "Open in VS Code" },
  { id: "ack-open-in", route: REPO, press: "Open in…", item: ".tf-appmenu [role=menuitem]" },
  // A renderer's long name and program (the render-as pass).
  { id: "repo-file-render-as", route: `${REPO}/files/src/sepa/retry.rs`, press: ".tf-rsw-face" },
  { id: "repo-file-render-picked", route: `${REPO}/files/src/sepa/retry.rs`, press: ".tf-rsw-face", then: ["~Orchestra score renderer"], wait: 300 },
  { id: "repo-file-render-picked-mac", route: `${REPO}/files/src/sepa/retry.rs`, press: ".tf-rsw-face", then: ["~Orchestra on the Mac"], wait: 300 },
  { id: "repo-file-render-reopened", route: `${REPO}/files/src/sepa/retry.rs`, press: ".tf-rsw-face", then: ["~Orchestra score renderer", ".tf-rsw-face"] },
  { id: "repo-file-pipeline-add", route: `${REPO}/files/src/sepa/retry.rs`, press: "Pipeline", then: ["Add step…"] },
  { id: "repo-folder-render", route: `${REPO}/render/orchestra-map/src`, wait: 1900 },
  { id: "repo-folder-render-fails", route: `${REPO}/render/orchestra-map-mac/src`, wait: 1900 },
  { id: "repo-folder-menu", route: REPO, context: [".tf-ftree button", "src"] },
  // A generated folder (ADR-0046): the band and the made bar with a 60-character renderer name, a generated file's
  // 140-character path, what was left out, and *Open in…* on the kept folder.
  { id: "rerender-graft", route: `${REPO}/rendered/site/docs`, fake: "&fake-rerender", wait: 2600 },
  { id: "rerender-left-out", route: `${REPO}/rendered/site/docs`, fake: "&fake-rerender", press: "2 left out" },
  { id: "rerender-open-in", route: `${REPO}/rendered/site/docs`, fake: "&fake-rerender", wait: 2600, press: ".tf-madebar-open .tf-openin:not(.tf-oo-runin) button" },
  // A folder renderer in a comparison (ux26): Compare with the working tree, *Rendered* on, `docs` grouped under its bar with
  // the 60-character renderer's name, the rail's band, the bar in each state, the generated files' paths (one of 140
  // characters), and the glyph's menu where two renderers take the folder.
  { id: "compare-folder-idle", route: `${REPO}/compare/~working-tree`, fake: "&fake-rerender", press: ".tf-drendered", wait: 600 },
  { id: "compare-folder-running", route: `${REPO}/compare/~working-tree`, fake: "&fake-rerender", press: ".tf-drendered", then: [".tf-dfolder .tf-madebar-run"], wait: 300 },
  { id: "compare-folder-made", route: `${REPO}/compare/~working-tree`, fake: "&fake-rerender", press: ".tf-drendered", then: [".tf-dfolder .tf-madebar-run"], wait: 3500 },
  { id: "compare-folder-failed", route: `${REPO}/compare/~working-tree`, fake: "&fake-rerender&fake-rerender-fail", press: ".tf-drendered", then: [".tf-dfolder .tf-madebar-run"], wait: 3500 },
  { id: "compare-folder-pick", route: `${REPO}/compare/~working-tree`, fake: "&fake-rerender&fake-rerender-two", press: ".tf-drendered", then: [".tf-dgband", ".tf-dfmark"], wait: 300 },
  { id: "settings-renderers", route: "settings/renderers", tables: ".tf-stg-body .tf-tbl" },
  { id: "settings-renderers-add-default", route: "settings/renderers", under: ["#tf-rnd-dpat", ".tf-rnd-defaults"] },
  { id: "settings-renderer-edit", route: "settings/renderers", press: '[aria-label^="Edit Orchestra score"]' },
  { id: "settings-folder-renderer", route: "settings/renderers/folder/orchestra-map" },
  // A TeamCity configuration whose VCS roots are named by their whole addresses, 64th of a chain of 70 long names (the
  // build-header pass, 2026-11-20: Björn's "builds <huge repo link> step 64 of the chain x > y > z > foo > bar > etc.").
  { id: "build-pipeline", route: PIPELINE, sizes: BUILD_SIZES },
  { id: "build-chain-focus", route: PIPELINE, focus: ".tf-chainsign .tf-signbtn", sizes: BUILD_SIZES },
  { id: "build-chain-hover", route: PIPELINE, hover: ".tf-chainsign .tf-signbtn", sizes: BUILD_SIZES },
  { id: "build-chain-pinned", route: PIPELINE, press: ".tf-chainsign .tf-signbtn", sizes: BUILD_SIZES },
  { id: "build-run", route: RUN, wait: 300, sizes: BUILD_SIZES },
  { id: "build-run-unfolded", route: RUN, wait: 300, press: ".tf-chainfold", sizes: BUILD_SIZES },
  // A command, ten patterns and five fences, a folder renderer's long lists, a pipeline's long name and steps, defaults with a
  // deep pattern, and a Java deep under a profile (the renderers-columns pass).
  { id: "settings-folder-renderer-lists", route: "settings/renderers/folder/orchestra-flows" },
  { id: "settings-pipeline-edit", route: "settings/renderers", press: '[aria-label^="Edit Decode, unpack"]' },
  // The launcher under *Everything* (the everything-search build, 2026-11-20): a repository its files found draws a file's path
  // in its evidence line and in the pane, under `?fake-long-paths` each one deep in a folder with a long name.
  { id: "launcher-file-rows", route: "tickets", launcher: "settlebridge" },
  { id: "launcher-file-pane", route: "tickets", launcher: "#r settlebridge" },
  { id: "launcher-file-names", route: "tickets", launcher: "#r deployment" },
  // A source's site at a long address (ADR-0050): the host whole, the rest given up at its end, the whole in the title.
  { id: "site-bar", route: "site/confluence", press: "Open Tidewater Confluence", wait: 700, sizes: BUILD_SIZES },
  // A site's saved sign-in (ux29): Settings' row naming a long username, held to its column.
  { id: "settings-site-signin", route: "settings/source/confluence" },
  // Passwords (ADR-0056): an entry's header and a folder's, every button whole, at the window's smallest sizes and the gate's.
  // The check in the XML view (2026-11-24): the rail's head and its switch whole at the window's smallest.
  { id: "xml-checked", route: `${REPO}/files/orders/order-4711.xml`, fake: "&fake-xml&fake-xsd", wait: 3000, whole: ".tf-xv-head, .tf-xv-rail > .tf-oas-modes", sizes: SMALLEST },
  // The schema explorer's picker and its example (S3, S4), and the JSON view's jq field (J2), 2026-11-24: their controls whole.
  { id: "xsd-point", route: `${REPO}/files/orders/order-4711.xml`, fake: "&fake-xml&fake-xsd", wait: 3000, press: ".tf-xv-schema", then: ["~Point to another file"], whole: ".tf-xsd-point", sizes: SMALLEST },
  // The fixture's schema files load after the first read of the repository, so the explorer is reached from the order's head.
  { id: "xsd-example", route: `${REPO}/files/orders/order-4711.xml`, fake: "&fake-xml&fake-xsd", press: "opentrans_2_1.xsd", then: ["~ORDER", ".tf-xsd .tf-oas-pageseg button:last-child"], wait: 3000, whole: ".tf-xsd-ex", sizes: SMALLEST },
  // The example opened in the XML view (E1): the header's name after the schema's path, its meta and its buttons whole.
  { id: "xsd-example-open", route: `${REPO}/files/orders/order-4711.xml`, fake: "&fake-xml&fake-xsd", press: "opentrans_2_1.xsd", then: ["~ORDER", ".tf-xsd .tf-oas-pageseg button:last-child", "~Open in XML view"], wait: 3000, whole: ".tf-fileview header .tf-trail, .tf-xsd-opened .tf-xv-head", sizes: SMALLEST },
  // Zen mode everywhere in Code (Z1): its button in the top bar whole, and a file in Zen, its header's path whole.
  { id: "zen-code-file", route: `${REPO}/files/src/sepa/retry.rs`, press: ".tf-top > .tf-zenbtn", wait: 500, whole: ".tf-top, .tf-fileview header", sizes: SMALLEST },
  { id: "json-jq", route: `${REPO}/files/exports/orders-2026-11.json`, fake: "&fake-xml", wait: 1500, whole: ".tf-xv-qbar, .tf-xv-head, .tf-xv-rail > .tf-oas-modes", sizes: SMALLEST },
  // Commands (ADR-0060): Settings › Commands with a command whose program is deep under folders with spaces, its page, and
  // a run's dialog in a long Windows checkout, its run line naming the program, the checkout and the file by their whole paths.
  { id: "settings-commands", route: "settings/commands", fake: "&fake-oneoff", wait: 600, whole: ".tf-stg-body", sizes: SMALLEST },
  { id: "settings-command-page", route: "settings/commands/orchestra", fake: "&fake-oneoff", wait: 600, whole: ".tf-stg-body", sizes: SMALLEST },
  { id: "run-dialog", route: `${REPO}/files/src/sepa/retry.rs`, fake: "&fake-oneoff", press: ".tf-fileview header .tf-oo-runin button", then: ["~Orchestra lint"], wait: 900, whole: ".tf-modal .tf-oo-line, .tf-modal .tf-oo-bar", sizes: SMALLEST },
  // A Docs page's XML, JSON and schema blocks (D1): each block's bar, the views' Levels bars and the schema's rows whole in the
  // window's smallest, the three views open at once; a declaration opened in place; a ticket's junit.xml in the file viewer.
  { id: "docs-blocks", route: "page/confluence%3Aedi-formats", press: "Schema", then: ["XML view", "JSON view"], wait: 1500, whole: ".tf-pre-bar, .tf-doc-xv .tf-xv-bar, .tf-xsd-inline .tf-xsd-irow", sizes: SMALLEST },
  { id: "docs-blocks-decl", route: "page/confluence%3Aedi-formats", press: "Schema", then: ["~AmountType"], wait: 1500, whole: ".tf-pre-bar, .tf-xsd-iopen .tf-xsd-h, .tf-xsd-iopen .tf-oas-meta", sizes: SMALLEST },
  { id: "viewer-junit", route: "ticket/mock%3APAY-228", press: '[aria-labelledby="tf-tkatt"] summary', then: ["~junit.xml"], wait: 1500, whole: ".tf-viewer, .tf-vdoc .tf-xv-bar", sizes: SMALLEST },
  { id: "passwords-entry", route: "passwords/entry/team%3Apl-payout-db", whole: ".tf-pw-ehead, .tf-pw-head", sizes: PW_SIZES },
  { id: "passwords-folder", route: "passwords/in/team%3Af%2FTidewater%2FPayments%2FProd%2FDatabases", whole: ".tf-pw-head", sizes: PW_SIZES },
];

// Runs in the page: the cells of `selector`'s tables that take more than one line, each table's column widths, and the tables
// wider than their box. A table is named by its classes and its place among the tables, so the short run's is found again.
const TABLES = (selector) => String.raw`(() => {
  const root = document.querySelector(".tf-kept:not([hidden])") ?? document.querySelector(".tf-main") ?? document.body;
  const out = { wraps: [], widths: {}, wider: [] };
  [...root.querySelectorAll(${JSON.stringify(selector)})].forEach((table, i) => {
    if (table.getBoundingClientRect().width === 0) return;
    const key = [...table.classList].join(".") + "#" + i;
    out.widths[key] = [...table.querySelectorAll("thead th")].map((th) => Math.round(th.getBoundingClientRect().width));
    const box = table.parentElement.getBoundingClientRect();
    const b = table.getBoundingClientRect();
    if (b.right > box.right + 1) out.wider.push({ at: key, over: Math.round(b.right - box.right) });
    for (const td of table.querySelectorAll("tbody td")) {
      const centres = [];
      const walker = document.createTreeWalker(td, NodeFilter.SHOW_TEXT);
      for (let node = walker.nextNode(); node; node = walker.nextNode()) {
        if (node.textContent.trim() === "" || node.parentElement.closest(".tf-sr, option, select")) continue;
        const range = document.createRange();
        range.selectNodeContents(node);
        for (const r of range.getClientRects()) if (r.width > 0) centres.push((r.top + r.bottom) / 2);
      }
      if (centres.length > 1 && Math.max(...centres) - Math.min(...centres) > 6)
        out.wraps.push({ at: key + " > td:nth-child(" + (td.cellIndex + 1) + ")", text: td.textContent.trim().slice(0, 48), lines: Math.round((Math.max(...centres) - Math.min(...centres)) / 16) + 1 });
    }
  });
  return JSON.stringify(out);
})()`;

// Runs in the page: each control in the row that holds `control` against the column of `table` at its place; null when either
// is missing.
const UNDER = ([control, table]) => String.raw`(() => {
  const root = document.querySelector(".tf-kept:not([hidden])") ?? document.querySelector(".tf-main") ?? document.body;
  const row = root.querySelector(${JSON.stringify(control)})?.parentElement;
  const heads = [...(root.querySelector(${JSON.stringify(table)})?.querySelectorAll("thead th") ?? [])];
  if (!row || heads.length === 0) return "null";
  const out = [];
  [...row.children].forEach((e, i) => {
    const b = e.getBoundingClientRect(), col = heads[i]?.getBoundingClientRect();
    if (!col) { out.push({ at: e.tagName.toLowerCase() + "#" + i, over: Math.round(b.width) }); return; }
    const over = Math.max(b.right - col.right, col.left - b.left);
    if (over > 1) out.push({ at: (e.getAttribute("aria-label") ?? e.tagName.toLowerCase()) + "#" + i, over: Math.round(over) });
  });
  return JSON.stringify(out);
})()`;

// Runs in the page: what of a path's text leaves its boxes, and which boxes scroll sideways.
const CHECK = String.raw`(() => {
  const MARK = /folders with spaces|repository-name-that-is-long|Git for Windows|\/Users\/mara\/src|Orchestra Renderer|service-account-with-a-long-name/;
  // What gives way on purpose, its whole in a title: a path's start, a toast's middle, and a command's program start and its
  // arguments after the program's file (TfCommand, the renderers-columns pass).
  const CUT = ["tf-path-head", "tf-toast-head", "tf-cmd-head", "tf-cmd-args", "tf-site-path"];
  // A renderer's long name: held to its boxes as a path is, but it may end in an ellipsis.
  const NAME = /payments team's/;
  const roots = [document.querySelector(".tf-kept:not([hidden])") ?? document.querySelector(".tf-main") ?? document.body, document.querySelector(".tf-floats"), document.querySelector(".tf-toasts"), document.querySelector(".tf-acks"), document.querySelector(".tf-launch"), ...[...document.querySelectorAll(".tf-modal")].filter((m) => !m.closest(".tf-main, .tf-kept"))].filter(Boolean);
  const name = (el) => el.tagName.toLowerCase() + [...el.classList].map((c) => "." + c).join("");
  const where = (el) => { const parts = []; for (let e = el; e && e !== document.body && parts.length < 4; e = e.parentElement) parts.unshift(name(e)); return parts.join(" > "); };
  const misses = [];
  for (const root of roots) {
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
    for (let node = walker.nextNode(); node; node = walker.nextNode()) {
      if (!MARK.test(node.textContent) && !NAME.test(node.textContent)) continue;
      const range = document.createRange();
      range.selectNodeContents(node);
      const rects = [...range.getClientRects()].filter((r) => r.width > 0);
      if (rects.length === 0) continue;
      let hidden = false, cutOnPurpose = false;
      const name = NAME.test(node.textContent) && !MARK.test(node.textContent);
      // A file's or a drawing's own text in the code view scrolls sideways as code does: it is not the app's. Nor is what a
      // command printed (ADR-0060), which scrolls the same way until *Wrap* folds it.
      if (node.parentElement.closest(".tf-code, .tf-oo-out")) continue;
      for (let e = node.parentElement; e; e = e.parentElement) {
        const s = getComputedStyle(e);
        if (s.clipPath !== "none" || s.visibility === "hidden" || s.display === "none") hidden = true;
        if (CUT.some((c) => e.classList.contains(c))) cutOnPurpose = true;
        // A renderer's name, no path in it, in a box that ends it in an ellipsis: cut on purpose, whole in a title.
        if (name && s.textOverflow === "ellipsis" && s.overflowX !== "visible") cutOnPurpose = true;
      }
      if (hidden) continue;
      const left = Math.min(...rects.map((r) => r.left)), right = Math.max(...rects.map((r) => r.right));
      // What shows: the run as laid out, inside every box that clips it. A run clipped out of sight is not drawn.
      let shownLeft = left, shownRight = right;
      for (let e = node.parentElement; e; e = e.parentElement) {
        if (getComputedStyle(e).overflowX === "visible") continue;
        const b = e.getBoundingClientRect();
        shownLeft = Math.max(shownLeft, b.left);
        shownRight = Math.min(shownRight, b.right);
      }
      if (shownRight <= shownLeft) continue;
      const text = node.textContent.trim().slice(-48);
      if (shownRight > innerWidth + 0.5 || shownLeft < -0.5) { misses.push({ kind: "window", at: where(node.parentElement), text, over: Math.round(Math.max(shownRight - innerWidth, -shownLeft)) }); continue; }
      if (cutOnPurpose) continue;
      for (let e = node.parentElement; e && e !== document.body; e = e.parentElement) {
        const s = getComputedStyle(e);
        if (s.display === "inline") continue;
        const b = e.getBoundingClientRect();
        if (b.width === 0) continue;
        if (right > b.right + 1 || left < b.left - 1) { misses.push({ kind: "box", at: where(e), text, over: Math.round(Math.max(right - b.right, b.left - left)) }); break; }
        // A float (the sign's bubble, a menu) stands out of the boxes around it on purpose: past it, only the window holds it.
        if (s.position === "absolute" || s.position === "fixed") break;
      }
    }
  }
  // A path an ellipsis cuts: Chrome reports such a run as only what shows, so the boxes above can't see it.
  for (const root of roots) for (const e of root.querySelectorAll("*")) {
    // Strict: an ellipsis shows for a shortfall under a pixel, which rounds away in a +1 margin.
    if (e.clientWidth === 0 || e.scrollWidth <= e.clientWidth || !MARK.test(e.textContent) || e.closest(".tf-code, .tf-oo-out")) continue;
    const s = getComputedStyle(e);
    if (s.textOverflow !== "ellipsis" || CUT.some((c) => e.classList.contains(c)) || e.closest(".tf-sr")) continue;
    misses.push({ kind: "cut", at: where(e), text: e.textContent.trim().slice(-48), over: e.scrollWidth - e.clientWidth });
  }
  const scrolls = [];
  for (const root of roots) for (const e of [root, ...root.querySelectorAll("*")]) {
    if (e.clientWidth === 0 || e.scrollWidth <= e.clientWidth + 1) continue;
    const s = getComputedStyle(e);
    // A clip is no scroll: the kept page clips what passes under the scroller's 8 px bar on purpose (chrome.css).
    // A visually hidden run (a list's rest, which a screen reader reads) is not drawn, so it scrolls nothing either.
    if (s.overflowX === "visible" || s.overflowX === "clip" || s.textOverflow === "ellipsis" || e.closest(".tf-code, .tf-oo-out") || e.closest(".tf-sr")) continue;
    scrolls.push(where(e));
  }
  // A Builds header's line is one line, under the title and its actions, and the actions share the title's line.
  for (const root of roots) for (const head of root.querySelectorAll(".tf-bhead")) {
    const sub = head.querySelector(".tf-sub"), title = head.querySelector("h1"), acts = head.querySelector(".tf-acts");
    if (!sub || !title || sub.getBoundingClientRect().width === 0) continue;
    const line = parseFloat(getComputedStyle(sub).lineHeight) || 18;
    const tall = sub.getBoundingClientRect().height;
    if (tall > line * 1.5 + 2) misses.push({ kind: "lines", at: where(sub), text: "the header's line wraps", over: Math.round(tall - line) });
    // Narrower, a title too long to share its line sends the actions under it, by design (builds.css).
    if (acts && innerWidth >= 1400 && Math.abs(acts.getBoundingClientRect().top - title.getBoundingClientRect().top) > title.getBoundingClientRect().height)
      misses.push({ kind: "acts", at: where(acts), text: "the header's actions left the title's line", over: Math.round(acts.getBoundingClientRect().top - title.getBoundingClientRect().top) });
  }
  return JSON.stringify({ misses, scrolls, page: document.documentElement.scrollWidth > innerWidth + 1 });
})()`;

const PRESS = (words) => String.raw`(() => {
  const words = ${JSON.stringify(words)};
  const root = document.querySelector(".tf-kept:not([hidden])") ?? document.querySelector(".tf-main");
  const floats = document.querySelector(".tf-floats");
  // What a person can press: drawn, and not disabled (a generated folder's *Open in…* is, until the run has made the folder).
  const shown = (e) => e.getBoundingClientRect().width > 0 && !e.disabled;
  const all = [...root.querySelectorAll("button, a"), ...(floats ? floats.querySelectorAll("button, a") : [])];
  const found = /^[.#[]/.test(words)
    ? [...document.querySelectorAll(words)].find(shown)
    : words.startsWith("~")
      ? all.find((e) => e.textContent.includes(words.slice(1)) && shown(e))
      : all.find((e) => e.textContent.trim() === words && shown(e));
  if (!found) return false;
  found.click();
  return true;
})()`;

// Runs in the page: each button under `selector` that is drawn and not shown whole, sideways, inside the window and every box
// that clips it. Null when no button is drawn there.
const WHOLE = (selector) => String.raw`(() => {
  const name = (el) => el.tagName.toLowerCase() + [...el.classList].map((c) => "." + c).join("");
  const where = (el) => { const parts = []; for (let e = el; e && e !== document.body && parts.length < 4; e = e.parentElement) parts.unshift(name(e)); return parts.join(" > "); };
  const buttons = [...document.querySelectorAll(${JSON.stringify(selector)})].flatMap((head) => [...head.querySelectorAll("button, a.tf-btn")]).filter((b) => b.getBoundingClientRect().width > 0);
  if (buttons.length === 0) return "null";
  const out = [];
  for (const b of buttons) {
    const r = b.getBoundingClientRect();
    let left = Math.max(r.left, 0), right = Math.min(r.right, innerWidth);
    // Past a fixed box (a dialog's scrim, drawn where it was opened), only an ancestor that holds fixed
    // boxes (a transform, a filter, containment: holdsFixed in TfDialog) clips it; another's overflow does not.
    let fixed = false;
    const holds = (s) => (s.transform !== "none" && s.transform !== "") || (s.filter !== "none" && s.filter !== "") || /\b(transform|filter|perspective)\b/.test(s.willChange) || /\b(layout|paint|strict|content)\b/.test(s.contain);
    for (let e = b.parentElement; e; e = e.parentElement) {
      const s = getComputedStyle(e);
      if (fixed && !holds(s)) continue;
      if (s.position === "fixed") fixed = true;
      if (s.overflowX === "visible") continue;
      const box = e.getBoundingClientRect();
      left = Math.max(left, box.left);
      right = Math.min(right, box.right);
    }
    const cut = Math.max(0, r.right - right) + Math.max(0, left - r.left);
    if (cut > 0.5) out.push({ at: where(b), text: b.textContent.trim(), over: Math.round(cut) });
  }
  return JSON.stringify(out);
})()`;

// Opens the launcher (⌘K) under *Everything* (⌘D when it is under *Titles*), types `words` and waits for the pane to follow the
// first row. False when the launcher or its rows never come.
const LAUNCHER = (words) => String.raw`(async () => {
  const wait = async (test) => { for (let i = 0; i < 50; i++) { const v = test(); if (v) return v; await new Promise((r) => setTimeout(r, 100)); } return null; };
  document.dispatchEvent(new KeyboardEvent("keydown", { key: "k", metaKey: true, bubbles: true }));
  const box = await wait(() => document.querySelector("[role=dialog] input"));
  if (!box) return false;
  if (localStorage.getItem("knobas.launcher.scope") !== "everything") box.dispatchEvent(new KeyboardEvent("keydown", { key: "d", metaKey: true, bubbles: true, cancelable: true }));
  if (localStorage.getItem("knobas.launcher.scope") !== "everything") return false;
  const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set;
  set.call(box, ${JSON.stringify(words)});
  box.dispatchEvent(new InputEvent("input", { bubbles: true }));
  if (!(await wait(() => document.querySelector(".tf-launch [role=option] .tf-ln-why")))) return false;
  await new Promise((r) => setTimeout(r, 600));
  return true;
})()`;

// How long a press waits for its control to be drawn. The screen's settle sits out no timer over 900 ms (chrome.mjs), so it can
// call a page quiet while the fixture's folder run (1.5 s) is still going: "2 left out" on a generated folder came after the
// press in about half the reads, a different size failing each run, and *Open in…* on the made bar was pressed while still
// disabled, so that screen shot no menu and passed with nothing to hold (2026-11-20). A person presses what is shown and
// enabled, so a press waits for that, and still fails, saying it found nothing, when it never comes.
const PRESS_WAIT_MS = 5000;

const chrome = await launch("knobas-long-paths");
const { send, evaluate } = chrome;
const press = async (words) => {
  for (const until = Date.now() + PRESS_WAIT_MS; ; await sleep(100)) {
    if (await evaluate(PRESS(words))) return true;
    if (Date.now() > until) return false;
  }
};
const opener = screenOpener(chrome, port);
await opener.install();
// Headless, the page has no focus of its own, so a focus() would fire no focus event and the sign's bubble would stay shut.
await send("Emulation.setFocusEmulationEnabled", { enabled: true });

const results = [];
let failed = 0;
for (const [at, screen] of SCREENS.entries()) {
  if (at % partM !== partK - 1) continue;
  if (only && !only.some((word) => screen.id.includes(word))) continue;
  for (const [width, height, scale] of screen.sizes ?? SIZES) {
    await send("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: scale, mobile: false });
    const size = `${width}x${height}@${scale}`;
    const read = async (query) => {
      // `fake`: more of the fixture's switches the screen needs (a generated folder's `&fake-rerender`).
      await opener.openScreen({ route: screen.route, query: query + (screen.fake ?? "") });
      let stepped = true;
      if (screen.press) stepped = await press(screen.press);
      for (const next of screen.then ?? []) {
        if (!stepped) break;
        await opener.settleAfterChange();
        stepped = await press(next);
      }
      if (screen.context) {
        const [selector, words] = screen.context;
        stepped = await evaluate(`(() => {
          const e = [...document.querySelectorAll(${JSON.stringify(selector)})].find((e) => e.textContent.trim() === ${JSON.stringify(words)});
          if (!e) return false;
          const b = e.getBoundingClientRect();
          e.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: b.left + 8, clientY: b.top + b.height / 2, button: 2 }));
          return true;
        })()`);
      }
      if (screen.launcher) stepped = await evaluate(LAUNCHER(screen.launcher));
      if (screen.focus) stepped = await evaluate(`(() => { const e = document.querySelector(${JSON.stringify(screen.focus)}); if (!e) return false; e.focus(); return true; })()`);
      if (screen.hover) {
        const box = await evaluate(`(() => { const e = document.querySelector(${JSON.stringify(screen.hover)}); if (!e) return null; const b = e.getBoundingClientRect(); return { x: b.left + b.width / 2, y: b.top + b.height / 2 }; })()`);
        stepped = box !== null;
        if (box) await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: box.x, y: box.y });
        await sleep(600);
      }
      if (screen.item) {
        await opener.settleAfterChange();
        stepped = stepped && (await evaluate(`(() => { const e = document.querySelector(${JSON.stringify(screen.item)}); if (!e) return false; e.click(); return true; })()`));
      }
      if (screen.wait) await sleep(screen.wait);
      if (screen.toast) {
        stepped = stepped && (await evaluate(`(() => { const e = [...document.querySelectorAll(".tf-toast button")].find((b) => b.textContent.trim() === ${JSON.stringify(screen.toast)}); if (!e) return false; e.click(); return true; })()`));
      }
      if (screen.press || screen.focus || screen.context || screen.launcher) await opener.settleAfterChange();
      const tables = screen.tables ? JSON.parse(await evaluate(TABLES(screen.tables))) : null;
      const under = screen.under ? JSON.parse(await evaluate(UNDER(screen.under))) : null;
      if (screen.under && under === null) stepped = false;
      const whole = screen.whole ? JSON.parse(await evaluate(WHOLE(screen.whole))) : null;
      if (screen.whole && whole === null) stepped = false;
      return { stepped, tables, under, whole, ...JSON.parse(await evaluate(CHECK)) };
    };
    const shoot = async (name) => {
      if (!shots) return;
      const data = (await send("Page.captureScreenshot", { format: "png" })).result.data;
      mkdirSync(out, { recursive: true });
      writeFileSync(join(out, `${name}.png`), Buffer.from(data, "base64"));
    };
    const short = await read("");
    await shoot(`${screen.id}-${size}-short`);
    const long = await read("&fake-long-paths");
    await shoot(`${screen.id}-${size}`);
    const scrolls = long.scrolls.filter((box) => !short.scrolls.includes(box));
    const misses = [
      ...(long.stepped ? [] : [{ kind: "step", at: [screen.whole, screen.under?.join(" under "), screen.launcher, screen.press, ...(screen.then ?? []), screen.focus, screen.hover, screen.context?.join(" "), screen.item, screen.toast].filter(Boolean).join(" then "), text: "found nothing to press" }]),
      ...long.misses,
      ...scrolls.map((at) => ({ kind: "scrolls", at, text: "scrolls sideways only with the long paths" })),
      ...(long.page && !short.page ? [{ kind: "page", at: "html", text: "the page is wider than the window" }] : []),
      ...(long.under ?? []).map((u) => ({ kind: "under", at: u.at, text: "stands outside the table's column", over: u.over })),
      ...[...(short.whole ?? []), ...(long.whole ?? [])].map((w) => ({ kind: "whole", at: w.at, text: `"${w.text}" is cut`, over: w.over })),
      ...(long.tables?.wraps ?? []).map((w) => ({ kind: "wraps", at: w.at, text: `${w.text} (${w.lines} lines)` })),
      ...(long.tables?.wider ?? []).map((w) => ({ kind: "wider", at: w.at, text: "the table is wider than its box", over: w.over })),
      ...Object.entries(long.tables?.widths ?? {})
        .filter(([key, widths]) => short.tables?.widths[key] && widths.some((w, i) => Math.abs(w - short.tables.widths[key][i]) > 1))
        .map(([key, widths]) => ({ kind: "columns", at: key, text: `columns ${short.tables.widths[key].join("/")} px with the short values, ${widths.join("/")} with the long` })),
    ];
    if (misses.length > 0) failed += 1;
    results.push({ screen: screen.id, size, misses });
    console.log(`${misses.length === 0 ? "ok  " : "MISS"} ${screen.id} ${size}${misses.map((m) => `\n       ${m.kind}: ${m.at}: ${m.text}${m.over ? ` (${m.over} px)` : ""}`).join("")}`);
  }
}
chrome.stop();
mkdirSync(out, { recursive: true });
writeFileSync(join(out, "long-paths.json"), JSON.stringify(results, null, 2));
console.log(`${results.length - failed} of ${results.length} screens hold their paths`);
process.exit(failed === 0 ? 0 : 1);
