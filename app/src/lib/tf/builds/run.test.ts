/**
 * The run page's rules (#853, R-9.9 to R-9.11), on plain data: the tabs and
 * their addresses, the Overview's lines, what the run's view says, and what
 * *Re-run* sends.
 */
import { expect, test } from "vitest";

import type { PipelineSettings, RunRow, SourceView, ViewSection } from "../../ipc/entity";
import {
  artifactsOf,
  changesOf,
  commitOf,
  contextLine,
  factOf,
  failureHeading,
  firstFailure,
  locationIn,
  parameterRows,
  pipelineOfRun,
  previousOnBranch,
  rerunRequest,
  runTabHash,
  runTabOf,
  tookLine,
} from "./run";

function run(no: number, tone: "ok" | "err" | null, branch = "main", extra: Partial<RunRow> = {}): RunRow {
  return {
    entity_id: `teamcity:build:${no}`,
    number: String(no),
    state: tone === null ? null : { text: tone === "err" ? "FAILURE" : "SUCCESS", tone },
    branch,
    started: "2026-10-16T08:00:00Z",
    finished: tone === null ? null : "2026-10-16T08:04:00Z",
    duration_seconds: tone === null ? null : 240,
    updated_at: "2026-10-16T08:04:00Z",
    web_url: null,
    ...extra,
  };
}

const FEATURE = "feature/PAY-231-sepa-retry";

// Newest first, as `pipeline_runs` answers.
const RUNS: RunRow[] = [
  run(1188, null, FEATURE),
  run(1187, "err", FEATURE, { duration_seconds: 252 }),
  run(1186, "ok", "main"),
  run(1185, "err", FEATURE, { duration_seconds: 268 }),
  run(1184, "ok", FEATURE),
];

test("a run's tab is the address's third segment, and Overview is the run's own address", () => {
  expect(runTabOf(["p", "r"])).toBe("overview");
  expect(runTabOf(["p", "r", "changes"])).toBe("changes");
  expect(runTabOf(["p", "r", "params"])).toBe("params");
  expect(runTabOf(["p", "r", "nonsense"])).toBe("overview");
  expect(runTabHash("teamcity:buildType:A", "teamcity:build:7", "overview")).toBe(
    "#/build/teamcity%3AbuildType%3AA/teamcity%3Abuild%3A7",
  );
  expect(runTabHash("teamcity:buildType:A", "teamcity:build:7", "log")).toBe(
    "#/build/teamcity%3AbuildType%3AA/teamcity%3Abuild%3A7/log",
  );
});

test("the previous run on the branch is the next older finished one on the same branch", () => {
  expect(previousOnBranch(RUNS, RUNS[1]!)?.number).toBe("1185");
  expect(previousOnBranch(RUNS, RUNS[0]!)?.number).toBe("1187");
  expect(previousOnBranch(RUNS, RUNS[4]!)).toBeNull();
});

test("took says the duration and the difference to the previous run on the branch", () => {
  expect(tookLine(RUNS[1]!, RUNS[3]!)).toBe("4 m 12 s, −16 s against #1185");
  expect(tookLine(RUNS[3]!, RUNS[4]!)).toBe("4 m 28 s, +28 s against #1184");
  expect(tookLine(RUNS[4]!, null)).toBe("4 m 00 s");
  expect(tookLine(RUNS[0]!, null)).toBe("still running");
  expect(tookLine(run(1189, null, "main", { started: null }), null)).toBe("not started");
});

test("the context line says how the branch's run before and the default branch's last run went", () => {
  expect(contextLine(RUNS, RUNS[1]!, "main")).toBe("Also failed in #1185 on this branch. #1186 on main passed.");
  expect(contextLine(RUNS, RUNS[3]!, "main")).toBe("#1184 on this branch passed.");
  const mainFailed = [run(20, "err", FEATURE), run(19, "err", "main"), run(18, "ok", FEATURE)];
  expect(contextLine(mainFailed, mainFailed[0]!, "main")).toBe("#18 on this branch passed. #19 on main failed.");
  // A run on the default branch says nothing more about the default branch.
  const onMain = [run(30, "err", "main"), run(29, "ok", "main")];
  expect(contextLine(onMain, onMain[0]!, "main")).toBe("#29 on this branch passed.");
  expect(contextLine([run(1, "err")], run(1, "err"), null)).toBe("");
});

