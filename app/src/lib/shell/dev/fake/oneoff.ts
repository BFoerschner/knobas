/**
 * `?fake-ipc`: commands, run on a press and read in a dialog (ADR-0060). Answers `oneoff_commands`,
 * `set_oneoff_commands`, `oneoff_run`, `oneoff_stop`, `oneoff_runs`, `oneoff_lines`, `oneoff_watch`, `oneoff_forget`, `oneoff_save` and
 * `oneoff_shell_path`.
 *
 * None is saved until one is, as in the app. `?fake-oneoff` saves four: *ESLint* on any file, *ESLint* on a
 * folder, *xmllint* on an order (`*.xml`) and *Tests* on the repository. The fake runs no program: a run plays lines
 * back as the backend sends them, read already (colour runs, places, findings), in batches every 33 ms, the last one
 * carrying how it ended. A command holding `fail` exits 2 with stderr, one holding `missing` does not start, one holding
 * `sleep` runs until stopped or its timeout. `?fake-oneoff=flood` makes *Tests* print 120,000 lines at 20,000 a second
 * (the probes' flood); `?fake-oneoff-lines=<n>` sets how many.
 */
import type { OneOff, OneOffBatch, OneOffDraft, OneOffEnded, OneOffLine, OneOffRunRow, OneOffTarget } from "../../../ipc/oneoff";
import type { FakeBridge, Handlers } from "../fake-tauri";
import { keepSeed, onFakeReset } from "../fake-seed";

const atLoad = () => ({ bridge: null as FakeBridge | null, saved: [] as OneOff[], flood: false, floodLines: 120_000, nextRun: 0 });
let { bridge, saved, flood, floodLines, nextRun } = atLoad();
onFakeReset(() => {
  ({ bridge, saved, flood, floodLines, nextRun } = atLoad());
});

/** One run of the session. */
interface FakeRun {
  row: OneOffRunRow;
  lines: OneOffLine[];
  findings: number;
  bytes: number;
  stop: boolean;
  sinks: { onmessage: (batch: OneOffBatch) => void }[];
  started: number;
}
const runs: FakeRun[] = [];
keepSeed(runs);

const base = (id: string, name: string, command: string, files: string[] = []): OneOff => ({
  id,
  name,
  command,
  stdin: "none",
  files,
  folders: [],
  has: [],
  repo_holds: [],
  repos: [],
  shows: "auto",
  start_in: "root",
  path: [],
  env: [],
  colours: true,
  timeout_secs: 120,
  wsl_paths: false,
});

const SEEDED: OneOff[] = [
  base("eslint", "ESLint", "npx eslint --max-warnings 0 {file}"),
  base("eslint-dir", "ESLint on a folder", "npx eslint {dir}"),
  base("xmllint", "xmllint, against openTRANS", "xmllint --noout --schema schemas/opentrans_2_1.xsd {file}", ["*.xml"]),
  base("tests", "Tests", "npm test"),
];

export function seed(params: URLSearchParams): void {
  saved = params.has("fake-oneoff") ? SEEDED.map((c) => ({ ...c })) : [];
  flood = params.get("fake-oneoff") === "flood";
  floodLines = Number(params.get("fake-oneoff-lines") ?? 120_000) || 120_000;
  runs.length = 0;
}

export function attach(installed: FakeBridge): void {
  bridge = installed;
}

function line(n: number, text: string, extra: Partial<OneOffLine> = {}): OneOffLine {
  return { n, stream: "out", text, ...extra };
}

