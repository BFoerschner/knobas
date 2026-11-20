// Long filesystem paths held to their boxes, in headless Chrome against a built app under ?fake-ipc (the clone-path pass, PR #2479,
// 2026-11-16: Björn's "cloned at C:\x\y\z\foo\bar can get long in certain places and go out of bounds").
//
// Usage:
//   testenv/ui-checks/build-app.sh . <folder>
//   testenv/ui-checks/with-built.sh <folder> <port> node testenv/ui-checks/long-paths.mjs <out dir> --port <port> [--only a,b] [--shots]
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
if (!out || !port) {
  console.error("usage: long-paths.mjs <out dir> --port <port> [--only a,b]");
  process.exit(2);
}

const SIZES = [
  [5120, 1440, 1],
  [2048, 1152, 1.25],
  [960, 540, 2],
  [1100, 600, 1],
];
// The Builds screens add 1440x868, the size Björn's report came from (the build-header pass, 2026-11-20).
const BUILD_SIZES = [
  [5120, 1440, 1],
  [2048, 1152, 1.25],
  [1440, 868, 1],
  [1100, 600, 1],
];
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
  { id: "rerender-left-out", route: `${REPO}/rendered/site/docs`, fake: "&fake-rerender", wait: 4500, press: "2 left out" },
  { id: "rerender-open-in", route: `${REPO}/rendered/site/docs`, fake: "&fake-rerender", wait: 2600, press: ".tf-madebar-open button" },
  { id: "settings-renderers", route: "settings/renderers" },
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
  // A source's site at a long address (ADR-0048): the host whole, the rest given up at its end, the whole in the title.
  { id: "site-bar", route: "site/confluence", press: "Open Tidewater Confluence", wait: 700, sizes: BUILD_SIZES },
];

// Runs in the page: what of a path's text leaves its boxes, and which boxes scroll sideways.
const CHECK = String.raw`(() => {
  const MARK = /folders with spaces|repository-name-that-is-long|Git for Windows|\/Users\/mara\/src|Orchestra Renderer/;
  // A renderer's long name: held to its boxes as a path is, but it may end in an ellipsis.
  const NAME = /payments team's/;
  const roots = [document.querySelector(".tf-kept:not([hidden])") ?? document.querySelector(".tf-main") ?? document.body, document.querySelector(".tf-floats"), document.querySelector(".tf-toasts"), document.querySelector(".tf-acks")].filter(Boolean);
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
      // A file's or a drawing's own text in the code view scrolls sideways as code does: it is not the app's.
      if (node.parentElement.closest(".tf-code")) continue;
      for (let e = node.parentElement; e; e = e.parentElement) {
        const s = getComputedStyle(e);
        if (s.clipPath !== "none" || s.visibility === "hidden" || s.display === "none") hidden = true;
        if (e.classList.contains("tf-path-head") || e.classList.contains("tf-toast-head") || e.classList.contains("tf-site-path")) cutOnPurpose = true;
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
    if (e.clientWidth === 0 || e.scrollWidth <= e.clientWidth || !MARK.test(e.textContent) || e.closest(".tf-code")) continue;
    const s = getComputedStyle(e);
    if (s.textOverflow !== "ellipsis" || e.classList.contains("tf-path-head") || e.classList.contains("tf-toast-head") || e.classList.contains("tf-site-path") || e.closest(".tf-sr")) continue;
    misses.push({ kind: "cut", at: where(e), text: e.textContent.trim().slice(-48), over: e.scrollWidth - e.clientWidth });
  }
  const scrolls = [];
  for (const root of roots) for (const e of [root, ...root.querySelectorAll("*")]) {
    if (e.clientWidth === 0 || e.scrollWidth <= e.clientWidth + 1) continue;
    const s = getComputedStyle(e);
    // A clip is no scroll: the kept page clips what passes under the scroller's 8 px bar on purpose (chrome.css).
    if (s.overflowX === "visible" || s.overflowX === "clip" || s.textOverflow === "ellipsis" || e.closest(".tf-code")) continue;
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
  const shown = (e) => e.getBoundingClientRect().width > 0;
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

const chrome = await launch("knobas-long-paths");
const { send, evaluate } = chrome;
const opener = screenOpener(chrome, port);
await opener.install();
// Headless, the page has no focus of its own, so a focus() would fire no focus event and the sign's bubble would stay shut.
await send("Emulation.setFocusEmulationEnabled", { enabled: true });

const results = [];
let failed = 0;
for (const screen of SCREENS) {
  if (only && !only.some((word) => screen.id.includes(word))) continue;
  for (const [width, height, scale] of screen.sizes ?? SIZES) {
    await send("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: scale, mobile: false });
    const size = `${width}x${height}@${scale}`;
    const read = async (query) => {
      // `fake`: more of the fixture's switches the screen needs (a generated folder's `&fake-rerender`).
      await opener.openScreen({ route: screen.route, query: query + (screen.fake ?? "") });
      let stepped = true;
      if (screen.press) stepped = await evaluate(PRESS(screen.press));
      for (const next of screen.then ?? []) {
        if (!stepped) break;
        await opener.settleAfterChange();
        stepped = await evaluate(PRESS(next));
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
      if (screen.press || screen.focus || screen.context) await opener.settleAfterChange();
      return { stepped, ...JSON.parse(await evaluate(CHECK)) };
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
      ...(long.stepped ? [] : [{ kind: "step", at: [screen.press, ...(screen.then ?? []), screen.focus, screen.hover, screen.context?.join(" "), screen.item, screen.toast].filter(Boolean).join(" then "), text: "found nothing to press" }]),
      ...long.misses,
      ...scrolls.map((at) => ({ kind: "scrolls", at, text: "scrolls sideways only with the long paths" })),
      ...(long.page && !short.page ? [{ kind: "page", at: "html", text: "the page is wider than the window" }] : []),
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
