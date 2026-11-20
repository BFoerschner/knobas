// The press trips' time limit (`trip-runner.mjs`, the trip-limit pass, 2026-11-20): with stand-in browsers whose trips
// answer, throw or never answer, a stuck trip fails by name at its limit, its browser is stopped and a fresh one runs the
// rest, the run ends with a result for every trip, and the progress line names what each worker is on. No Chrome.
//
//   node --test testenv/ui-checks/test/        (`testenv/ui-checks/trip-runner-test.sh`, in `just witness-unit`)
import assert from "node:assert/strict";
import { test } from "node:test";
import { runQueue } from "../trip-runner.mjs";

const trip = (id, how = "pass") => ({ id, kind: "fake", finding: "F-0", page: "Fake", route: "fake", how });

/** Browsers whose trips act by their `how`: pass, fail, throw, or hang (a promise that never settles). */
function fakeBrowsers() {
  const made = [];
  const startBrowser = async () => {
    const b = { no: made.length + 1, ran: [], stopped: false };
    b.run = (t) => {
      b.ran.push(t.id);
      if (b.stopped) return Promise.reject(new Error("a trip ran on a stopped browser"));
      if (t.how === "hang") return new Promise(() => {});
      if (t.how === "hang-then-reject") return new Promise((_, reject) => setTimeout(() => reject(new Error("socket closed")), 80));
      if (t.how === "throw") return Promise.reject(new Error("no plain words in view"));
      return Promise.resolve({ reasons: t.how === "fail" ? ["it failed"] : [], note: "fine" });
    };
    b.stop = () => { b.stopped = true; };
    made.push(b);
    return b;
  };
  return { made, startBrowser };
}

test("a stuck trip fails by name at its limit, its browser is replaced, and the run goes on", async () => {
  const { made, startBrowser } = fakeBrowsers();
  const queue = [trip("a"), trip("stuck", "hang"), trip("b"), trip("c", "fail"), trip("d", "throw")];
  const results = [];
  const lines = [];
  const began = Date.now();
  await runQueue({ queue, results, workers: 1, limitMs: 50, startBrowser, log: (l) => lines.push(l) });
  assert.ok(Date.now() - began < 2000, "the run ended soon after the limit");
  const by = Object.fromEntries(results.map((r) => [r.id, r]));
  assert.deepEqual(results.map((r) => r.id), ["a", "stuck", "b", "c", "d"]);
  assert.equal(by.stuck.status, "fail");
  assert.equal(by.stuck.timedOut, true);
  assert.match(by.stuck.reasons[0], /^stuck ran past its 0 s limit/);
  assert.equal(by.a.status, "pass");
  assert.equal(by.b.status, "pass");
  assert.equal(by.c.status, "fail");
  assert.equal(by.d.status, "can't run");
  // The stuck trip's browser was stopped, a second one ran the rest, and it was stopped at the end.
  assert.equal(made.length, 2);
  assert.deepEqual(made[0].ran, ["a", "stuck"]);
  assert.deepEqual(made[1].ran, ["b", "c", "d"]);
  assert.ok(made.every((b) => b.stopped));
  assert.ok(lines.some((l) => /^fail\s+fake stuck .* \(timed out\)$/.test(l)), lines.join("\n"));
});

test("a stuck trip that rejects after its browser is gone neither crashes the run nor changes its result", async () => {
  const { startBrowser } = fakeBrowsers();
  const results = [];
  await runQueue({ queue: [trip("late", "hang-then-reject"), trip("after")], results, workers: 1, limitMs: 20, startBrowser, log: () => {} });
  await new Promise((r) => setTimeout(r, 150));
  assert.deepEqual(results.map((r) => [r.id, r.status]), [["late", "fail"], ["after", "pass"]]);
});

test("with two workers, one stuck trip holds up neither the other worker nor the run", async () => {
  const { made, startBrowser } = fakeBrowsers();
  const queue = [trip("stuck", "hang"), ...Array.from({ length: 6 }, (_, i) => trip(`t${i}`))];
  const results = [];
  await runQueue({ queue, results, workers: 2, limitMs: 60, startBrowser, log: () => {} });
  assert.equal(results.length, 7);
  assert.equal(results.filter((r) => r.status === "pass").length, 6);
  // The other worker took the whole rest of the queue while the stuck one waited out its limit, so no third browser started.
  assert.equal(made.length, 2);
  assert.deepEqual(made[1].ran, ["t0", "t1", "t2", "t3", "t4", "t5"]);
  assert.ok(made.every((b) => b.stopped));
});

test("the progress line names each worker's trip and how long it has been on it", async () => {
  const { startBrowser } = fakeBrowsers();
  const lines = [];
  await runQueue({ queue: [trip("stuck", "hang"), trip("next")], results: [], workers: 2, limitMs: 120, everyMs: 40, startBrowser, log: (l) => lines.push(l) });
  const progress = lines.filter((l) => l.startsWith("progress"));
  assert.ok(progress.length >= 1, lines.join("\n"));
  assert.match(progress[0], /^progress {2}\d+ of 2 done after \d+ s: worker 1 on stuck for \d+ s, worker 2 idle$/);
});

test("a browser that can't start makes its trips can't-run, named, and the run still ends", async () => {
  let starts = 0;
  const startBrowser = async () => { starts++; throw new Error("no Chrome page"); };
  const results = [];
  await runQueue({ queue: [trip("a"), trip("b")], results, workers: 1, limitMs: 50, startBrowser, log: () => {} });
  assert.deepEqual(results.map((r) => [r.id, r.status, r.reasons[0]]), [["a", "can't run", "no Chrome page"], ["b", "can't run", "no Chrome page"]]);
  assert.equal(starts, 2);
});