/** TeamCity's *Tests* section as the adapter draws it: a row per failure, then its details. */
const TESTS: ViewSection = {
  id: "tests",
  title: "Tests",
  count: 3,
  deferred: false,
  blocks: [
    { type: "entities", title: null, rows: [row("sepa::retry::muted_one", "err", "muted", "muted")] },
    { type: "entities", title: null, rows: [row("sepa::retry::gives_up_after_max_attempts", "err", "new")] },
    {
      type: "text",
      markdown: false,
      text: "thread 'gives_up' panicked at tests/sepa_retry.rs:14:5:\nassertion `left == right` failed\n  left: 6\n right: 5",
    },
    { type: "entities", title: null, rows: [row("sepa::retry::backs_off", "err", "failed")] },
  ],
};

function row(title: string, tone: "err" | "muted" | null, chip: string | null, muted?: "muted") {
  return { entity_id: null, title, meta: null, tone: muted === "muted" ? ("muted" as const) : tone, chip, url: null };
}

test("the first failure is the first failed test that is not muted, with its details and how many failed", () => {
  const first = firstFailure(TESTS);
  expect(first?.name).toBe("sepa::retry::gives_up_after_max_attempts");
  expect(first?.details).toContain("assertion `left == right` failed");
  expect(first?.count).toBe(3);
  expect(first?.location).toEqual({ path: "tests/sepa_retry.rs", line: 14 });
  expect(firstFailure({ ...TESTS, blocks: [TESTS.blocks[0]!] })).toBeNull();
  expect(firstFailure(null)).toBeNull();
});

test("a failure's heading names one test, or how many and the first", () => {
  expect(failureHeading({ name: "a::b", details: null, count: 1, location: null })).toBe("1 test failed: a::b");
  expect(failureHeading({ name: "a::b", details: null, count: 3, location: null })).toBe("3 tests failed. The first: a::b");
});

test("a location is a path with a directory and a line, never a bare class name", () => {
  expect(locationIn("panicked at tests/sepa_retry.rs:14:5:")).toEqual({ path: "tests/sepa_retry.rs", line: 14 });
  expect(locationIn("at src/main/java/a/B.java:31)")).toEqual({ path: "src/main/java/a/B.java", line: 31 });
  expect(locationIn("at com.example.B(B.java:31)")).toBeNull();
  expect(locationIn(null)).toBeNull();
});

/** The whole view of a run, as TeamCity's adapter answers it. */
const VIEW: SourceView = {
  actions: [],
  sections: [
    {
      id: "overview",
      title: "Overview",
      count: null,
      deferred: false,
      blocks: [
        {
          type: "facts",
          facts: [
            { label: "Outcome", value: "failed", tone: "err" },
            { label: "Triggered by", value: "mara.lindqvist", tone: null },
            { label: "Agent", value: "agent-linux-03", tone: null },
          ],
        },
        {
          type: "entities",
          title: "Configuration",
          rows: [{ entity_id: "teamcity:buildType:A", title: "Payouts / A", meta: null, tone: null, chip: null, url: null }],
        },
        {
          type: "entities",
          title: "Revision",
          rows: [{ entity_id: "gitea:commit:a41f2c", title: "payout-service", meta: "main", tone: null, chip: "a41f2c0b9e11", url: null }],
        },
      ],
    },
    {
      id: "changes",
      title: "Changes",
      count: 2,
      deferred: false,
      blocks: [
        {
          type: "entities",
          title: null,
          rows: [
            { entity_id: null, title: "PAY-231 backoff jitter", meta: "mara · 2026-10-16 09:58 +00:00", tone: null, chip: "a41f2c0b9e11", url: null },
            { entity_id: null, title: "PAY-231: retry", meta: null, tone: null, chip: "7be0e4aa0000", url: null },
          ],
        },
      ],
    },
    {
      id: "artifacts",
      title: "Artifacts",
      count: 5,
      deferred: false,
      blocks: [
        {
          type: "entities",
          title: null,
          rows: ["a", "b", "c", "d", "e"].map((name) => ({ entity_id: null, title: name, meta: "1 KB", tone: null, chip: null, url: `http://ci.example/${name}` })),
        },
      ],
    },
    {
      id: "parameters",
      title: "Parameters",
      count: 3,
      deferred: false,
      blocks: [
        {
          type: "settings",
          title: null,
          entity: "teamcity:build:7",
          write_op: "",
          values: [
            { name: "env", value: "prod-dryrun", inherited: false, secret: false, editable: false, description: null },
            { name: "deploy.token", value: "", inherited: false, secret: true, editable: false, description: null },
            { name: "extra.flag", value: "on", inherited: false, secret: false, editable: false, description: null },
          ],
        },
      ],
    },
  ],
};

