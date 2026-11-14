// A file reopened after a sync moved its branch (the forge-waits pass, 2026-11-13): the live block saw the file read
// again from Gitea, 2.1 s for a large one, where the clone held the new commit. This times it on the Mac against
// `standin-forge.mjs --git <repo>`: the repository's branch on the stand-in, its clone a checkout knobas reads, and a
// windowless backend (`examples/headless_ipc.rs`) over a scratch profile whose Gitea source is the stand-in.
//
//   node testenv/perf/synced-file.mjs --app <url ?fake-ipc&fake-hooks&fake-real=<port>> --db <psql url> \
//     --git <stand-in repo> --clone <its clone, or none> --repo <entity id> --path <file> \
//     [--throttle 1,4] [--out file.json]
//
// Each round, at each throttle: the file is drawn at the branch's commit; then a sync moves the branch (a commit on
// the stand-in, fetched into the clone where there is one, and the mirror's tip moved to it, as a run writes it), and
//
// - `return`: the reader goes to Time and back to the file (the repository page is kept, and reads again);
// - `reopen`: the page is loaded afresh at the file, as after a restart.
//
// The windowless backend tells the page nothing, so the probe tells it what the app's would once its read behind the
// kept answer has landed: `view:fresh` for the tree, once the stand-in has answered the new commit's tree (its
// `--control` port's `/tree`), the read behind's last request, and 50 ms more. Asking the backend instead would start
// reads behind of its own. Timed from the step to the frame that shows the new commit's text, with the IPC calls between.
import { execFileSync } from "node:child_process";
import { writeFileSync } from "node:fs";
import { attach, launch, sleep } from "./cdp.mjs";

const args = process.argv.slice(2);
const opt = (name, fallback = null) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : fallback; };
const base = opt("--app");
const db = opt("--db");
const gitDir = opt("--git");
const clone = opt("--clone");
const repo = opt("--repo");
const path = opt("--path");
const rates = (opt("--throttle") ?? "1,4").split(",").map(Number);
const out = opt("--out");
const control = opt("--control", "3912");
if (!base || !db || !gitDir || !repo || !path) { console.log("synced-file: --app, --db, --git, --repo and --path are required"); process.exit(2); }

const git = (dir, ...argv) => execFileSync("git", ["-C", dir, "-c", "user.name=s", "-c", "user.email=s@s.invalid", ...argv]).toString().trim();
const psql = (sql) => execFileSync("psql", [db, "-Atc", sql]).toString().trim();
const fileHash = `#/repo/${encodeURIComponent(repo)}/files/${path.split("/").map(encodeURIComponent).join("/")}`;
let mark = 0;

/** A sync: the branch moves on the stand-in, the clone fetches it (where there is one), and the mirror's tip moves. */
function sync() {
  mark += 1;
  execFileSync("sh", ["-c", `printf '// synced %s\\n' "$1" >> "$2"`, "sh", String(mark), `${gitDir}/${path}`]);
  git(gitDir, "commit", "-qam", `sync ${mark}`);
  const sha = git(gitDir, "rev-parse", "HEAD");
  if (clone) git(clone, "fetch", "-q", "origin");
  psql(`update sync.item set payload = jsonb_set(payload, '{commit,id}', to_jsonb('${sha}'::text)), synced_at = now() where entity_id = '${repo}@refs/heads/main'`);
  return { text: `// synced ${mark}`, sha };
}

const browser = await launch();
const c = await attach(browser.port);
await c.send("Page.enable");
await c.send("Runtime.enable");
const js = c.js;
const cpu = (rate) => c.send("Emulation.setCPUThrottlingRate", { rate });

