// The probes' watchdog (cdp.mjs, the probe-watchdog pass, 2026-11-18): against `fake-chrome.mjs`, a DevTools endpoint
// that leaves a chosen call unanswered, dies on one, or never opens its socket, every wait ends, a case is tried once
// more on a fresh page, then reported failed-to-run, and each gate probe goes on and reports instead of hanging.
// Every limit runs at a hundredth (`KNOBAS_PERF_WATCHDOG_SCALE`): a 30 s call limit is 300 ms here. The launch and
// connect limits stay 60 s (`KNOBAS_PERF_START_MS`, the probes this file starts inherit it): starting the fake Chrome
// and its first page list are a process start, which a loaded machine stretches past 300 and 150 ms (#2546); the case
// of a launch and a socket that stall sets its own.
//
//   node --test testenv/perf/test/        (`testenv/perf/watchdog-test.sh`, in `just check-scripts`)
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { after, afterEach, describe, test } from "node:test";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const perf = dirname(here);
const FAKE = join(here, "fake-chrome.mjs");
process.env.KNOBAS_CHROME = FAKE;
process.env.KNOBAS_PERF_WATCHDOG_SCALE = "0.01";
process.env.KNOBAS_PERF_START_MS = "60000";
const cdp = await import("../cdp.mjs");
const { attach, launch, runCase, watch, sleep, isStalled, LIMITS } = cdp;

const scratch = mkdtempSync(join(tmpdir(), "knobas-watchdog-test-"));
after(() => rmSync(scratch, { recursive: true, force: true }));
/** The fakes a test started, stopped after it whatever its outcome, so a failed test leaves no socket open. */
const live = [];
afterEach(() => { for (const b of live.splice(0)) b.stop(); });
const start = async () => { const b = await launch(); live.push(b); return b; };
let n = 0;
/** A fresh state folder for the fake, and its env set for the next launch. */
function fake(env = {}) {
  const state = join(scratch, `s${++n}`);
  for (const k of Object.keys(process.env)) if (k.startsWith("FAKE_CDP_")) delete process.env[k];
  Object.assign(process.env, { FAKE_CDP_STATE: state }, env);
  mkdirSync(state, { recursive: true });
  return { state, lines: (file) => (existsSync(join(state, file)) ? readFileSync(join(state, file), "utf8").trim().split("\n").filter(Boolean) : []) };
}

/** `p`'s outcome, and how long it took. */
async function timed(p) { const t0 = Date.now(); try { return { value: await p, ms: Date.now() - t0 }; } catch (error) { return { error, ms: Date.now() - t0 }; } }

describe("the DevTools client's deadlines", () => {
  test("a command that never answers ends in a stall naming the probe, size, throttle, case and call", async () => {
    const f = fake({ FAKE_CDP_HANG: "Page.navigate" });
    const b = await start();
    const c = await attach(b.port);
    watch({ size: "1920x1080@2", throttle: 4, case: "tickets-letter" });
    const r = await timed(c.send("Page.navigate", { url: "about:blank" }));
    assert.ok(isStalled(r.error), String(r.error));
    assert.match(r.error.message, /watchdog\.test\.mjs 1920x1080@2 4x tickets-letter: Page\.navigate got no answer in 300 ms/);
    assert.ok(r.ms >= 250 && r.ms < 2000, `${r.ms} ms`);
    watch({ throttle: null, case: null });
    b.stop();
  });

  test("an event that never comes, and an evaluate whose promise never settles, end too", async () => {
    const f = fake({ FAKE_CDP_HANG: "Page.loadEventFired", FAKE_CDP_HANG_MATCH: "never" });
    const b = await start();
    const c = await attach(b.port);
    const loaded = c.once("Page.loadEventFired");
    await c.send("Page.navigate", { url: "about:blank" });
    const r = await timed(loaded);
    assert.match(String(r.error?.message), /Page\.loadEventFired did not come in 600 ms/);
    const e = await timed(c.js("new Promise(() => {}) /* never */", { timeout: 20000 }));
    assert.match(String(e.error?.message), /Runtime\.evaluate got no answer in 350 ms/);
    b.stop();
  });

  test("a Chrome that dies fails every waiting call at once", async () => {
    const f = fake({ FAKE_CDP_DIE: "Page.reload", FAKE_CDP_HANG: "Page.navigate" });
    const b = await start();
    const c = await attach(b.port);
    const waiting = timed(c.send("Page.navigate", { url: "about:blank" }));
    await sleep(20);
    void c.send("Page.reload").catch(() => {});
    const r = await waiting;
    assert.ok(isStalled(r.error), String(r.error));
    assert.match(r.error.message, /socket closed|Chrome died/);
    assert.ok(r.ms < 250, `failed at once, not at the deadline: ${r.ms} ms`);
    b.stop();
  });

  test("a socket that never opens, and a Chrome that never writes its port, end in stalls", async () => {
    const f = fake({ FAKE_CDP_NO_UPGRADE: "1" });
    const b = await start();
    const { launch: launchLimit, connect: connectLimit } = LIMITS;
    // A socket that never opens and a Chrome that never writes its port fail at their limits however fast it started.
    // The connect limit also covers finding the page on `/json/list`, which a loaded machine took past 150 ms
    // (`no matching page … in 150 ms`, the #2603 gate): 1 s finds the page and still fails at the socket.
    LIMITS.connect = 1000;
    const r = await timed(attach(b.port)).finally(() => { LIMITS.connect = connectLimit; });
    assert.match(String(r.error?.message), /DevTools socket .* did not open in 1000 ms/);
    b.stop();
    const g = fake({ FAKE_CDP_NO_PORT: "1" });
    LIMITS.launch = 300;
    const l = await timed(launch()).finally(() => { LIMITS.launch = launchLimit; });
    assert.match(String(l.error?.message), /wrote no DevToolsActivePort in 300 ms/);
  });
});

