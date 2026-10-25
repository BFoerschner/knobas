/**
 * The pipeline page's rules (#851) on plain data: the chart, the facts, the
 * Runs filter and day groups, the older page appended, the source's word,
 * and the warn line.
 */
import { expect, test } from "vitest";

import type { PipelineSettings, RunRow } from "../../ipc/entity";
import type { AuthMethod, CredentialHealth, SourceDescriptor, SourceSyncStatus } from "../../ipc/sources";
import {
  appendOlder,
  barHeight,
  byDay,
  defaultBranch,
  historyFacts,
  historyOf,
  jobBranchFor,
  keptRuns,
  longest,
  echoRun,
  pipelineWord,
  QUEUED_RUN,
  queuedRunRows,
  resultOf,
  runLanded,
  staleLine,
  syncWith,
  tabHash,
  tabOf,
  triggerDetail,
  triggerLabel,
  triggersSentence,
} from "./pipeline";
import { forgetEchoes, echoesOn, type Echo } from "../queued.svelte";

function run(no: number, tone: "ok" | "err" | "info", extra: Partial<RunRow> = {}): RunRow {
  return {
    entity_id: `teamcity:build:${no}`,
    number: String(no),
    state: { text: tone === "err" ? "FAILURE" : tone === "ok" ? "SUCCESS" : "running", tone },
    branch: "main",
    started: "2026-08-22T10:05:00Z",
    finished: "2026-08-22T10:09:12Z",
    duration_seconds: 252,
    updated_at: "2026-08-22T10:09:12Z",
    web_url: null,
    trigger: null,
    started_by: null,
    started_by_me: false,
    revision: null,
    commit_id: null,
    agent: null,
    current_step: null,
    tests: null,
    ...extra,
  };
}

const running = (no: number) => run(no, "info", { finished: null, duration_seconds: null });
const queued = (no: number) => run(no, "info", { number: null, started: null, finished: null, duration_seconds: null });

test("the tab is the address's second segment, Runs for anything else; Runs is the pipeline's own address", () => {
  expect(tabOf(["teamcity:buildType:X"])).toBe("runs");
  expect(tabOf(["teamcity:buildType:X", "params"])).toBe("params");
  expect(tabOf(["teamcity:buildType:X", "triggers"])).toBe("triggers");
  expect(tabOf(["teamcity:buildType:X", "nope"])).toBe("runs");
  expect(tabHash("teamcity:buildType:X", "runs")).toBe("#/builds/teamcity%3AbuildType%3AX");
  expect(tabHash("teamcity:buildType:X", "params")).toBe("#/builds/teamcity%3AbuildType%3AX/params");
});

test("the chart draws the last thirty runs oldest first, height by duration with a floor, in flight full height", () => {
  const runs = Array.from({ length: 40 }, (_, i) => run(1200 - i, "ok", { duration_seconds: 100 + i }));
  const history = historyOf(runs);
  expect(history).toHaveLength(30);
  expect(history[0]!.number).toBe("1171");
  expect(history[29]!.number).toBe("1200");
  const max = longest(runs);
  expect(max).toBe(139);
  expect(barHeight(run(1, "ok", { duration_seconds: 139 }), max)).toBe(56);
  expect(barHeight(run(1, "ok", { duration_seconds: 1 }), max)).toBe(8);
  expect(barHeight(run(1, "ok", { duration_seconds: 70 }), max)).toBe(28);
  expect(barHeight(running(1), max)).toBe(56);
  expect(longest([])).toBe(1);
});

test("Passed counts the finished runs that passed, Usual duration is their median, Last passing the newest that passed", () => {
  const facts = historyFacts([
    running(1190),
    run(1189, "err", { duration_seconds: 300 }),
    run(1188, "ok", { duration_seconds: 100 }),
    run(1187, "ok", { duration_seconds: 200 }),
    queued(1186),
  ]);
  expect(facts.passed).toBe(2);
  expect(facts.finished).toBe(3);
  expect(facts.usual).toBe(200);
  expect(facts.lastPassing?.number).toBe("1188");
  expect(historyFacts([run(1, "err")]).lastPassing).toBeNull();
  expect(historyFacts([]).usual).toBeNull();
});

test("a run falls under one result chip by its declared tone and instants", () => {
  expect(resultOf(run(1, "ok"))).toBe("Passed");
  expect(resultOf(run(1, "err"))).toBe("Failed");
  expect(resultOf(running(1))).toBe("Running");
  expect(resultOf(queued(1))).toBe("Queued");
  expect(resultOf(run(1, "ok", { state: { text: "canceled", tone: "muted" } }))).toBeNull();
});