/** What `command` prints on `target`, read as the backend reads it. */
function script(command: OneOff, target: OneOffTarget): { lines: OneOffLine[]; ended: OneOffEnded; every: number } {
  const file = target.kind === "file" ? target.path : "src/retry/window.ts";
  if (command.command.includes("missing")) return { lines: [], ended: { how: "not_started", reason: `${command.command.split(" ")[0]} was not found: is it on the PATH the command sees?` }, every: 0 };
  if (command.command.includes("fail")) {
    return {
      lines: [line(0, `Error: Cannot find module 'eslint.config.js'`, { stream: "err", runs: [{ from: 0, to: 6, style: { fg: "r", bold: true } }] }), line(1, "    at Module._resolveFilename (node:internal/modules/cjs/loader:1249:15)", { stream: "err" })],
      ended: { how: "exit", code: 2 },
      every: 40,
    };
  }
  if (command.id === "xmllint") {
    const rows: [number, string, string][] = [
      [118, "Element '{urn:bmecat:2005}ORDER_UNIT': [facet 'enumeration'] The value 'PCE' is not an element of the set {'C62', 'KGM', 'MTR'}.", "ORDER_UNIT"],
      [212, "Element 'QUANTITY': 'zwölf' is not a valid value of the atomic type 'xs:decimal'.", "QUANTITY"],
      [305, "Element 'DELIVERY_DATE': Missing child element(s). Expected is ( DELIVERY_START_DATE ).", "DELIVERY_DATE"],
    ];
    const out = rows.map(([at, said, rule], n) => {
      const head = `${file}:${at}`;
      return line(n, `${head}: Schemas validity error : ${said}`, { stream: "err", places: [{ from: 0, to: head.length, path: file, line: at, col: null }], finding: { severity: "error", said, rule } });
    });
    out.push(line(out.length, `${file} fails to validate`, { stream: "err" }));
    return { lines: out, ended: { how: "exit", code: 3 }, every: 120 };
  }
  if (command.id === "tests" && flood) {
    // Made as they are sent, as a backend reads them: the page never holds the whole run before it is printed.
    const lines = new Proxy([] as OneOffLine[], {
      get: (target, key) => {
        if (key === "length") return floodLines;
        if (key === "slice") return (a: number, b: number) => Array.from({ length: Math.max(0, Math.min(b, floodLines) - a) }, (_, i) => floodLine(a + i));
        return Reflect.get(target, key);
      },
    });
    return { lines, ended: { how: "exit", code: 0 }, every: 0 };
  }
  if (command.id === "tests") {
    const names = ["window", "backoff", "ledger", "sepa", "idempotency", "payout"];
    const lines = names.map((name, n) => line(n, ` ✓ src/retry/${name}.test.ts (${4 + n} tests) ${120 + n * 37}ms`, { runs: [{ from: 1, to: 2, style: { fg: "g" } }] }));
    lines.push(line(lines.length, ""), line(lines.length + 1, " Test Files  6 passed (6)", { runs: [{ from: 13, to: 19, style: { fg: "g", bold: true } }] }));
    return { lines, ended: { how: "exit", code: 0 }, every: 160 };
  }
  if (command.command.includes("sleep")) return { lines: [line(0, "waiting…")], ended: { how: "timed_out", seconds: command.timeout_secs }, every: 1e9 };
  const shown = target.kind === "folder" ? target.path || "." : file;
  const findings: [number, number, string, string, string][] = [
    [12, 7, "error", "'attempts' is never reassigned. Use 'const' instead", "prefer-const"],
    [27, 21, "warning", "Unexpected any. Specify a different type", "@typescript-eslint/no-explicit-any"],
    [48, 5, "error", "Promises must be awaited, end with a call to .catch, or be explicitly marked as ignored with the `void` operator", "@typescript-eslint/no-floating-promises"],
    [63, 11, "error", "'jitter' is assigned a value but never used", "@typescript-eslint/no-unused-vars"],
  ];
  const head = target.kind === "folder" ? `${shown}/window.ts` : shown;
  const lines: OneOffLine[] = [line(0, ""), line(1, head, { places: [{ from: 0, to: head.length, path: head, line: null, col: null }], runs: [{ from: 0, to: head.length, style: { underline: true } }] })];
  for (const [at, col, severity, said, rule] of findings) {
    const pos = `${at}:${col}`;
    const text = `  ${pos.padEnd(6)}  ${severity.padEnd(7)}  ${said}  ${rule}`;
    lines.push(line(lines.length, text, { places: [{ from: 2, to: 2 + pos.length, path: head, line: at, col }], finding: { severity, said, rule }, runs: [{ from: 2, to: 2 + pos.length, style: { dim: true } }, { from: 10, to: 10 + severity.length, style: { fg: severity === "error" ? "r" : "y" } }] }));
  }
  lines.push(line(lines.length, ""), line(lines.length + 1, "✖ 4 problems (3 errors, 1 warning)", { runs: [{ from: 0, to: 34, style: { fg: "r", bold: true } }] }));
  return { lines, ended: { how: "exit", code: 1 }, every: 90 };
}

/** The flood's line `n`. */
function floodLine(n: number): OneOffLine {
  return line(n, `  ✓ src/retry/window.test.ts > case ${n} keeps the payout held (${(n % 7) + 1} ms)`, { runs: [{ from: 2, to: 3, style: { fg: "g" } }] });
}

function batch(run: FakeRun, lines: OneOffLine[], ended: OneOffEnded | null): OneOffBatch {
  return {
    run: run.row.id,
    lines,
    changed: [],
    total: run.lines.length,
    dropped: 0,
    tail_from: run.lines.length,
    bytes: run.bytes,
    findings: run.findings,
    elapsed_ms: Date.now() - run.started,
    ended,
  };
}

function send(run: FakeRun, b: OneOffBatch) {
  for (const sink of run.sinks) sink.onmessage(b);
}

function end(run: FakeRun, ended: OneOffEnded) {
  if (run.row.ended) return;
  run.row.ended = ended;
  run.row.duration_ms = Date.now() - run.started;
  run.row.lines = run.lines.length;
  run.row.findings = run.findings;
  send(run, batch(run, [], ended));
  run.sinks = [];
  bridge?.emit("oneoff:moved", run.row);
}