describe("runCase", () => {
  test("a stalled case is tried once more on a fresh page, with the page's setup made again", async () => {
    const f = fake({ FAKE_CDP_HANG: "Page.navigate", FAKE_CDP_HANG_TIMES: "1" });
    const b = await start();
    const c = await attach(b.port);
    await c.send("Emulation.setFocusEmulationEnabled", { enabled: true });
    const first = c.target.id;
    let prepared = 0;
    const r = await runCase(c, { name: "retry", throttle: 1, prepare: async () => { prepared++; } }, async (attempt) => {
      await c.send("Page.navigate", { url: "about:blank" });
      return attempt;
    });
    assert.deepEqual(r, { value: 2 });
    assert.equal(prepared, 1);
    assert.notEqual(c.target.id, first, "a new page");
    const calls = f.lines("calls.log").map((l) => l.split(" ")[1]);
    assert.equal(calls.filter((m) => m === "Emulation.setFocusEmulationEnabled").length, 2, "the setup made again on the fresh page");
    b.stop();
  });

  test("a case that stalls twice is failed-to-run, and the next case runs on a fresh page", async () => {
    const f = fake({ FAKE_CDP_HANG_MATCH: "case-one" });
    const b = await start();
    const c = await attach(b.port);
    const one = await timed(runCase(c, { name: "one", throttle: 4 }, () => c.js("'case-one'", { timeout: 5000 })));
    assert.equal(one.value.value, undefined);
    assert.match(one.value.failedToRun, /^Runtime\.evaluate got no answer in \d+ ms, twice$/);
    assert.ok(one.ms < 3000, `${one.ms} ms`);
    const two = await runCase(c, { name: "two", throttle: 4 }, () => c.js("'case-two'", { timeout: 5000 }));
    assert.deepEqual(two, { value: true });
    assert.equal(f.lines("hung.log").length, 2);
    b.stop();
  });

  test("a Chrome that died is started again and the case tried once more", async () => {
    const f = fake({ FAKE_CDP_DIE: "Page.reload" });
    const b = await start();
    const c = await attach(b.port);
    const port = b.port;
    const r = await runCase(c, { name: "died", throttle: 1 }, async (attempt) => { await c.send("Page.reload"); return attempt; });
    assert.deepEqual(r, { value: 2 });
    assert.equal(f.lines("launches.log").length, 2, "a second Chrome");
    assert.notEqual(b.port, port);
    b.stop();
  });

  test("a case past its wall-clock limit is given up, and its calls after that fail", async () => {
    const f = fake();
    const b = await start();
    const c = await attach(b.port);
    const late = [];
    const r = await timed(runCase(c, { name: "forever", throttle: 1, limit: 200 }, async () => {
      // A probe loop that never ends and swallows what it is told: only the case's own limit ends it.
      for (;;) { try { await c.send("Runtime.enable"); } catch (e) { late.push(e); } await sleep(20); }
    }));
    assert.match(r.value.failedToRun, /the case ran past its 200 ms limit, twice/);
    assert.ok(r.ms < 2000, `${r.ms} ms`);
    await sleep(100);
    assert.ok(late.every(isStalled), "the given-up attempts' calls fail");
    const before = f.lines("calls.log").length;
    await sleep(100);
    assert.equal(f.lines("calls.log").length, before, "and reach Chrome no more");
    b.stop();
  });

  test("a stall the probe caught still makes the attempt a stall", async () => {
    const f = fake({ FAKE_CDP_HANG: "Input.dispatchKeyEvent", FAKE_CDP_HANG_TIMES: "1" });
    const b = await start();
    const c = await attach(b.port);
    const errors = [];
    const r = await runCase(c, { name: "caught", throttle: 1 }, async (attempt) => {
      for (let i = 0; i < 3; i++) { try { await c.send("Input.dispatchKeyEvent", { type: "keyDown" }); } catch (e) { errors.push(e); } }
      return attempt;
    });
    assert.deepEqual(r, { value: 2 }, "tried again, not measured with a stalled press");
    assert.equal(errors.length, 3, "the stalled attempt's later calls failed at once");
    b.stop();
  });

  test("a probe's own error is thrown as before", async () => {
    const f = fake();
    const b = await start();
    const c = await attach(b.port);
    await assert.rejects(runCase(c, { name: "own" }, async () => { throw new Error("nothing to press"); }), /nothing to press/);
    b.stop();
  });
});