/** Resolves with the time (ms since the epoch) of the frame where the shown file holds `text`. */
const WAIT = (text) => `new Promise((resolve, reject) => {
  const give = setTimeout(() => reject(new Error("never drew ${text}")), 30000);
  const look = () => {
    const view = document.querySelector(".tf-kept:not([hidden]) .tf-fileview");
    if (view && view.textContent.includes(${JSON.stringify(text)}) && !view.querySelector("[aria-busy='true']")) {
      requestAnimationFrame(() => { clearTimeout(give); resolve(performance.timeOrigin + performance.now()); });
    } else requestAnimationFrame(look);
  };
  look();
})`;
const NOW = "(performance.timeOrigin + performance.now())";
const TAP = `(() => { window.__calls = []; window.__knobasIpcTap = (cmd, a, o, next) => { const at = ${NOW}; return next(cmd, a, o).finally(() => window.__calls.push([cmd, a?.section ?? a?.path ?? null, at, ${NOW}])); }; return 1; })()`;
// Every document taps the IPC, so a page loaded afresh is timed from its first call.
await c.send("Page.addScriptToEvaluateOnNewDocument", { source: TAP });
const TELL = `window.__fake.emit("view:fresh", { entity_id: ${JSON.stringify(repo)}, section: "files" }), 1`;

async function load(hash) {
  // Through a blank page: a navigation that changes only the hash fires no load.
  const blank = c.once("Page.loadEventFired");
  await c.send("Page.navigate", { url: "about:blank" });
  await blank;
  const loaded = c.once("Page.loadEventFired");
  await c.send("Page.navigate", { url: `${base.split("#")[0]}${hash}` });
  await loaded;
  for (let i = 0; i < 300; i++) { if (await js("!!document.querySelector('.tf-nav')").catch(() => false)) break; await sleep(50); }
}

/** Resolves once the stand-in answered `sha`'s tree, and the backend has had a moment to keep it: when the app's backend would tell `view:fresh`. */
async function moved(sha) {
  for (let i = 0; i < 600; i++) {
    const seen = await fetch(`http://127.0.0.1:${control}/tree?sha=${sha}`).then((r) => r.text()).catch(() => "");
    if (seen.startsWith("yes")) return sleep(50);
    await sleep(50);
  }
}

/** Starts `step`, tells the page the read behind landed, and times the new text. A step that loads the page looks once it has. */
async function timed(step, { text, sha }, loads = false) {
  await js(TAP);
  const t0 = Date.now();
  let drawn = loads ? null : js(WAIT(text)).catch((e) => String(e.message));
  await step();
  drawn ??= js(WAIT(text)).catch((e) => String(e.message));
  const told = moved(sha).then(() => js(TELL));
  const at = await drawn;
  await told;
  await sleep(500);
  const calls = await js("window.__calls ?? []").catch(() => []);
  return { ms: typeof at === "number" ? Math.round(at - t0) : null, error: typeof at === "number" ? null : at, calls: calls.filter(([cmd]) => /entity_(view|local)/.test(cmd)).map(([cmd, what, from, to]) => `${cmd}(${what ?? ""}) ${Math.round(from - t0)}..${Math.round(to - t0)}`) };
}

const results = [];
for (const rate of rates) {
  await cpu(1);
  await load(fileHash);
  await js(WAIT("line 399")).catch(() => {});
  await sleep(1500);
  await cpu(rate);
  // The return: away to Time, a sync, back to the file.
  await js(`location.hash = "#/time", 1`);
  await sleep(1200);
  const text = sync();
  const back = await timed(() => js(`location.hash = ${JSON.stringify(fileHash)}, 1`), text);
  console.log(`${rate}x return: ${back.ms ?? back.error} ms  ${back.calls.join(", ")}`);
  results.push({ rate, round: "return", ...back });
  // The reads behind the return's (the page's own, and the one its `view:fresh` starts) land before the next sync, as
  // they would in the app between two syncs minutes apart: one still out across the sync would be kept as read after it.
  await sleep(8000);
  // A page opened afresh after the next sync.
  await cpu(1);
  const next = sync();
  await cpu(rate);
  const reopen = await timed(() => load(fileHash), next, true);
  console.log(`${rate}x reopen: ${reopen.ms ?? reopen.error} ms  ${reopen.calls.join(", ")}`);
  results.push({ rate, round: "reopen", ...reopen });
  await sleep(1500);
}
await cpu(1);
if (out) writeFileSync(out, JSON.stringify(results, null, 1));
c.close();
browser.stop();
process.exit(0);