test("the Runs filter keeps by words over number and branch, the job's branches, any result chip and the branch", () => {
  const runs = [
    run(1190, "err", { branch: "feature/PAY-231-sepa-retry" }),
    run(1189, "ok", { branch: "main" }),
    running(1188),
    run(1187, "ok", { branch: "feature/csv-export" }),
  ];
  const keep = (filter: Partial<Parameters<typeof keptRuns>[1]>, job: string[] = []) =>
    keptRuns(runs, { text: "", scope: "all", results: [], branch: null, ...filter }, job).map((r) => r.number);
  expect(keep({})).toEqual(["1190", "1189", "1188", "1187"]);
  expect(keep({ text: "#1189" })).toEqual(["1189"]);
  expect(keep({ text: "sepa" })).toEqual(["1190"]);
  expect(keep({ scope: "job" }, ["feature/PAY-231-sepa-retry"])).toEqual(["1190"]);
  expect(keep({ scope: "job" }, [])).toEqual([]);
  expect(keep({ results: ["Failed", "Running"] })).toEqual(["1190", "1188"]);
  expect(keep({ branch: "main" })).toEqual(["1189", "1188"]);
});

test("a trigger reads as prototype-6 says it: VCS change and the short sha, Schedule, the person, you for the reader", () => {
  const sha = "a41f2c0e9b8d7c6a5f4e3d2c1b0a9f8e7d6c5b4a";
  expect(triggerLabel(run(1, "ok", { trigger: "change", revision: sha }))).toBe("VCS change a41f2c");
  expect(triggerLabel(run(2, "ok", { trigger: "change" }))).toBe("VCS change");
  expect(triggerLabel(run(3, "ok", { trigger: "schedule" }))).toBe("Schedule");
  expect(triggerLabel(run(4, "ok", { trigger: "person", started_by: "jonas" }))).toBe("jonas");
  expect(triggerLabel(run(5, "ok", { trigger: "person", started_by: "mara", started_by_me: true }))).toBe("you");
  expect(triggerLabel(run(6, "ok", { started_by: "priya" }))).toBe("priya");
  expect(triggerLabel(run(7, "ok"))).toBe("");
});

test("a step a chain queued reads as the chain, and by whom where the run names a person (#1146)", () => {
  expect(triggerLabel(run(1, "ok", { trigger: "chain" }))).toBe("Chain");
  expect(triggerLabel(run(2, "ok", { trigger: "chain", started_by: "jonas" }))).toBe("Chain, by jonas");
  expect(triggerLabel(run(3, "ok", { trigger: "chain", started_by: "mara", started_by_me: true }))).toBe("Chain, by you");
});

test("Started by me keeps the reader's runs; the words reach the commit and who or what started a run", () => {
  const runs = [
    run(1190, "ok", { trigger: "change", revision: "a41f2c0e9b8d" }),
    run(1189, "ok", { trigger: "person", started_by: "mara", started_by_me: true, revision: "91fe0c11" }),
    run(1188, "ok", { trigger: "person", started_by: "jonas" }),
    run(1187, "ok", { trigger: "schedule" }),
  ];
  const keep = (filter: Partial<Parameters<typeof keptRuns>[1]>) =>
    keptRuns(runs, { text: "", scope: "all", results: [], branch: null, ...filter }, []).map((r) => r.number);
  expect(keep({ scope: "mine" })).toEqual(["1189"]);
  expect(keep({ text: "a41f2c" })).toEqual(["1190"]);
  expect(keep({ text: "91fe0c11" })).toEqual(["1189"]);
  expect(keep({ text: "jonas" })).toEqual(["1188"]);
  expect(keep({ text: "mara" })).toEqual(["1189"]);
  expect(keep({ text: "you" })).toEqual(["1189"]);
  expect(keep({ text: "schedule" })).toEqual(["1187"]);
});