/**
 * Runs `probe` with `args` on the fake (with `env`), and its outcome: the exit code, stdout, stderr, how long it took.
 * A run that has not ended in `limit` ms is killed and fails the test.
 */
function probe(file, args, env, limit = 60000) {
  const state = join(scratch, `p${++n}`);
  mkdirSync(state, { recursive: true });
  const clean = Object.fromEntries(Object.entries(process.env).filter(([k]) => !k.startsWith("FAKE_CDP_")));
  return new Promise((resolve, reject) => {
    const t0 = Date.now();
    const p = spawn(process.execPath, [join(perf, file), ...args.map((a) => a.replaceAll("$STATE", state))], { env: { ...clean, FAKE_CDP_STATE: state, ...env }, stdio: ["ignore", "pipe", "pipe"] });
    let out = "", err = "";
    p.stdout.on("data", (d) => { out += d; });
    p.stderr.on("data", (d) => { err += d; });
    const timer = setTimeout(() => { p.kill("SIGKILL"); reject(new Error(`${file} still ran after ${limit} ms: it hung\n${out}\n${err}`)); }, limit);
    p.on("exit", (code) => { clearTimeout(timer); resolve({ code, out, err, ms: Date.now() - t0, state, json: (name) => JSON.parse(readFileSync(join(state, name), "utf8")) }); });
  });
}

// Each gate probe, its cases' first call never answered: each case is tried twice, reported failed-to-run, and the
// probe ends with every case it was asked for, and fails its check.
const HANG = { FAKE_CDP_HANG: "Emulation.setCPUThrottlingRate" };
// Failed to run after its first call stalled: tried twice, or, under a loaded gate, given up when the fresh page for the
// second try itself got no answer (`cdp.mjs`'s `then <call>`; seen as `then Page.enable got no answer in 300 ms`,
// PR #2614's gate). Either way the probe reported the case and went on.
const failedToRun = (said) => /(, twice|, then [\w.]+ got no answer in \d+ ms)$/.test(said ?? "");
describe("the gate probes report instead of hanging", { concurrency: true }, () => {
  test("keys.mjs", async () => {
    const r = await probe("keys.mjs", ["--app", "http://127.0.0.1:9/", "--out", "$STATE/keys.json", "--only", "tickets-letter,docs-spaces-letter", "--throttle", "1,4", "--size", "1920x1080@2", "--check", join(perf, "budget-keys.json")], HANG);
    assert.equal(r.code, 1, r.out + r.err);
    assert.equal((r.err.match(/trying the case once more on a fresh page/g) ?? []).length, 4);
    assert.match(r.err, /keys\.mjs 1920x1080@2 4x docs-spaces-letter: Emulation\.setCPUThrottlingRate got no answer in 300 ms/);
    const cases = r.json("keys.json").cases;
    assert.deepEqual(cases.map((x) => `${x.throttle}x ${x.name}`), ["1x tickets-letter", "1x docs-spaces-letter", "4x tickets-letter", "4x docs-spaces-letter"]);
    assert.ok(cases.every((x) => failedToRun(x.failedToRun)), JSON.stringify(cases));
    assert.match(r.out, /OVER FAILED TO RUN keys\.mjs 1920x1080@2 1x tickets-letter/);
    assert.match(r.out, /keys: 4 over their lines/);
  });

  test("open.mjs", async () => {
    const r = await probe("open.mjs", ["--app", "http://127.0.0.1:9/", "--out", "$STATE/open.json", "--only", "inbox,tickets-board", "--throttle", "1", "--no-big-folders", "--check", join(perf, "budget-open.json")], { ...HANG, FAKE_CDP_VALUE: "null" });
    assert.equal(r.code, 1, r.out + r.err);
    const places = r.json("open.json").places;
    assert.ok(places.length > 0 && places.every((x) => x.failedToRun), JSON.stringify(places).slice(0, 400));
    assert.match(r.out, /OVER FAILED TO RUN open\.mjs 1440x868@1 1x/);
  });

  test("moves.mjs", async () => {
    const r = await probe("moves.mjs", ["--app", "http://127.0.0.1:9/", "--out", "$STATE/moves.json", "--only", "tickets,inbox", "--throttle", "1", "--check"], { ...HANG, FAKE_CDP_VALUE: "null" });
    assert.equal(r.code, 1, r.out + r.err);
    const places = r.json("moves.json").places;
    assert.ok(places.length > 0 && places.every((x) => x.failedToRun), JSON.stringify(places).slice(0, 400));
    assert.match(r.out, /FAILED TO RUN moves\.mjs 1440x868@1 1x/);
  });

  test("run.mjs, and the frame budget it feeds", async () => {
    const r = await probe("run.mjs", ["--app", "http://127.0.0.1:9/", "--out", "$STATE/run.json", "--kinds", "type,nav", "--only", "code-filter,#/inbox", "--throttle", "4"], { ...HANG, FAKE_CDP_VALUE: '"x"' });
    assert.equal(r.code, 0, r.out + r.err);
    const run = r.json("run.json");
    assert.deepEqual([...run.type, ...run.nav].map((x) => [x.name ?? `${x.hash} ${x.round}`, failedToRun(x.failedToRun)]), [["code-filter", true], ["#/inbox first", true], ["#/inbox return", true]]);
    const budget = await new Promise((resolve) => {
      const p = spawn(process.execPath, [join(perf, "budget.mjs"), join(r.state, "run.json"), "--lines", "fake"], { stdio: ["ignore", "pipe", "pipe"] });
      let out = "";
      p.stdout.on("data", (d) => { out += d; });
      p.on("exit", (code) => resolve({ code, out }));
    });
    assert.equal(budget.code, 2, budget.out);
    assert.match(budget.out, /failed to run/);
  });
});

