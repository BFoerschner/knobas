// Long filesystem paths held to their boxes, in headless Chrome against a built app under ?fake-ipc (the clone-path pass, PR #2479,
// 2026-11-16: Björn's "cloned at C:\x\y\z\foo\bar can get long in certain places and go out of bounds").
//
// Usage:
//   testenv/ui-checks/build-app.sh . <folder>
//   testenv/ui-checks/with-built.sh <folder> <port> node testenv/ui-checks/long-paths.mjs <out dir> --port <port> [--only a,b]
//
// Each screen below opens with `?fake-long-paths`, which makes every path the fixture draws a long Windows one with spaces (a clone,
// the clones root, git, a pull request's worktree), at each size a person uses: 5120x1440@1, 2560x1440@1.25, 1920x1080@2 and the
// window's smallest, 1100x600. It fails when:
//   - a run of a path's text reaches past the window, or past any box that holds it, cut or spilling, unless it is the start of a
//     path drawn by `TfPath` (`.tf-path-head`), which gives up its middle for the folder at its end, whole path on hover;
//   - an ellipsis cuts a path, other than that start;
//   - a box scrolls sideways with the long paths and did not with the fixture's short ones.
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
const REPO = "repo/mock%3Atidewater%2Fpayout-service";
// `press`: the control to press first, by its words or a selector; `focus`: the control to focus (the sign's bubble opens on it).
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
];

// Runs in the page: what of a path's text leaves its boxes, and which boxes scroll sideways.
const CHECK = String.raw`(() => {
  const MARK = /folders with spaces|repository-name-that-is-long|Git for Windows|\/Users\/mara\/src/;
  const roots = [document.querySelector(".tf-kept:not([hidden])") ?? document.querySelector(".tf-main") ?? document.body, document.querySelector(".tf-floats")].filter(Boolean);
  const name = (el) => el.tagName.toLowerCase() + [...el.classList].map((c) => "." + c).join("");
  const where = (el) => { const parts = []; for (let e = el; e && e !== document.body && parts.length < 4; e = e.parentElement) parts.unshift(name(e)); return parts.join(" > "); };
  const misses = [];
  for (const root of roots) {
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
    for (let node = walker.nextNode(); node; node = walker.nextNode()) {
      if (!MARK.test(node.textContent)) continue;
      const range = document.createRange();
      range.selectNodeContents(node);
      const rects = [...range.getClientRects()].filter((r) => r.width > 0);
      if (rects.length === 0) continue;
      let hidden = false, cutOnPurpose = false;
      for (let e = node.parentElement; e; e = e.parentElement) {
        const s = getComputedStyle(e);
        if (s.clipPath !== "none" || s.visibility === "hidden" || s.display === "none") hidden = true;
        if (e.classList.contains("tf-path-head")) cutOnPurpose = true;
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
    if (e.clientWidth === 0 || e.scrollWidth <= e.clientWidth + 1 || !MARK.test(e.textContent)) continue;
    const s = getComputedStyle(e);
    if (s.textOverflow !== "ellipsis" || e.classList.contains("tf-path-head") || e.closest(".tf-sr")) continue;
    misses.push({ kind: "cut", at: where(e), text: e.textContent.trim().slice(-48), over: e.scrollWidth - e.clientWidth });
  }
  const scrolls = [];
  for (const root of roots) for (const e of [root, ...root.querySelectorAll("*")]) {
    if (e.clientWidth === 0 || e.scrollWidth <= e.clientWidth + 1) continue;
    const s = getComputedStyle(e);
    // A clip is no scroll: the kept page clips what passes under the scroller's 8 px bar on purpose (chrome.css).
    if (s.overflowX === "visible" || s.overflowX === "clip" || s.textOverflow === "ellipsis") continue;
    scrolls.push(where(e));
  }
  return JSON.stringify({ misses, scrolls, page: document.documentElement.scrollWidth > innerWidth + 1 });
})()`;

const PRESS = (words) => String.raw`(() => {
  const words = ${JSON.stringify(words)};
  const root = document.querySelector(".tf-kept:not([hidden])") ?? document.querySelector(".tf-main");
  const found = [...root.querySelectorAll("button, a")].find((e) => e.textContent.trim() === words && e.getBoundingClientRect().width > 0);
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
  for (const [width, height, scale] of SIZES) {
    await send("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: scale, mobile: false });
    const size = `${width}x${height}@${scale}`;
    const read = async (query) => {
      await opener.openScreen({ route: screen.route, query });
      let stepped = true;
      if (screen.press) stepped = await evaluate(PRESS(screen.press));
      if (screen.focus) stepped = await evaluate(`(() => { const e = document.querySelector(${JSON.stringify(screen.focus)}); if (!e) return false; e.focus(); return true; })()`);
      if (screen.hover) {
        const box = await evaluate(`(() => { const e = document.querySelector(${JSON.stringify(screen.hover)}); if (!e) return null; const b = e.getBoundingClientRect(); return { x: b.left + b.width / 2, y: b.top + b.height / 2 }; })()`);
        stepped = box !== null;
        if (box) await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: box.x, y: box.y });
        await sleep(600);
      }
      if (screen.press || screen.focus) await opener.settleAfterChange();
      return { stepped, ...JSON.parse(await evaluate(CHECK)) };
    };
    const short = await read("");
    const long = await read("&fake-long-paths");
    const scrolls = long.scrolls.filter((box) => !short.scrolls.includes(box));
    const misses = [
      ...(long.stepped ? [] : [{ kind: "step", at: screen.press ?? screen.focus ?? screen.hover, text: "found nothing to press" }]),
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
