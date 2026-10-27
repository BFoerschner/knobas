/**
 * `?fake-ipc`'s run page reads (#854, #855, #856): `run_log`, `run_tests`,
 * `test_history`, `run_artifacts`, `artifact_preview` and
 * `download_run_file`, over prototype-6's `logOf`, `stepsOf`, `testsOf`,
 * `artifactsOf` and `artifactText` for the fixture runs `fake/builds.ts`
 * draws.
 */
import type { ArtifactEntry, LogLine, LogStep, RunTest, TestOutcome } from "../../../ipc/entity";
import type { Handlers } from "../fake-tauri";
import { fixtureRun } from "./builds";

const REPOS = ["payout-service", "ledger-core", "psp-client", "fx-rates", "batch-runner", "notify-hub", "api-gateway"];

/** prototype-6's `testNames` and `testsOf`. */
const TEST_NAMES =
  "backs_off_exponentially jitter_within_bounds stops_on_permanent_error moves_to_manual_review gives_up_after_max_attempts retries_on_503 retries_on_temp_unavailable does_not_retry_rate_limited delay_caps_at_max batch_splits_by_currency batch_skips_zero_amounts batch_marks_sent client_sets_idempotency_key client_times_out_after_60s client_parses_error_codes errors_display_code ledger_books_payout ledger_books_fee_once ledger_reverses_failed ledger_rounds_half_even csv_header csv_escapes_commas schedule_runs_at_13 schedule_skips_holidays sandbox_health sandbox_resets iban_validates_checksum iban_rejects_short amount_parses_cents amount_rejects_negative currency_eur_only queue_orders_by_age queue_dedupes metrics_counts_attempts metrics_counts_giveups config_reads_env config_defaults config_rejects_unknown auth_signs_requests auth_rotates_key webhook_verifies webhook_ignores_replay".split(
    " ",
  );
const FAILING = "gives_up_after_max_attempts";
const DETAILS = "assertion `left == right` failed\n  left: 6\n right: 5\n  at tests/sepa_retry.rs:14:5";

function moduleOf(i: number): string {
  return i < 9 ? "sepa::retry" : i < 12 ? "sepa::batch" : i < 16 ? "psp::client" : i < 20 ? "ledger::booking" : "misc";
}

function testsOf(config: string, failed: boolean): RunTest[] {
  if (config !== "Payout_IntegrationTests") return [];
  return TEST_NAMES.map((n, i) => {
    const fails = failed && n === FAILING;
    return {
      id: String(4000 + i),
      name: `${moduleOf(i)}::${n}`,
      module: moduleOf(i),
      status: fails ? "failed" : "passed",
      muted: false,
      new_failure: false,
      duration_ms: 40 + ((i * 37) % 900),
      details: fails ? DETAILS : null,
    } satisfies RunTest;
  }).sort((a, b) => Number(b.status === "failed") - Number(a.status === "failed"));
}

/** prototype-6's `stepsOf`. */
function stepsOf(config: string, st: string, dur = 180): { n: string; d: number; st: "ok" | "err" | "run" | "wait" }[] {
  if (config === "Payout_IntegrationTests")
    return (
      [
        ["Checkout", 12],
        ["Start PSP sandbox stub", 20],
        ["cargo build --tests", 130],
        ["cargo test --test sepa_retry", 82],
        ["Publish test report", 8],
      ] as const
    ).map(([n, d], i) => ({ n, d, st: st === "failed" && i === 3 ? "err" : "ok" }));
  if (config === "Payout_Build" && st === "running")
    return (["Checkout", "cargo build --release", "cargo test"] as const).map((n, i) => ({ n, d: [14, 98, 60][i]!, st: i < 2 ? "ok" : "run" }));
  return (
    [
      ["Checkout", 0.06],
      ["Build", 0.55],
      ["Test", 0.3],
      ["Publish", 0.09],
    ] as const
  ).map(([n, p]) => ({ n, d: Math.round(dur * p), st: st === "failed" && n === "Test" ? "err" : "ok" }));
}