test("runs group by the local day they started, Today and Yesterday by name", () => {
  const now = new Date(2026, 7, 22, 14, 0);
  const at = (d: number, h: number) => new Date(2026, 7, d, h, 0).toISOString();
  const groups = byDay(
    [
      queued(1191),
      run(1190, "ok", { started: at(22, 11) }),
      run(1189, "ok", { started: at(21, 18) }),
      run(1188, "ok", { started: at(21, 9) }),
      run(1187, "ok", { started: at(19, 9) }),
    ],
    now,
  );
  expect(groups.map((g) => [g.label, g.runs.map((r) => r.entity_id.split(":").at(-1))])).toEqual([
    ["Today", ["1191", "1190"]],
    ["Yesterday", ["1189", "1188"]],
    ["2026-08-19", ["1187"]],
  ]);
});

test("an older page goes after what the page holds, a run it holds already left out", () => {
  const held = [run(3, "ok"), run(2, "ok")];
  expect(appendOlder(held, [run(2, "ok"), run(1, "err")]).map((r) => r.number)).toEqual(["3", "2", "1"]);
  expect(appendOlder(held, [])).toHaveLength(2);
});

test("the source's word is the label its descriptor gives the kind it declares the pipeline frame on", () => {
  const descriptor = {
    adapter_kind: "teamcity",
    entity_kinds: [
      { id: "build", label: "Build" },
      { id: "build_config", label: "Build configuration" },
    ],
    payload_paths: [
      { kind: "build", frame: "run" },
      { kind: "build_config", frame: "pipeline" },
    ],
  } as unknown as SourceDescriptor;
  expect(pipelineWord([descriptor], "teamcity")).toBe("build configuration");
  expect(pipelineWord([descriptor], "gitea")).toBeNull();
  expect(pipelineWord([], null)).toBeNull();
});

test("settings: the default branch by name, the triggers switched on in a sentence, a trigger's facts in a line", () => {
  const settings: PipelineSettings = {
    repositories: [
      { name: "x", url: null, default_branch: null, entity_id: null },
      { name: "payout-service", url: null, default_branch: "refs/heads/main", entity_id: null },
    ],
    triggers: [
      { name: "VCS", facts: [{ label: "Branch filter", value: "+:*", tone: null }], disabled: false },
      { name: "Schedule", facts: [{ label: "When", value: "daily at 02:00", tone: null }], disabled: false },
      { name: "Finish build", facts: [], disabled: true },
    ],
    parameters: [],
    parameter_write_op: null,
    parameter_specs: [],
    chain: [],
    steps: [],
  };
  expect(defaultBranch(settings)).toBe("main");
  expect(defaultBranch(null)).toBeNull();
  expect(triggersSentence(settings)).toBe("Triggered by VCS and Schedule.");
  expect(triggersSentence({ ...settings, triggers: [settings.triggers[0]!] })).toBe("Triggered by VCS.");
  expect(triggersSentence(null)).toBe("");
  expect(triggerDetail(settings.triggers[1]!)).toBe("when daily at 02:00");
});

test("Run on offers the first of the job's branches this pipeline has built, else none", () => {
  const runs = [run(2, "ok", { branch: "main" }), run(1, "ok", { branch: "feature/PAY-231-sepa-retry" })];
  expect(jobBranchFor(runs, ["feature/other", "feature/PAY-231-sepa-retry"])).toBe("feature/PAY-231-sepa-retry");
  expect(jobBranchFor(runs, ["feature/other"])).toBeNull();
});

test("the warn line: a failing sign-in with its fix, a failed sync without one, nothing for a source that synced", () => {
  const now = new Date("2026-08-22T12:00:00Z");
  const pipeline = { source_id: "teamcity", source_name: "TeamCity" };
  const health = (state: CredentialHealth["state"], auth_kind: AuthMethod | null = "UserPassword") => [
    {
      id: "teamcity",
      auth_kind,
      health: { source_id: "teamcity", state, checked_at: "2026-08-22T11:22:00Z", detail: null, secret_expires_at: null },
    },
  ];
  const sync = (outcome: SourceSyncStatus["last_outcome"]): SourceSyncStatus[] => [
    { source_id: "teamcity", running: false, run_id: 1, started_at: null, last_finished_at: "2026-08-22T11:00:00Z", last_outcome: outcome } as SourceSyncStatus,
  ];
  expect(staleLine(pipeline, health("unauthorized"), sync("unauthorized"), now)).toEqual({
    text: "TeamCity data is 1 h old: sign-in failing. Runs you start wait in the queue until it works again.",
    fix: { href: "#/settings/source/teamcity", words: "Re-enter password", secret: "password" },
  });
  // A token source is told to re-enter its token (#1405).
  expect(staleLine(pipeline, health("missing_secret", "Pat"), sync("ok"), now)).toEqual({
    text: "TeamCity data is 38 min old: no token stored. Runs you start wait in the queue until it works again.",
    fix: { href: "#/settings/source/teamcity", words: "Re-enter token", secret: "token" },
  });
  expect(staleLine(pipeline, health("missing_secret"), sync("ok"), now)?.text).toContain("no password stored");
  expect(staleLine(pipeline, health("ok"), sync("unreachable"), now)).toEqual({
    text: "TeamCity data may be out of date: TeamCity cannot be reached, last tried 1 h ago.",
    fix: null,
  });
  expect(staleLine(pipeline, health("ok"), sync("ok"), now)).toBeNull();
  expect(staleLine({ ...pipeline, source_id: "other" }, health("unauthorized"), sync("error"), now)).toBeNull();
});