/** Plays `lines` back: one a beat, or, with no beat, as a flood at 20,000 lines a second. */
function play(run: FakeRun, lines: OneOffLine[], ended: OneOffEnded, every: number) {
  if (ended.how === "not_started") {
    setTimeout(() => end(run, ended), 0);
    return;
  }
  let at = 0;
  const step = every === 0 ? 33 : every;
  const per = every === 0 ? 660 : 1;
  const tick = () => {
    if (run.stop) return end(run, { how: "stopped" });
    const next = lines.slice(at, at + per);
    at += next.length;
    for (const l of next) {
      run.lines.push(l);
      run.bytes += l.text.length + 1;
      if (l.finding) run.findings += 1;
    }
    if (next.length > 0) send(run, batch(run, next, null));
    if (at >= lines.length && ended.how !== "timed_out") return end(run, ended);
    setTimeout(tick, step);
  };
  setTimeout(tick, step);
}

export function handlers(): Handlers {
  return {
    oneoff_commands: () => saved,
    set_oneoff_commands: (args) => {
      const drafts = args["drafts"] as OneOffDraft[];
      saved = drafts.map((d, i) => ({
        ...base(d.id?.trim() || d.name.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || `command-${i + 1}`, d.name.trim(), d.command.trim()),
        ...d,
        id: d.id?.trim() || d.name.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || `command-${i + 1}`,
        colours: d.colours ?? true,
        timeout_secs: d.timeout_secs || 120,
      }));
      return saved;
    },
    oneoff_run: (args) => {
      const command = saved.find((c) => c.id === args["command"]);
      if (!command) throw { code: "invalid", message: `'${String(args["command"])}' is not one of your commands: see Settings › Commands`, source_id: null };
      const target = args["target"] as OneOffTarget;
      const running = runs.filter((r) => r.row.ended === null || r.row.ended === undefined);
      if (running.length >= 4) throw { code: "invalid", message: `four commands are running (${running.map((r) => r.row.name).join(", ")}): stop one or wait for it to end`, source_id: null };
      const id = `fake-run-${++nextRun}`;
      const argv = command.command.split(/\s+/).map((w) => w.replace("{file}", `/Users/mara/code/payout-service/${target.kind === "file" ? target.path : ""}`).replace("{dir}", `/Users/mara/code/payout-service/${target.kind === "folder" ? target.path : ""}`));
      const run: FakeRun = {
        row: { id, command: command.id, name: command.name, entity_id: String(args["entityId"]), target, argv, cwd: "/Users/mara/code/payout-service", branch: "feature/retry-window", head: "7d3f2a1c", started_at: Date.now(), duration_ms: null, ended: null, lines: 0, findings: 0, kept: true },
        lines: [],
        findings: 0,
        bytes: 0,
        stop: false,
        sinks: [],
        started: Date.now(),
      };
      const channel = args["channel"] as { onmessage?: (b: OneOffBatch) => void } | undefined;
      if (channel && typeof channel.onmessage === "function") run.sinks.push(channel as { onmessage: (b: OneOffBatch) => void });
      runs.push(run);
      bridge?.emit("oneoff:moved", run.row);
      const { lines, ended, every } = script(command, target);
      play(run, lines, ended, every);
      return { run: id, argv, cwd: run.row.cwd, branch: run.row.branch ?? null, head: run.row.head ?? null, started_at: run.started, already: false };
    },
    oneoff_stop: (args) => {
      const run = runs.find((r) => r.row.id === args["run"]);
      if (run) run.stop = true;
      if (run && run.row.ended === null) setTimeout(() => end(run, { how: "stopped" }), 0);
    },
    oneoff_runs: (args) => {
      const of = (args["entityId"] as string | null) ?? null;
      const all = [...runs].reverse().map((r) => ({ ...r.row, lines: r.lines.length, findings: r.findings }));
      const sorted = [...all.filter((r) => r.ended === null), ...all.filter((r) => r.ended !== null)];
      return of === null ? sorted : sorted.filter((r) => r.entity_id === of);
    },
    oneoff_lines: (args) => {
      const run = runs.find((r) => r.row.id === args["run"]);
      if (!run) return null;
      const from = Number(args["from"]);
      const count = Math.min(Number(args["count"]), 20_000);
      return batch(run, run.lines.slice(from, from + count), run.row.ended ?? null);
    },
    oneoff_watch: (args) => {
      const run = runs.find((r) => r.row.id === args["run"]);
      if (!run) return false;
      const channel = args["channel"] as { onmessage?: (b: OneOffBatch) => void } | undefined;
      if ((run.row.ended === null || run.row.ended === undefined) && channel && typeof channel.onmessage === "function") run.sinks.push(channel as { onmessage: (b: OneOffBatch) => void });
      return true;
    },
    oneoff_forget: (args) => {
      const id = (args["run"] as string | null) ?? null;
      const keep = runs.filter((r) => r.row.ended === null || r.row.ended === undefined || (id !== null && r.row.id !== id));
      runs.splice(0, runs.length, ...keep);
    },
    oneoff_save: (args) => `/Users/mara/Downloads/${String(args["name"])}`,
    oneoff_shell_path: (args) => ({
      on: typeof args["on"] === "boolean" ? (args["on"] as boolean) : true,
      offered: true,
      read: args["again"] === true ? { shell: "/bin/zsh", path: "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin", folders: 4, read_at: Date.now(), failed: null } : null,
    }),
  };
}