/** prototype-6's `logOf`, as `run_log` answers it: numbered lines in steps, and the steps over them. */
export function fakeLog(config: string, no: number, st: string): { lines: LogLine[]; steps: LogStep[]; total: number } {
  const lines: LogLine[] = [];
  let t = 10 * 3600 + 5 * 60 + 58;
  const stamp = () => [Math.floor(t / 3600), Math.floor(t / 60) % 60, t % 60].map((n) => String(n).padStart(2, "0")).join(":");
  const add = (step: number, text: string, tone: "err" | "warn" | null = null) => {
    lines.push({ number: lines.length + 1, time: stamp(), text, tone, step });
    t += 1 + (text.length % 3);
  };
  const steps = stepsOf(config, st);
  if (config !== "Payout_IntegrationTests" || no !== 1187) {
    steps.forEach((s, i) => {
      add(i, `Step ${i + 1}/${steps.length}: ${s.n}`);
      for (let j = 0; j < 6; j += 1)
        add(i, `  ${["Resolving", "Compiling", "Running", "Uploading", "Checking"][(i + j) % 5]} ${REPOS[(i * 7 + j + no) % REPOS.length]} ${j + 1}/6`);
      if (s.st === "err") add(i, "error: step failed with exit code 1", "err");
      if (s.st === "run") add(i, "  running …");
    });
  } else {
    ["Updating sources: server side checkout", "Repository: ssh://git.tidewater.example/tidewater/payout-service.git", "Revision: a41f2c (feature/PAY-231-sepa-retry)", "Done in 12 s"].forEach((x) => add(0, x));
    ["docker compose -f tests/sandbox.yml up -d", "Container psp-sandbox  Created", "Container psp-sandbox  Started", "waiting for http://localhost:9090/health ... ok"].forEach((x) => add(1, x));
    ["Compiling rand v0.8.5", "Compiling tokio v1.40.0", "Compiling reqwest v0.12.7", "Compiling thiserror v1.0.63", "Compiling payout-service v1.9.0-rc1 (/opt/agent/work/payout-service)"].forEach((x) => add(2, x));
    add(2, "warning: unused variable: `cut`", "warn");
    add(2, "  --> src/sepa/batch.rs:3:9", "warn");
    add(2, "Finished `test` profile [unoptimized + debuginfo] target(s) in 2m 08s");
    add(3, "Running tests/sepa_retry.rs (target/debug/deps/sepa_retry-3f1c9e)");
    add(3, "");
    add(3, "running 42 tests");
    testsOf(config, true)
      .sort((a, b) => Number(a.id) - Number(b.id))
      .forEach((x) => add(3, `test ${x.name} ... ${x.status === "failed" ? "FAILED" : "ok"}`, x.status === "failed" ? "err" : null));
    [
      "",
      "failures:",
      "",
      "---- sepa::retry::gives_up_after_max_attempts stdout ----",
      "thread 'sepa::retry::gives_up_after_max_attempts' panicked at tests/sepa_retry.rs:14:5:",
      "assertion `left == right` failed",
      "  left: 6",
      " right: 5",
      "note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace",
      "",
      "failures:",
      "    sepa::retry::gives_up_after_max_attempts",
      "",
      "test result: FAILED. 41 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 81.30s",
    ].forEach((x) => add(3, x, /panicked|assertion|left:|right:|FAILED\./.test(x) ? "err" : null));
    add(3, "error: test failed, to rerun pass `--test sepa_retry`", "err");
    add(3, "Process exited with code 101", "err");
    [
      "Publishing artifacts: target/nextest/junit.xml => test-report/junit.xml",
      "Publishing artifacts: target/report/index.html => test-report/index.html",
      "Publishing artifacts: logs/*.log => logs/",
      "Publishing artifacts: target/coverage/lcov.info => coverage/",
    ].forEach((x) => add(4, x));
  }
  const steps2: LogStep[] = steps.flatMap((s, i) => {
    const mine = lines.filter((l) => l.step === i);
    if (mine.length === 0) return [];
    return [{ name: s.n, first: mine[0]!.number, last: mine.at(-1)!.number, seconds: s.st === "run" ? null : s.d, tone: s.st === "err" ? "err" : mine.some((l) => l.tone === "warn") ? "warn" : null }];
  });
  return { lines, steps: steps2, total: lines.length };
}