test("a fact of the run's overview is read by its label", () => {
  expect(factOf(VIEW, "Triggered by")).toBe("mara.lindqvist");
  expect(factOf(VIEW, "Agent")).toBe("agent-linux-03");
  expect(factOf(VIEW, "Now")).toBeNull();
  expect(factOf(null, "Agent")).toBeNull();
});

test("the commit is the overview's row with a sha, and the pipeline the row a pipeline of the list carries", () => {
  expect(commitOf(VIEW)).toEqual({ sha: "a41f2c0b9e11", entityId: "gitea:commit:a41f2c" });
  expect(commitOf(null)).toBeNull();
  expect(pipelineOfRun(VIEW, ["teamcity:buildType:B", "teamcity:buildType:A"])).toBe("teamcity:buildType:A");
  expect(pipelineOfRun(VIEW, ["teamcity:buildType:B"])).toBeNull();
});

test("the Artifacts panel lists the first four and counts them all; Changes lists the commits", () => {
  const artifacts = artifactsOf(VIEW);
  expect(artifacts.first.map((r) => r.title)).toEqual(["a", "b", "c", "d"]);
  expect(artifacts.total).toBe(5);
  expect(changesOf(VIEW).map((r) => r.chip)).toEqual(["a41f2c0b9e11", "7be0e4aa0000"]);
  expect(artifactsOf(null)).toEqual({ first: [], total: 0 });
});

const SETTINGS: PipelineSettings = {
  repositories: [{ name: "payout-service", url: null, default_branch: "refs/heads/main", entity_id: "gitea:repo:p" }],
  triggers: [],
  parameters: [
    { name: "env", value: "staging", inherited: false, secret: false, editable: true, description: null },
    { name: "log.level", value: "info", inherited: false, secret: false, editable: true, description: null },
    { name: "deploy.token", value: "", inherited: true, secret: true, editable: true, description: null },
  ],
  parameter_write_op: "set_parameter",
  parameter_specs: [],
  chain: [],
};

test("the Parameters tab pairs each value the run ran with with the pipeline's default", () => {
  expect(parameterRows(VIEW, SETTINGS)).toEqual([
    { name: "env", value: "prod-dryrun", secret: false, default: "staging" },
    { name: "deploy.token", value: "", secret: true, default: "" },
    { name: "extra.flag", value: "on", secret: false, default: null },
  ]);
});

test("Re-run sends the run's branch and every value it ran with that is not a default", () => {
  const request = rerunRequest("teamcity:buildType:A", run(7, "err", FEATURE), SETTINGS, VIEW);
  expect(request.payload).toEqual({
    RunBuild: {
      entity: "teamcity:buildType:A",
      branch: FEATURE,
      parameters: [
        { name: "env", value: "prod-dryrun" },
        { name: "extra.flag", value: "on" },
      ],
      clean_checkout: false,
      first_in_queue: false,
    },
  });
  expect(request.summary).toBe("Re-run of #7, same settings");
  expect(request.preset).toBeNull();
  // The default branch runs as no branch, as the Run… dialog sends it (#852).
  const onMain = rerunRequest("teamcity:buildType:A", run(8, "ok", "main"), SETTINGS, null);
  expect(onMain.payload).toEqual({
    RunBuild: { entity: "teamcity:buildType:A", branch: null, parameters: [], clean_checkout: false, first_in_queue: false },
  });
});