// The other probes with cases, the same way: each ends, says what stalled, and fails instead of hanging.
const A = "http://127.0.0.1:9/";
describe("the other probes report instead of hanging", { concurrency: true }, () => {
  for (const [file, args, env, expect] of [
    ["zoom.mjs", ["--app", A, "--out", "$STATE/o.json", "--throttle", "1", "--only", "tickets"], {}, /FAILED TO RUN zoom\.mjs 1440x868@1 1x tickets-list: .*, twice/],
    ["fonts.mjs", ["--app", A, "--out", "$STATE/o.json", "--throttle", "1", "--only", "tickets"], {}, /FAILED TO RUN fonts\.mjs 1440x868@1 1x tickets-list: .*, twice/],
    ["folders.mjs", ["--app", A, "--repo", "x", "--folders", "a/b", "--out", "$STATE/o.json", "--throttle", "1"], {}, /FAILED TO RUN folders\.mjs 1440x868@1 1x folders: .*, twice/],
    ["stored-trees.mjs", ["--app", A, "--repo", "x", "--deep", "a/b", "--find", "c", "--out", "$STATE/o.json", "--throttle", "1"], {}, /FAILED TO RUN stored-trees\.mjs .*, twice/],
    ["git-starts.mjs", ["--app", A, "--out", "$STATE/o.json", "--throttle", "1", "--shim-log", "/dev/null", "--only", "code"], {}, /FAILED TO RUN git-starts\.mjs 1440x868@1 1x code: .*, twice/],
    ["compare-moves.mjs", ["--app", A, "--repo", "x", "--out", "$STATE/o.json", "--throttle", "1", "--budget"], {}, /OVER FAILED TO RUN compare-moves\.mjs 1440x868@1 1x moves: .*, twice/],
    // A soak's lap gives up at the first stall: a fresh page would not read what the session grew.
    ["soak.mjs", ["--app", A, "--repo", "x", "--out", "$STATE/o.json", "--laps", "2"], { FAKE_CDP_HANG: "", FAKE_CDP_HANG_MATCH: "__soak.long.splice", FAKE_CDP_VALUE: "true" }, /FAILED TO RUN soak\.mjs 1440x868@1 lap 1: Runtime\.evaluate got no answer/],
  ]) {
    test(file, async () => {
      const r = await probe(file, args, { ...HANG, FAKE_CDP_VALUE: "null", ...env });
      assert.match(r.out, expect, r.out + r.err);
      assert.notEqual(r.code, 0, r.out + r.err);
      assert.match(JSON.stringify(r.json("o.json")), /"failedToRun"/);
    });
  }
});