/** prototype-6's `artifactsOf`, as folders and files. */
function artifactsOf(config: string, st: string): { d: string; f: [string, number][] }[] {
  if (st === "running" || st === "queued") return [];
  if (config === "Payout_IntegrationTests")
    return [
      { d: "test-report", f: [["junit.xml", 14_541], ["index.html", 90_112]] },
      { d: "logs", f: [["psp-sandbox.log", 31_744], ["integration.log", 217_088]] },
      { d: "coverage", f: [["lcov.info", 1_153_434]] },
    ];
  if (/Build|Docker|Release/.test(config))
    return [{ d: "", f: [["payout-service-1.9.0-rc1-x86_64.tar.gz", 6_501_171], ["payout-service", 19_293_798], ["sbom.cdx.json", 245_760], ["SHA256SUMS", 1_024]] }];
  return [{ d: "", f: [["report.txt", 4_096]] }];
}

/** prototype-6's `artifactText`. */
function artifactText(config: string, no: number, st: string, name: string): string | null {
  if (name === "junit.xml")
    return `<?xml version="1.0" encoding="UTF-8"?>\n<testsuites tests="42" failures="1">\n  <testsuite name="sepa_retry" tests="42" failures="1" time="81.30">\n    <testcase classname="sepa::retry" name="gives_up_after_max_attempts" time="0.42">\n      <failure message="assertion left == right failed">left: 6\n right: 5</failure>\n    </testcase>\n    <testcase classname="sepa::retry" name="backs_off_exponentially" time="0.31"/>\n  </testsuite>\n</testsuites>`;
  if (name === "psp-sandbox.log")
    return Array.from({ length: 14 }, (_, i) => `10:08:${String(41 + i * 2).padStart(2, "0")} POST /batches 503 TEMP_UNAVAILABLE attempt=${i % 6}`).join("\n");
  if (name === "SHA256SUMS") return `9f2c1e7a0b…  payout-service-1.9.0-rc1-x86_64.tar.gz\n44ab0d93c1…  payout-service`;
  if (name === "sbom.cdx.json")
    return `{\n  "bomFormat": "CycloneDX",\n  "specVersion": "1.5",\n  "components": [ { "name": "tokio", "version": "1.40.0" }, { "name": "reqwest", "version": "0.12.7" } ]\n}`;
  if (name === "integration.log") return fakeLog(config, no, st).lines.map((l) => `[${l.time}] ${l.text}`).join("\n");
  if (name === "report.txt") return `${config} #${no}\nresult: ${st}`;
  return null;
}

function runOf(args: Record<string, unknown>) {
  const id = String(args["runId"] ?? "");
  const run = fixtureRun(id);
  if (run === undefined) throw { code: "not_found", message: `${id} is not a run of a configured source`, source_id: null };
  return { id, ...run };
}

function entries(config: string, st: string, path: string): ArtifactEntry[] | undefined {
  const tree = artifactsOf(config, st);
  const files = (d: string, list: [string, number][]) =>
    list.map(([name, size]) => ({ path: d === "" ? name : `${d}/${name}`, name, size, folder: false, children: null }) satisfies ArtifactEntry);
  if (path === "")
    return tree.flatMap((d): ArtifactEntry[] => (d.d === "" ? files("", d.f) : [{ path: d.d, name: d.d, size: null, folder: true, children: d.f.length }]));
  const folder = tree.find((d) => d.d === path);
  return folder === undefined ? undefined : files(folder.d, folder.f);
}

/**
 * `?fake-log-lines=<n>` (perf audit, #2200's class): every run's log is `n`
 * lines in four steps, the third failing and holding most of them.
 */
