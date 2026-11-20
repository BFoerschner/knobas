// How trips.mjs walks its queue (the trip-limit pass, 2026-11-20). Twice that day a `trips.mjs --strict` run on the Mac sat
// 22 and 23 minutes with no output and flat CPU while it held the machine, and its log could not say which trip was stuck.
//
// So each trip gets a time limit (`limitMs`, `--trip-limit` seconds in trips.mjs). A trip past it is a failure that names
// the trip and the limit; its browser is stopped, a fresh one takes its place, and the worker goes on down the queue. A
// progress line every `everyMs` names what each worker is on and for how long, so a quiet run still says where it is.
//
// `startBrowser()` gives one browser: `{ run(trip) -> { reasons, note }, stop() }`. `log` takes one line.

/** Runs `queue` (emptied as it goes) across `workers` browsers into `results`. Resolves once every trip has a result. */
export async function runQueue({ queue, results, workers, limitMs, everyMs = 60000, startBrowser, log = console.log }) {
  const total = queue.length;
  const startedAt = Date.now();
  /** What each worker is on: its trip and when it began, or null between trips. */
  const on = Array.from({ length: workers }, () => null);
  const secs = (ms) => `${Math.round(ms / 1000)} s`;
  const progress = setInterval(() => {
    const now = Date.now();
    const busy = on.map((x, i) => (x ? `worker ${i + 1} on ${x.id} for ${secs(now - x.began)}` : `worker ${i + 1} idle`)).join(", ");
    log(`progress  ${results.length} of ${total} done after ${secs(now - startedAt)}: ${busy}`);
  }, everyMs);
  progress.unref?.();

  async function worker(slot) {
    let browser = null;
    try {
      for (let t = queue.shift(); t; t = queue.shift()) {
        const began = Date.now();
        on[slot] = { id: t.id, began };
        const base = { id: t.id, kind: t.kind, finding: t.finding, page: t.page, route: t.route };
        let timer;
        let timedOut = false;
        try {
          browser ??= await startBrowser();
          // The race also takes an abandoned trip's later rejection (its browser gone), so that reaches no one.
          const trip = browser.run(t);
          const limit = new Promise((resolve) => { timer = setTimeout(() => { timedOut = true; resolve(null); }, limitMs); });
          const done = await Promise.race([trip, limit]);
          if (timedOut) {
            results.push({ ...base, status: "fail", reasons: [`${t.id} ran past its ${secs(limitMs)} limit: its browser was stopped and a fresh one took the rest`], note: "", took: Date.now() - began, timedOut: true });
            const stuck = browser;
            browser = null;
            try { stuck.stop(); } catch {}
          } else {
            const { reasons, note } = done;
            results.push({ ...base, status: reasons.length ? "fail" : "pass", reasons, note, took: Date.now() - began });
          }
        } catch (err) {
          results.push({ ...base, status: "can't run", reasons: [String(err?.message ?? err).split("\n")[0]], note: "", took: Date.now() - began });
        } finally {
          clearTimeout(timer);
          on[slot] = null;
        }
        const last = results.at(-1);
        log(`${last.status.padEnd(9)} ${t.kind} ${t.id} ${(last.took / 1000).toFixed(1)} s${last.timedOut ? " (timed out)" : ""}`);
      }
    } finally {
      try { browser?.stop(); } catch {}
    }
  }

  try {
    await Promise.all(on.map((_, slot) => worker(slot)));
  } finally {
    clearInterval(progress);
  }
}