// -- runs queued this session (#1449) -------------------------------------------

const mark = (write: number, branch: string, base: string | null): Echo => ({ write, kind: "run", text: branch, base, state: "queued", surface: null, source: null });

test("a queued run is a queued row on its branch, the default branch where it named none, newest first", () => {
  const rows = queuedRunRows([mark(4, "feature/x", "teamcity:build:9"), mark(5, "", "teamcity:build:9")], "main");
  expect(rows.map((r) => [r.entity_id, r.branch, r.number, r.state?.text, triggerLabel(r)])).toEqual([
    ["queued:5", "main", null, "queued", "you"],
    ["queued:4", "feature/x", null, "queued", "you"],
  ]);
  expect(rows.every((r) => r.started === null && r.finished === null)).toBe(true);
});

test("a read holds a queued run once a run newer than the newest held then, started by a person, is on its branch", () => {
  const held = [run(10, "ok"), run(9, "ok")];
  const queued = mark(4, "feature/x", "teamcity:build:10");
  // Nothing newer: still queued.
  expect(runLanded(held)(queued)).toBe(false);
  // A newer run a change started, or one on another branch, is not it.
  expect(runLanded([run(11, "ok", { branch: "feature/x", trigger: "change" }), ...held])(queued)).toBe(false);
  expect(runLanded([run(11, "ok", { branch: "main", trigger: "person" }), ...held])(queued)).toBe(false);
  // A newer run on its branch, by a person or saying nothing of how: it.
  expect(runLanded([run(11, "info", { branch: "refs/heads/feature/x", trigger: "person" }), ...held])(queued)).toBe(true);
  expect(runLanded([run(11, "info", { branch: "feature/x" }), ...held])(queued)).toBe(true);
  // Queued on the default branch, whose name the page may not know: a newer run on any branch.
  expect(runLanded([run(11, "info", { branch: "release" }), ...held])(mark(4, "", "teamcity:build:10"))).toBe(true);
  // Queued on an empty pipeline: every run read is newer.
  expect(runLanded(held)(mark(4, "main", null))).toBe(true);
});

test("each newer run holds one queued run, the oldest first", () => {
  const first = mark(4, "main", "teamcity:build:10");
  const second = mark(5, "main", "teamcity:build:10");
  const landed = runLanded([run(11, "info"), run(10, "ok")]);
  expect([first, second].filter(landed)).toEqual([first]);
});

test("a sync status heard takes its source's place, or joins the list", () => {
  const status = (source_id: string, running: boolean) => ({ source_id, running }) as SourceSyncStatus;
  const rows = [status("teamcity", true), status("gitea", false)];
  expect(syncWith(rows, status("teamcity", false))).toEqual([status("teamcity", false), status("gitea", false)]);
  expect(syncWith(rows, status("jira", false))).toEqual([...rows, status("jira", false)]);
});

test("a run queued before the page's read arrives is based on the pipeline's own recent runs", () => {
  const follower = { follow: () => () => {} };
  const pipeline = { entity_id: "teamcity:buildType:X", name: "X", recent: [run(7, "ok"), run(6, "ok")] };
  echoRun(pipeline, { id: 3 }, null, null, follower);
  echoRun(pipeline, { id: 4 }, "feature/x", [run(8, "ok")], follower);
  expect(echoesOn("teamcity:buildType:X", QUEUED_RUN)).toMatchObject([
    { write: 3, kind: QUEUED_RUN, text: "", base: "teamcity:build:7", state: "queued" },
    { write: 4, kind: QUEUED_RUN, text: "feature/x", base: "teamcity:build:8", state: "queued" },
  ]);
  forgetEchoes();
});