function bulkLog(n: number): { lines: LogLine[]; steps: LogStep[]; total: number } {
  const lines: LogLine[] = [];
  const share = [0.05, 0.15, 0.75, 0.05];
  share.forEach((part, step) => {
    const count = Math.max(1, Math.round(n * part));
    for (let j = 0; j < count; j += 1) {
      const number = lines.length + 1;
      const err = step === 2 && j === count - 1;
      lines.push({ number, time: "10:05:58", text: err ? "error: test failed" : `  Running payout-service check ${number}`, tone: err ? "err" : null, step });
    }
  });
  const steps: LogStep[] = ["Checkout", "Build", "Test", "Publish"].map((name, step) => {
    const mine = lines.filter((l) => l.step === step);
    return { name, first: mine[0]!.number, last: mine.at(-1)!.number, seconds: 30, tone: step === 2 ? "err" : null };
  });
  return { lines, steps, total: lines.length };
}

/** `?fake-tests=<n>` (perf audit): every run ran `n` tests, one in a hundred failing. */
function bulkTests(n: number): RunTest[] {
  return Array.from({ length: n }, (_, i): RunTest => {
    const fails = i % 100 === 3;
    const module = `module_${String(Math.floor(i / 40)).padStart(3, "0")}`;
    return { id: String(90_000 + i), name: `${module}::case_${i}`, module, status: fails ? "failed" : "passed", muted: false, new_failure: false, duration_ms: 40 + (i % 900), details: fails ? DETAILS : null };
  }).sort((a, b) => Number(b.status === "failed") - Number(a.status === "failed"));
}

export function handlers(params: URLSearchParams = new URLSearchParams()): Handlers {
  const bulkLines = Number(params.get("fake-log-lines") ?? 0) || 0;
  const bulkTestCount = Number(params.get("fake-tests") ?? 0) || 0;
  return {
    run_log: (args) => {
      const run = runOf(args);
      const log = bulkLines > 0 ? bulkLog(bulkLines) : fakeLog(run.config, run.no, run.st);
      const count = typeof args["count"] === "number" ? args["count"] : 20_000;
      const from = typeof args["from"] === "number" ? args["from"] : null;
      const lines = from === null ? log.lines.slice(-count) : log.lines.slice(from - 1, from - 1 + count);
      return { ...log, lines };
    },
    run_tests: (args) => {
      const run = runOf(args);
      if (bulkTestCount > 0) return { tests: bulkTests(bulkTestCount), truncated: false };
      return { tests: testsOf(run.config, run.st === "failed"), truncated: false };
    },
    test_history: (args) => {
      const run = runOf(args);
      const failing = String(args["testId"]) === String(4000 + TEST_NAMES.indexOf(FAILING));
      // prototype-6: "Failed in 2 of the last 10 runs, both on this branch."
      return Array.from({ length: 10 }, (_, i): TestOutcome => {
        const no = run.no - i;
        const failed = failing && (i === 0 || i === 2);
        return {
          run: `teamcity:build:${run.config}-${no}`,
          number: String(no),
          branch: i % 2 === 0 ? "feature/PAY-231-sepa-retry" : "main",
          status: failed ? "failed" : "passed",
        };
      });
    },
    run_artifacts: (args) => {
      const run = runOf(args);
      const path = typeof args["path"] === "string" ? args["path"] : "";
      const found = entries(run.config, run.st, path);
      if (found === undefined) throw { code: "not_found", message: `no folder ${path}`, source_id: null };
      return found;
    },
    artifact_preview: (args) => {
      const run = runOf(args);
      const path = String(args["path"] ?? "");
      const text = artifactText(run.config, run.no, run.st, path.split("/").at(-1) ?? path);
      return { text, truncated: false };
    },
    download_run_file: (args) => {
      const run = runOf(args);
      const target = args["target"] as { kind: string; path?: string };
      const name =
        target.kind === "log"
          ? `${run.config}-${run.no}.log`
          : target.kind === "all"
            ? `${run.config}-${run.no}-artifacts.zip`
            : target.kind === "folder"
              ? `${target.path!.split("/").at(-1)}.zip`
              : target.path!.split("/").at(-1)!;
      return `/Users/mara/Downloads/${name}`;
    },
  };
}
