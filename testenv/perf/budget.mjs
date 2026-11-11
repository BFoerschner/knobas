// The frame budget: reads a run of run.mjs and fails when a list, a key or a page switch is over its line in
// budget.json. testenv/perf/README.md says where the lines come from.
//
//   node testenv/perf/budget.mjs <run.json> [--lines fake|real] [--budget testenv/perf/budget.json] [--md <file.md>]
//
// Exit 0 when everything is within its line, 1 when something is over, 2 when a scenario the run tried has no measure
// (a list that could not be scrolled fails, not skips: a page that stopped drawing must not pass). A scenario the run
// did not try (`--only`) is left out.
import { readFileSync, writeFileSync } from "node:fs";

const args = process.argv.slice(2);
const opt = (name, fallback = null) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : fallback; };
const run = JSON.parse(readFileSync(args[0], "utf8"));
// The lines for the corpus the run drew: "fake" (`?fake-scale`) or "real" (the saved scale database).
const budget = JSON.parse(readFileSync(opt("--budget") ?? new URL("./budget.json", import.meta.url), "utf8"))[opt("--lines") ?? "fake"];
const rows = [];
let over = 0;
let missing = 0;
const check = (what, rate, measure, got, line, worse) => {
  if (line === undefined) return;
  const bad = got === undefined || got === null || (worse === "below" ? got < line : got > line);
  if (got === undefined || got === null) missing++;
  else if (bad) over++;
  rows.push({ what, rate, measure, got: got ?? "missing", line: `${worse === "below" ? "≥" : "≤"} ${line}`, ok: !bad });
};

for (const [name, lines] of Object.entries(budget.scroll ?? {})) {
  for (const [rate, line] of Object.entries(lines)) {
    const r = run.scroll.find((e) => e.name === name && e.throttle === Number(rate));
    if (!r) continue;
    const ok = !r.error && !r.skipped;
    check(name, rate, "rAF fps", ok ? r.raf.fps : null, line.fps, "below");
    check(name, rate, "frame work p50 ms", ok ? r.raf.work.p50 : null, line.workP50, "above");
    check(name, rate, "frame work p99 ms", ok ? r.raf.work.p99 : null, line.workP99, "above");
    check(name, rate, "elements a style pass", ok ? r.style.elements.max : null, line.styleElements, "above");
    check(name, rate, "longest task ms", ok ? r.longest.maxMs : null, line.longestTask, "above");
  }
}
for (const [name, lines] of Object.entries(budget.wheel ?? {})) {
  for (const [rate, line] of Object.entries(lines)) {
    const r = run.wheel.find((e) => e.name === name && e.throttle === Number(rate));
    if (!r) continue;
    const ok = !r.error && !r.skipped;
    check(`${name} (wheel)`, rate, "longest main-thread task ms", ok ? r.longest.maxMs : null, line.longestTask, "above");
    check(`${name} (wheel)`, rate, "moved fps", ok ? r.shown.movedFps : null, line.movedFps, "below");
  }
}
for (const [name, lines] of Object.entries(budget.type ?? {})) {
  for (const [rate, line] of Object.entries(lines)) {
    const r = run.type.find((e) => e.name === name && e.throttle === Number(rate));
    if (!r) continue;
    check(name, rate, "slowest key to paint ms", !r.error ? r.maxKeyMs : null, line.maxKeyMs, "above");
  }
}
for (const [hash, lines] of Object.entries(budget.nav ?? {})) {
  for (const [rate, line] of Object.entries(lines)) {
    for (const round of ["first", "return"]) {
      const r = run.nav.find((e) => e.hash === hash && e.round === round && e.throttle === Number(rate));
      if (!r) continue;
      check(`${hash} (${round})`, rate, "switch ms", !r.error ? r.switchMs : null, line.switchMs, "above");
    }
  }
}

const table = ["| What | Throttle | Measure | Got | Line | |", "|---|---|---|---|---|---|", ...rows.map((r) => `| ${r.what} | ${r.rate}x | ${r.measure} | ${r.got} | ${r.line} | ${r.ok ? "ok" : "**over**"} |`)].join("\n");
console.log(table);
if (opt("--md")) writeFileSync(opt("--md"), table + "\n");
console.log(`\nframe budget: ${rows.length - over - missing} within, ${over} over, ${missing} missing (${run.label}, ${run.ua?.match(/(Headless)?Chrome\/[\d.]+|Edg\/[\d.]+/)?.[0] ?? "?"})`);
process.exit(missing ? 2 : over ? 1 : 0);
