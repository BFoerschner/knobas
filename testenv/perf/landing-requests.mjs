// What a repository's landing asks the forge on its first open (the landing-rounds pass, 2026-11-16): opens the app on
// another page in a headless Chrome on the windowless backend, then the repository's landing, and prints the requests
// the stand-in forge answered for it (`standin-forge.mjs --log`'s lines, read from `--log-file`) in the order they
// came, with when each arrived after the press.
//
//   node testenv/perf/landing-requests.mjs --app "http://127.0.0.1:5980/?fake-ipc&fake-real=<port>#/time"
//        --repo <entity id> --log-file <stand-in log> [--wait 6000]
import { readFileSync } from "node:fs";
import { attach, launch, sleep } from "./cdp.mjs";

const args = process.argv.slice(2);
const opt = (name, fallback = null) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : fallback; };
const app = opt("--app");
const repo = opt("--repo");
const logFile = opt("--log-file");
const wait = Number(opt("--wait", "6000"));
if (!app || !repo || !logFile) { console.log("landing-requests: --app, --repo and --log-file are required"); process.exit(2); }
setTimeout(() => { console.log("watchdog: gave up"); process.exit(3); }, 120000).unref();

const browser = await launch();
const c = await attach(browser.port);
const loaded = c.once("Page.loadEventFired");
await c.send("Page.enable");
await c.send("Page.navigate", { url: app });
await loaded;
for (let i = 0; i < 300; i++) { if (await c.js("!!document.querySelector('.tf-nav')").catch(() => false)) break; await sleep(100); }
await sleep(3000);
const before = readFileSync(logFile, "utf8").split("\n").length;
const pressed = Date.now();
await c.js(`location.hash = ${JSON.stringify(`#/repo/${encodeURIComponent(repo)}`)}, 1`);
await sleep(wait);
const lines = readFileSync(logFile, "utf8").split("\n").slice(before - 1).filter((l) => /^(GET|POST) /.test(l));
for (const line of lines) console.log(line);
console.log(`landing-requests: ${lines.length} requests in ${wait} ms after the press (${Date.now() - pressed} ms)`);
await browser.close?.();
process.exit(0);
