/**
 * `?fake-ipc`: Builds (#848 to #853): `pipelines`, `pipeline_runs`,
 * `set_pipeline_pinned`, `job_pipelines`, `older_pipeline_runs`,
 * `pipeline_settings`, `run_chain`, `run_presets` and `save_run_preset`, and a run's
 * view for `entity_view` (`runView`), over
 * prototype-6's 59 build configurations, so the list can be compared with
 * the prototype at `#/builds`.
 *
 * **Transcribed from prototype-6**, not invented: the configuration names
 * and their projects are its `cfgDefs`, and the fourteen with a story of
 * their own (`realCfg`) end on its run, state and branch. The rest of each
 * history is filled the way the prototype fills it -- mostly passing, a
 * failure now and then, every run a few minutes to hours before the next --
 * but from a fixed seed, so two screenshots of one build agree. Times are
 * relative to now, as the prototype's are relative to its *today 14:32*.
 *
 * The state chip is TeamCity's own word (`SUCCESS`, `FAILURE`, `running`,
 * `queued`), which is what the adapter declares (#666), and not the
 * prototype's *passed*: the shell draws what the source says.
 */
import { SCALE_HISTORY } from "../fake-scale";
import type {
  ChainStepRow,
  JobPipeline,
  MarkScope,
  ParameterSpec,
  PipelineReason,
  PipelineRow,
  PipelineSettings,
  RecentCursor,
  RecentFilter,
  BubbleCounts,
  RunCounts,
  RunMarks,
  SeenMove,
  RunPreset,
  RunRow,
  Setting,
  SourceView,
  Fact,
  ViewBlock,
  ViewRow,
  ViewSection,
} from "../../../ipc/entity";
import type { Handlers } from "../fake-tauri";
import { keepSeed, onFakeReset } from "../fake-seed";
import { fakeRoleScope } from "./corpus";
import { payoutBranchPulls } from "./views/repos";
import { keyOf } from "../../../time/format";
import { recentOrder } from "../../../tf/builds/recent";
import { runBubble } from "../../../tf/builds/builds";
import { dayKey } from "../../../time/day";

/** The role `pipelines` is read for, as `knobas_source::role::CI` spells it. */
const CI_ROLE = "ci";

const SOURCE = "teamcity";
/** The fixture's TeamCity, as `fake/sources.ts` names it. */
const SOURCE_NAME = "Tidewater TeamCity";

/** prototype-6's `cfgDefs`: project path to its configurations. */
const CONFIGURATIONS: Record<string, string[]> = {
  "Payments/Payouts": ["Payout_Build", "Payout_IntegrationTests", "Payout_Lint", "Payout_Docker"],
  "Payments/Payouts/Deploy": ["Payout_Deploy_Staging", "Payout_Deploy_Prod"],
  "Payments/Payouts/Dashboard": ["Dashboard_Build", "Dashboard_E2E"],
  "Payments/PSP client": ["PspClient_Build", "PspClient_Release"],
  "Payments/Workers": ["RefundWorker_Build", "Settlement_Nightly", "Notifier_Build"],
  "Payments/Ledger": ["Ledger_Build", "Ledger_Tests", "Ledger_Migrations_Check", "FxRates_Build"],
  "Payments/Ledger/Deploy": ["Ledger_Deploy_Staging", "Ledger_Deploy_Prod"],
  "Logistics/Customs": [
    "CustomsGateway_Build",
    "CustomsGateway_Contract",
    "CustomsDocs_Build",
    "HsLookup_Build",
    "EdiParser_Build",
    "EdiParser_Fuzz",
    "EdiGateway_Deploy",
  ],
  "Logistics/Tracking": [
    "TrackingEvents_Build",
    "CarrierAdapters_Build",
    "CarrierDhl_Contract",
    "CarrierMaersk_Contract",
    "EtaModel_Train",
    "ContainerTracker_Build",
    "PortCalls_Build",
    "VesselSchedule_Build",
  ],
  "Logistics/Booking": [
    "BookingApi_Build",
    "BookingWeb_Build",
    "BookingWeb_E2E",
    "RateQuotes_Build",
    "QuoteEngine_Build",
    "QuoteEngine_Bench",
    "InvoiceEngine_Build",
    "InvoicePdf_Build",
    "DocStore_Build",
  ],
  "Platform/Services": ["AuthGateway_Build", "UserAdmin_Build", "NotificationHub_Build"],
  "Platform/Infrastructure": [
    "Terraform_Plan",
    "Terraform_Apply_Stage",
    "Helm_Lint",
    "TraefikConfig_Validate",
    "MonitoringStack_Deploy",
  ],
  "Platform/Tooling": ["CiTemplates_Test", "Runbooks_Linkcheck"],
  Data: [
    "DataPipeline_Build",
    "DataPipeline_Nightly",
    "Dbt_Run_Nightly",
    "Dbt_Test",
    "WarehouseSync_Build",
    "YardPlanner_Build",
  ],
};

/** prototype-6's `S.pins`: pinned until the session unpins them (#849). */
const pins = new Set(["Payout_IntegrationTests", "Payout_Build", "Payout_Deploy_Staging"].map((c) => `${SOURCE}:buildType:${c}`));
/** The runs a mark reads (ux18, ADR-0063): none at load, as on a database before `0098`. */
const readRuns = new Set<string>();
/** The runs marked while they ran (ADR-0063): out of the blue only. */
const whileRunningRuns = new Set<string>();

/**
 * The instants green, yellow and grey count from (#2568, the backend's `builds.seen`): when Builds was last left, each
 * pipeline's own, and the visit's start while Builds is in front. `at` starts four hours back, so the fixture's
 * recent passes are green. A run's stamp is its finish, the fixture's one clock.
 */
const seen: { at: string; pipelines: Map<string, string>; visit: string | null } = {
  at: new Date(Date.now() - 4 * 3_600_000).toISOString(),
  pipelines: new Map(),
  visit: null,
};

/** The five counts, all zero. */
function noCounts(): BubbleCounts {
  return { blue: 0, green: 0, yellow: 0, red: 0, grey: 0 };
}

/**
 * `run_counts` over `pipelines`' runs (the backend's `RUN_COUNTS`), or the five numbers `fake-counts=b,g,y,r,x` names
 * for the whole, on one pipeline: what a shot of each colour draws.
 */
function fakeRunCounts(pipelines: readonly { id: string; runs: readonly RunRow[] }[], override: string | null): RunCounts {
  const visiting = seen.visit !== null;
  if (override !== null) {
    const [blue = 0, green = 0, yellow = 0, red = 0, grey = 0] = override.split(",").map((n) => Number(n) || 0);
    const counts = { blue, green, yellow, red, grey };
    const any = blue + green + yellow + red + grey > 0;
    return { pipelines: any && pipelines[0] ? [{ pipeline: pipelines[0].id, counts }] : [], total: counts, visiting, fresh: [] };
  }
  const answer: RunCounts = { pipelines: [], total: noCounts(), visiting, fresh: [] };
  const visit = seen.visit === null ? Infinity : Date.parse(seen.visit);
  for (const { id, runs } of pipelines) {
    const instant = Math.max(Date.parse(seen.at), Date.parse(seen.pipelines.get(id) ?? seen.at));
    const counts = noCounts();
    for (const run of new Map(runs.map((one) => [one.entity_id, one])).values()) {
      const bubble = runBubble(run);
      if (bubble === null || readRuns.has(run.entity_id)) continue;
      if (bubble === "blue") {
        if (!whileRunningRuns.has(run.entity_id)) counts.blue += 1;
        continue;
      }
      if (bubble !== "red") {
        const at = Date.parse(run.finished ?? "");
        if (!(at > instant && at <= visit)) continue;
        if (visiting) answer.fresh.push(run.entity_id);
      }
      counts[bubble] += 1;
    }
    if (Object.values(counts).some((n) => n > 0)) {
      answer.pipelines.push({ pipeline: id, counts });
      for (const key of Object.keys(counts) as (keyof BubbleCounts)[]) answer.total[key] += counts[key];
    }
  }
  return answer;
}

/** `move_seen`: the scope's instant moved to now; `null` for runs by id alone. */
function fakeMoveSeen(scope: Partial<MarkScope>): SeenMove | null {
  const everything = scope.everything === true;
  if (!everything && (scope.pipelines ?? []).length === 0) return null;
  const to = new Date().toISOString();
  const moved: SeenMove = { to, everything, at: everything ? seen.at : null, pipelines: [] };
  if (everything) seen.at = to;
  else
    for (const pipeline of new Set(scope.pipelines)) {
      moved.pipelines.push({ pipeline, before: seen.pipelines.get(pipeline) ?? null });
      seen.pipelines.set(pipeline, to);
    }
  return moved;
}

/** `put_back_seen`: each instant the move moved goes back, unless it moved since. */
function fakePutBackSeen(moved: SeenMove): boolean {
  let back = false;
  if (moved.everything && seen.at === moved.to && moved.at !== null) ((seen.at = moved.at), (back = true));
  for (const { pipeline, before } of moved.pipelines) {
    if (seen.pipelines.get(pipeline) !== moved.to) continue;
    if (before === null) seen.pipelines.delete(pipeline);
    else seen.pipelines.set(pipeline, before);
    back = true;
  }
  return back;
}

/** `builds_in_front`: coming in front holds the visit's start; leaving moves the instant to now. */
function fakeBuildsInFront(front: boolean): boolean {
  if (front) {
    seen.visit ??= new Date().toISOString();
    return true;
  }
  if (seen.visit === null) return false;
  seen.visit = null;
  seen.at = new Date().toISOString();
  seen.pipelines.clear();
  return true;
}

/** Whether a fixture run is a finished failure, the only kind that can be unread. */
function failedFake(run: RunRow): boolean {
  return run.finished !== null && run.state?.tone === "err";
}

/**
 * prototype-6's `realCfg[...].job`: the configurations each job's pull
 * requests are checked by (#850), and the job's branches, from its `code`.
 */
const JOBS: Record<string, { checks: string[]; pr: string; branches: string[] }> = {
  "PAY-231": {
    checks: [
      "Payout_Build",
      "Payout_IntegrationTests",
      "Payout_Lint",
      "Payout_Docker",
      "Payout_Deploy_Staging",
      "PspClient_Build",
      "PspClient_Release",
    ],
    pr: "gitea:tidewater/payout-service#142",
    branches: ["feature/PAY-231-sepa-retry", "feature/PAY-231-temp-unavailable"],
  },
  "PAY-228": { checks: ["Ledger_Deploy_Staging"], pr: "gitea:tidewater/ledger-api#88", branches: [] },
  "PAY-240": { checks: ["Dashboard_Build"], pr: "gitea:tidewater/payout-service#151", branches: [] },
};

type Latest = { no: number; st: "running" | "failed" | "success" | "queued"; branch: string; at: string; dur: number | null };

/** prototype-6's `realCfg`: the configurations whose latest run has a story. */
const LATEST: Record<string, Latest> = {
  Payout_Build: { no: 1188, st: "running", branch: "feature/PAY-231-sepa-retry", at: "today 11:45", dur: null },
  Payout_IntegrationTests: { no: 1187, st: "failed", branch: "feature/PAY-231-sepa-retry", at: "today 10:05", dur: 252 },
  Payout_Lint: { no: 904, st: "success", branch: "feature/PAY-231-sepa-retry", at: "today 11:44", dur: 38 },
  Payout_Docker: { no: 611, st: "success", branch: "feature/PAY-231-sepa-retry", at: "today 11:52", dur: 160 },
  Payout_Deploy_Staging: { no: 318, st: "success", branch: "feature/PAY-231-sepa-retry", at: "today 12:04", dur: 65 },
  PspClient_Build: { no: 233, st: "success", branch: "main", at: "days 2 15:20", dur: 111 },
  PspClient_Release: { no: 41, st: "success", branch: "main", at: "days 2 15:31", dur: 44 },
  Ledger_Deploy_Staging: { no: 412, st: "success", branch: "main", at: "yesterday 16:20", dur: 122 },
  Dashboard_Build: { no: 377, st: "success", branch: "main", at: "days 3 14:32", dur: 190 },
  EdiParser_Fuzz: { no: 88, st: "failed", branch: "main", at: "today 06:00", dur: 1800 },
  BookingWeb_E2E: { no: 2093, st: "failed", branch: "feature/quote-banner", at: "today 13:12", dur: 468 },
  Dbt_Test: { no: 640, st: "failed", branch: "main", at: "today 05:30", dur: 182 },
  EtaModel_Train: { no: 52, st: "running", branch: "main", at: "today 13:00", dur: null },
  Terraform_Plan: { no: 1402, st: "queued", branch: "main", at: "today 14:20", dur: null },
};

/**
 * prototype-6's "real history": the runs after the story of the two
 * configurations the job is about, so the run page's context line reads as
 * the prototype's (*Also failed in #1185 on this branch. #1186 on main passed.*).
 */
const AFTER_STORY: Record<string, Latest[]> = {
  Payout_IntegrationTests: [
    { no: 1186, st: "success", branch: "main", at: "yesterday 18:10", dur: 241 },
    { no: 1185, st: "failed", branch: "feature/PAY-231-sepa-retry", at: "yesterday 17:35", dur: 268 },
  ],
  Payout_Build: [{ no: 1184, st: "success", branch: "feature/PAY-231-sepa-retry", at: "today 10:03", dur: 402 }],
};

/** A small fixed-seed generator: the same fixture on every load. */
function seeded(seed: number): () => number {
  let state = seed;
  return () => {
    state = (state * 1103515245 + 12345) % 2147483648;
    return state / 2147483648;
  };
}

/** `today 11:45`, `yesterday 16:20` or `days N HH:MM`, against `now`. */
function instant(spoken: string, now: Date): Date {
  const [, word, n, hh, mm] = /^(today|yesterday|days (\d+)) (\d\d):(\d\d)$/.exec(spoken) ?? [];
  const back = word === "today" ? 0 : word === "yesterday" ? 1 : Number(n);
  return new Date(now.getFullYear(), now.getMonth(), now.getDate() - back, Number(hh), Number(mm));
}

const STATE: Record<Latest["st"], RunRow["state"]> = {
  success: { text: "SUCCESS", tone: "ok" },
  failed: { text: "FAILURE", tone: "err" },
  running: { text: "running", tone: "info" },
  queued: { text: "queued", tone: "info" },
};

function runRow(config: string, no: number, st: Latest["st"], branch: string, started: Date, dur: number | null): RunRow {
  const finished = dur === null ? null : new Date(started.getTime() + dur * 1000);
  const queued = st === "queued";
  return {
    entity_id: `${SOURCE}:build:${config}-${no}`,
    number: queued ? null : String(no),
    state: STATE[st],
    branch,
    started: queued ? null : started.toISOString(),
    finished: finished?.toISOString() ?? null,
    duration_seconds: dur,
    updated_at: (finished ?? started).toISOString(),
    web_url: queued ? null : `https://ci.tidewater.example/buildConfiguration/${config}/${no}`,
    ...startOf(config, no),
    ...ranOn(config, no, st),
    tests: testsOf(config, st),
  };
}

/**
 * A fixture run's test summary (#1448): prototype-6's, where a configuration
 * named for its tests ran 42, one of them failing on a failed run. Every
 * other configuration, and a run not yet finished, ran none.
 */
function testsOf(config: string, st: Latest["st"]): RunRow["tests"] {
  if (!config.includes("Test") || (st !== "success" && st !== "failed")) return null;
  return st === "failed" ? { failed: 1, passed: 41, ignored: 0 } : { failed: 0, passed: 42, ignored: 0 };
}

/**
 * Where a fixture run ran and what it is doing now (#1004): prototype-6's
 * agent, *Step 3 of 5 is running.* while it runs, and the mirrored commit of
 * its revision where the fixture's repository holds it, by the id the run's
 * view names it with.
 */
function ranOn(config: string, no: number, st: Latest["st"]): Pick<RunRow, "agent" | "current_step" | "commit_id"> {
  const story = storyOf(config, no);
  const repo = repositoryOf(config);
  return {
    agent: st === "queued" ? null : story.agent,
    current_step: st === "running" ? "Step 3 of 5 is running." : null,
    commit_id: repo === null ? null : `mock:tidewater/${repo}@${story.commit}`,
  };
}

/** prototype-6's `TRIG`: three changes, a schedule, then Mara (the reader), Jonas and Priya. */
const TRIGGERS: readonly (string | null)[] = [null, null, null, "schedule", "mara", "jonas", "priya"];

/** Whose account the fixture's reader signs in as: Mara, prototype-6's `ME`. */
const READER = "mara";

/**
 * How a fixture run was started (#961): prototype-6's story runs are VCS
 * changes, #1188 is the reader's, and the rest cycle through its `TRIG`. A
 * person's run of any step but a chain's last is the chain's, queued when
 * they ran a later step (#1146, prototype-6's "Run… on any step runs the
 * steps before it too"), so it still names them.
 */
function startOf(config: string, no: number): Pick<RunRow, "trigger" | "started_by" | "started_by_me" | "revision"> {
  const revision = `${storyOf(config, no).commit}${"0".repeat(34)}`;
  const story = ["Payout_IntegrationTests-1187", "Payout_IntegrationTests-1186", "Payout_IntegrationTests-1185", "Payout_Build-1184"];
  const readersRun = config === "Payout_Build" && no === 1188;
  const pick = readersRun ? READER : story.includes(`${config}-${no}`) ? null : TRIGGERS[no % TRIGGERS.length]!;
  if (pick === null) return { trigger: "change", started_by: null, started_by_me: false, revision };
  if (pick === "schedule") return { trigger: "schedule", started_by: null, started_by_me: false, revision };
  const chain = chainOf(config);
  const chained = !readersRun && chain.length > 0 && chain.indexOf(config) < chain.length - 1;
  return { trigger: chained ? "chain" : "person", started_by: pick, started_by_me: pick === READER, revision };
}

/**
 * Each configuration's runs, newest first: its story, then 33 more. Built once
 * per calendar day of `now` (#2218): {@link instant} reads only the day, so a
 * history built earlier the same day is the same history. Each read gets its
 * own rows (and test counts; `state` is one shared table's, as it always was),
 * so a caller that changes one cannot change the next read.
 */
function history(now: Date): Map<string, RunRow[]> {
  const day = dayKey(now);
  if (built?.day !== day) {
    built = { day, runs: buildHistory(now) };
    historyBuilds += 1;
  }
  const own = (run: RunRow): RunRow => ({ ...run, tests: run.tests && { ...run.tests } });
  return new Map([...built.runs].map(([config, runs]) => [config, runs.map(own)]));
}

/** The last history {@link history} built, and the day it was built for. */
let built: { day: string; runs: Map<string, RunRow[]> } | null = null;

/** How many times {@link history} has built a history: a test's witness that reads reuse it. */
export let historyBuilds = 0;

/**
 * `?fake-hooks`' `runNew` (the still-rows trips, 2026-11-11): every configuration gets a run newer than its last, as a sync
 * that saw builds finish brings them.
 */
export function addFakeRuns(now: Date = new Date()): void {
  history(now);
  if (built === null) return;
  for (const [config, runs] of built.runs) {
    if (runs.length === 0) continue;
    const no = Math.max(...runs.map((run) => Number(run.number ?? 0))) + 1;
    runs.unshift(runRow(config, no, "success", "main", new Date(now.getTime() - 60_000), 120));
  }
}

onFakeReset(() => {
  built = null;
  longPaths = false;
});

/** {@link history} built from scratch, from the seed, for the day of `now`. */
function buildHistory(now: Date): Map<string, RunRow[]> {
  const random = seeded(7);
  const out = new Map<string, RunRow[]>();
  let index = 0;
  for (const [path, configs] of Object.entries(CONFIGURATIONS)) {
    for (const config of configs) {
      index += 1;
      const story = LATEST[config] ?? {
        no: 100 + Math.floor(random() * 2000),
        st: "success",
        branch: random() < 0.8 ? "main" : `feature/retry-${Math.floor(random() * 90)}`,
        at: `days ${Math.floor(random() * 4)} ${String(8 + (index % 10)).padStart(2, "0")}:${String((index * 7) % 60).padStart(2, "0")}`,
        dur: 60 + Math.floor(random() * 540),
      };
      const base = story.dur ?? 180;
      const runs = [runRow(config, story.no, story.st, story.branch, instant(story.at, now), story.dur)];
      let no = story.no;
      let at = instant(story.at, now).getTime();
      const features = path.startsWith("Payments/Payouts") ? ["feature/PAY-231-sepa-retry", "feature/csv-export"] : ["feature/cleanup-87"];
      for (const after of AFTER_STORY[config] ?? []) {
        runs.push(runRow(config, after.no, after.st, after.branch, instant(after.at, now), after.dur));
        no = after.no;
        at = instant(after.at, now).getTime();
      }
      for (let i = runs.length; i < 34; i += 1) {
        no -= 1 + Math.floor(random() * 2);
        at -= (60 + Math.floor(random() * 700)) * 60_000;
        const st = random() < 0.05 ? "failed" : "success";
        const branch = /Prod/.test(config) || random() < 0.55 ? "main" : features[Math.floor(random() * features.length)]!;
        runs.push(runRow(config, no, st, branch, new Date(at), Math.round(base * (0.8 + random() * 0.45))));
      }
      out.set(config, runs);
    }
  }
  return out;
}

/** The unread failures among `runs`: what a pipeline row's `unread` names. */
function unreadOf(runs: readonly RunRow[]): string[] {
  return runs.filter((run) => failedFake(run) && !readRuns.has(run.entity_id)).map((run) => run.entity_id);
}

/** `pipelines`: every configuration by name, each with its last twelve runs and its unread failures (#2543). */
export function fakePipelines(now: Date = new Date()): PipelineRow[] {
  const runs = history(now);
  return Object.entries(CONFIGURATIONS)
    .flatMap(([path, configs]) =>
      configs.map((config) => ({
        entity_id: `${SOURCE}:buildType:${config}`,
        source_id: SOURCE,
        source_name: longPaths ? LONG_SOURCE_NAME : SOURCE_NAME,
        name: config,
        group: path.split("/").at(-1) ?? path,
        path: path.split("/"),
        pinned: pins.has(`${SOURCE}:buildType:${config}`),
        web_url: `https://ci.tidewater.example/buildConfiguration/${config}`,
        recent: (runs.get(config) ?? []).slice(0, 12),
        // Every unread failure of the mirrored history, past the twelve too (#2543), as `PIPELINES` names them.
        unread: unreadOf(runs.get(config) ?? []),
      })),
    )
    .sort((a, b) => a.name.toLowerCase().localeCompare(b.name.toLowerCase()));
}

/**
 * `job_pipelines`: *your branch* first, then *PR check*, as the read orders them.
 * The crate's read starts from the job's reach, as the card's Builds count does, so
 * only a configuration the job reaches (`job.checks`, `job-reach.ts`) has a reason.
 */
export function fakeJobPipelines(entityId: string, now: Date = new Date()): JobPipeline[] {
  const job = JOBS[keyOf(entityId)];
  if (job === undefined) return [];
  const runs = history(now);
  return fakePipelines(now)
    .map((pipeline) => {
      const config = pipeline.name;
      const reasons: PipelineReason[] = [];
      const built = new Set((runs.get(config) ?? []).map((run) => run.branch));
      if (!job.checks.includes(config)) return { pipeline_id: pipeline.entity_id, reasons };
      for (const branch of job.branches) if (built.has(branch)) reasons.push({ kind: "branch", branch });
      reasons.push({ kind: "pr_check", pr: job.pr });
      return { pipeline_id: pipeline.entity_id, reasons };
    })
    .filter((row) => row.reasons.length > 0);
}

/** How far back a configuration's history goes, in runs before its story: the scroll ends. */
const OLDER_DEPTH = 200;

/**
 * `older_pipeline_runs` (#851): the runs before `before`, continuing the
 * history the way it was filled, from a seed of the run it starts from, so
 * two scrolls agree. A history ends {@link OLDER_DEPTH} runs before the story.
 */
export function fakeOlderRuns(config: string, before: RunRow, limit: number, story: number, depth = OLDER_DEPTH): RunRow[] {
  const random = seeded(Number(before.number ?? 1) * 31 + config.length);
  const base = before.duration_seconds ?? 180;
  const out: RunRow[] = [];
  let no = Number(before.number ?? 0);
  let at = new Date(before.started ?? before.updated_at ?? Date.now()).getTime();
  while (out.length < limit) {
    no -= 1 + Math.floor(random() * 2);
    if (no <= story - depth) break;
    at -= (60 + Math.floor(random() * 700)) * 60_000;
    const st = random() < 0.05 ? "failed" : "success";
    const branch = /Prod/.test(config) || random() < 0.55 ? "main" : "feature/cleanup-87";
    out.push(runRow(config, no, st, branch, new Date(at), Math.round(base * (0.8 + random() * 0.45))));
  }
  return out;
}

/** One of prototype-6's `paramDefs`: a name, its default, its options or hint. */
type ParamDef = { k: string; def: string; opts?: string[]; hint?: string; inherited?: boolean };

/** prototype-6's `paramDefs`, word for word. */
function paramDefs(config: string): ParamDef[] {
  if (config === "Payout_IntegrationTests")
    return [
      { k: "env", opts: ["staging", "prod-dryrun"], def: "staging", inherited: true },
      { k: "test.filter", def: "", hint: "empty runs every test" },
      { k: "log.level", opts: ["info", "debug", "trace"], def: "info" },
    ];
  if (/Deploy/.test(config))
    return [
      { k: "version", def: "1.9.0-rc1" },
      { k: "env", opts: [/Prod/.test(config) ? "prod" : "staging"], def: /Prod/.test(config) ? "prod" : "staging" },
      { k: "dry.run", opts: ["false", "true"], def: "false" },
    ];
  return [
    { k: "profile", opts: ["release", "debug"], def: "release" },
    { k: "env", opts: ["staging", "prod-dryrun"], def: "staging" },
  ];
}

/** The settings read's parameters, from {@link paramDefs}. */
function parameters(config: string): Setting[] {
  return paramDefs(config).map((p) => ({ name: p.k, value: p.def, inherited: p.inherited ?? false, secret: false, editable: true, description: null }));
}

/** The settings read's parsed specs (#852), from {@link paramDefs}. */
function parameterSpecs(config: string): ParameterSpec[] {
  return paramDefs(config).map((p) => ({
    name: p.k,
    label: null,
    hint: p.hint ?? null,
    kind: p.opts ? { type: "select", options: p.opts.map((value) => ({ value, label: null })) } : { type: "text" },
  }));
}

/** prototype-6's `S.presets`: one preset, kept for the session as the app keeps them (#852). */
const presets = new Map<string, RunPreset[]>([
  [
    `${SOURCE}:buildType:Payout_IntegrationTests`,
    [
      {
        pipeline_id: `${SOURCE}:buildType:Payout_IntegrationTests`,
        name: "Only the retry tests",
        branch: null,
        parameters: [
          { name: "test.filter", value: "sepa::retry" },
          { name: "log.level", value: "debug" },
        ],
        clean_checkout: false,
        first_in_queue: false,
      },
    ],
  ],
]);

/** prototype-6's `prTitles`, for a run's one change. */
const PR_TITLES = [
  "Bump dependencies", "Add tracing spans", "Fix flaky test", "Retry on 429", "Split handler module",
  "Add OpenAPI examples", "Remove dead feature flag", "Cache lookup results", "Validate input dates", "Batch inserts",
  "Upgrade to Kotlin 2.1", "Make timeouts configurable", "Add health endpoint", "Paginate list endpoint", "Log request ids",
  "Speed up CI cache", "Add contract test", "Handle empty payloads", "Move config to env", "Add metrics for queue depth",
];

/** prototype-6's run facts: who started it (`trigLabel`), the commit, the agent. */
function storyOf(config: string, no: number) {
  const commits: Record<string, string> = {
    "Payout_IntegrationTests-1187": "a41f2c",
    "Payout_IntegrationTests-1186": "91fe0c",
    "Payout_IntegrationTests-1185": "7be0e4",
    "Payout_Build-1188": "c90d11",
    "Payout_Build-1184": "a41f2c",
  };
  const commit = commits[`${config}-${no}`] ?? (0x5e3a1 + no * 977).toString(16).padStart(6, "0").slice(0, 6);
  const by = config === "Payout_Build" && no === 1188 ? "you" : `VCS change ${commit}`;
  // `by` stays the view's *Triggered by* fact, which the source's own view
  // still draws; the shell reads the run's declared trigger (`startOf`).
  const agent = config === "Payout_IntegrationTests" && no === 1187 ? "linux-agent-03" : `linux-agent-0${1 + (no % 4)}`;
  return { commit, by, agent };
}

/** prototype-6's `artifactsOf`, as TeamCity's top-level listing draws it: a folder with its count. */
function artifactRows(config: string, no: number) {
  const row = (title: string, meta: string, file: boolean) => ({
    entity_id: null,
    title,
    meta,
    tone: null,
    chip: null,
    url: file ? `https://ci.tidewater.example/repository/download/${config}/${no}:id/${title}` : null,
  });
  if (config === "Payout_IntegrationTests") return [row("test-report/", "2 files", false), row("logs/", "2 files", false), row("coverage/", "1 file", false)];
  if (/Build|Docker|Release/.test(config))
    return [
      row("payout-service-1.9.0-rc1-x86_64.tar.gz", "6.2 MB", true),
      row("payout-service", "18.4 MB", true),
      row("sbom.cdx.json", "240 KB", true),
      row("SHA256SUMS", "1 KB", true),
    ];
  return [row("report.txt", "4 KB", true)];
}

/** prototype-6's failing test of #1187 and #1185, as the adapter's *Tests* draws one. */
const FAILED_TEST: ViewBlock[] = [
  {
    type: "entities",
    title: null,
    rows: [{ entity_id: null, title: "sepa::retry::gives_up_after_max_attempts", meta: null, tone: "err", chip: "new", url: null }],
  },
  { type: "text", markdown: false, text: "assertion `left == right` failed\n  left: 6\n right: 5\n  at tests/sepa_retry.rs:14:5" },
];

/**
 * A run's source view (#852's *#N settings*, #853's run page): the whole of
 * it, as TeamCity's adapter answers a build, or the one section asked for.
 * The facts are prototype-6's (`trigLabel`, the commit, the agent), the
 * parameters what its run *Parameters* tab says (`env` staging, the rest their
 * defaults), the artifacts its `artifactsOf` at the top level, and the
 * changes its *Changes* tab. `undefined` for an id that is not a fixture run.
 */
/** A fixture run's configuration, number and result, or `undefined` for an id that is not one (#854 to #856's reads use it too). */
export function fixtureRun(id: string): { config: string; no: number; st: Latest["st"] } | undefined {
  const match = /^teamcity:build:(.+)-(\d+)$/.exec(id);
  if (match === null) return undefined;
  const config = match[1]!;
  const no = Number(match[2]);
  if (!Object.values(CONFIGURATIONS).some((list) => list.includes(config))) return undefined;
  const latest = LATEST[config];
  const st = latest?.no === no ? latest.st : (AFTER_STORY[config]?.find((r) => r.no === no)?.st ?? "success");
  return { config, no, st };
}

/**
 * A fixture build or build configuration as the mirror holds its row (#2137),
 * for `get_entity` of an id the corpus does not hold, so the run and
 * pipeline pages draw their *Refresh*; `undefined` for an id that is neither.
 */
export function fixtureBuildEntity(
  id: string,
): { kind: "build" | "build_config"; title: string; path: string; source: string } | undefined {
  const run = fixtureRun(id);
  const configPrefix = `${SOURCE}:buildType:`;
  const config = run?.config ?? (id.startsWith(configPrefix) ? id.slice(configPrefix.length) : null);
  const path = Object.entries(CONFIGURATIONS).find(([, list]) => config !== null && list.includes(config))?.[0];
  if (config === null || path === undefined) return undefined;
  return run !== undefined
    ? { kind: "build", title: `${config} #${run.no}`, path, source: SOURCE }
    : { kind: "build_config", title: config, path, source: SOURCE };
}

export function runView(id: string, section: string | null): SourceView | undefined {
  const found = fixtureRun(id);
  if (found === undefined) return undefined;
  const { config, no, st } = found;
  const done = st === "success" || st === "failed";
  const story = storyOf(config, no);
  const repo = repositoryOf(config);
  const commitId = repo === null ? null : `mock:tidewater/${repo}@${story.commit}`;
  const values = paramDefs(config).map((p) => ({
    name: p.k,
    value: p.k === "env" && p.opts?.includes("staging") ? "staging" : p.def,
    inherited: false,
    secret: false,
    editable: false,
    description: null,
  }));
  const failing = config === "Payout_IntegrationTests" && st === "failed";
  const facts: Fact[] = [
    { label: "Outcome", value: st === "success" ? "passed" : st, tone: st === "success" ? "ok" : st === "failed" ? "err" : "info" },
    { label: "Number", value: `#${no}`, tone: null },
    { label: "Triggered by", value: story.by, tone: null },
    ...(st === "running" ? [{ label: "Now", value: "Step 3 of 5 is running.", tone: null }] : []),
    { label: "Agent", value: story.agent, tone: null },
  ];
  const changes: [string, string][] =
    config === "Payout_IntegrationTests" && no === 1187
      ? [
          ["a41f2c", "PAY-231 backoff jitter"],
          ["7be0e4", "PAY-231: retry SEPA payouts on transient PSP errors"],
        ]
      : [[story.commit, PR_TITLES[no % PR_TITLES.length]!]];
  const entities = (rows: ViewRow[], title: string | null = null): ViewBlock => ({ type: "entities", title, rows });
  const tab = (tabId: string, title: string, count: number | null, blocks: ViewBlock[], deferred = false): ViewSection => ({ id: tabId, title, count, deferred, blocks });
  const sections = [
    tab("overview", "Overview", null, [
      { type: "facts", facts },
      entities([{ entity_id: `${SOURCE}:buildType:${config}`, title: `Payouts / ${config}`, meta: null, tone: null, chip: null, url: null }], "Configuration"),
      entities([{ entity_id: commitId, title: repo ?? config, meta: null, tone: null, chip: story.commit, url: null }], "Revision"),
    ]),
    tab(
      "changes",
      "Changes",
      changes.length,
      [entities(changes.map(([sha, title]) => ({ entity_id: repo === null ? null : `mock:tidewater/${repo}@${sha}`, title, meta: "mara", tone: null, chip: sha, url: null })))],
    ),
    tab("tests", "Tests", failing ? 1 : 0, section === "tests" && failing ? FAILED_TEST : [], section !== "tests"),
    tab("log", "Log", null, [], section !== "log"),
    tab("artifacts", "Artifacts", done ? artifactRows(config, no).length : 0, [entities(done ? artifactRows(config, no) : [])]),
    tab("parameters", "Parameters", values.length, [{ type: "settings", title: null, entity: id, write_op: "", values }]),
  ];
  return { sections: section === null ? sections : sections.filter((s) => s.id === section), actions: [] };
}

/** The fixture's open pull requests by the branch each is opened from (`views/repos.ts`' branches). */
const PULL_HEADS: Record<string, { branch: string; entity_id: string; cell: string | null }[]> = {
  "payout-service": [
    { branch: "feat/PAY-231-idempotent-retry", entity_id: "mock:payout-service#142", cell: "#142" },
    { branch: "feature/csv-export", entity_id: "mock:payout-service#150", cell: "#150" },
  ],
};

/** prototype-6's `repoOfCfg`: the repository a configuration builds, by the mock's repository ids. */
function repositoryOf(config: string): string | null {
  if (/^(Payout_|Dashboard_)/.test(config)) return "payout-service";
  if (/^PspClient/.test(config)) return "psp-client";
  if (/^(Ledger|FxRates)/.test(config)) return "ledger-api";
  return null;
}

/** `pipeline_settings` (#851): prototype-6's repository, triggers and parameters. */
export function fakeSettings(config: string): PipelineSettings {
  const repo = repositoryOf(config);
  return {
    repositories:
      repo === null
        ? []
        : longPaths
          ? longRepositories(repo)
          : [
              {
                name: repo,
                url: `http://gitea.example/tidewater/${repo}.git`,
                default_branch: "refs/heads/main",
                entity_id: `mock:tidewater/${repo}`,
              },
            ],
    triggers: [
      {
        name: "VCS",
        facts: [
          { label: "Branch filter", value: "+:refs/heads/*", tone: null },
          { label: "Quiet period", value: "60 s", tone: null },
        ],
        disabled: false,
      },
      {
        name: "Schedule",
        facts: [
          { label: "When", value: "daily at 02:00", tone: null },
          { label: "Branch filter", value: "+:main", tone: null },
        ],
        disabled: false,
      },
    ],
    parameters: parameters(config),
    parameter_write_op: "set_parameter",
    parameter_specs: parameterSpecs(config),
    chain: chainOf(config).map((step) => ({ pipeline: `${SOURCE}:buildType:${stepId(step)}`, name: step })),
    steps: stepNames(config),
  };
}

/**
 * The steps a run of the configuration takes (#1016), from prototype-6's
 * `stepsOf`: `Payout_Build`'s are its running run's five, of which 1188's log
 * has reached three.
 */
function stepNames(config: string): string[] {
  if (config === "Payout_IntegrationTests")
    return ["Checkout", "Start PSP sandbox stub", "cargo build --tests", "cargo test --test sepa_retry", "Publish test report"];
  if (config === "Payout_Build") return ["Checkout", "cargo build --release", "cargo test", "Build image", "Publish artifacts"];
  return ["Checkout", "Build", "Test", "Publish"];
}

/** prototype-6's `CHAINS` (#857): TeamCity snapshot dependencies, first step first. */
const CHAINS = [
  ["Payout_Build", "Payout_Docker", "Payout_Deploy_Staging", "Payout_Deploy_Prod"],
  ["Ledger_Build", "Ledger_Tests", "Ledger_Deploy_Staging", "Ledger_Deploy_Prod"],
];

function chainOf(config: string): string[] {
  return (longPaths ? [LONG_CHAIN, ...CHAINS.slice(1)] : CHAINS).find((chain) => chain.includes(config)) ?? [];
}

/**
 * `?fake-long-paths` (the build-header pass, 2026-11-20: Björn's "Teamcity configuration, builds <huge repo link> step
 * 64 of the chain x > y > z > foo > bar > etc."): the payouts chain seventy steps long, `Payout_Docker` its 64th, the
 * steps around it with long names, and each repository a VCS root named as TeamCity names one by default, by its whole
 * address and branch, with a second root the mirror does not hold.
 */
let longPaths = false;
/** `?fake-long-paths`' display name: *Open in ⟨source⟩* and the header's line cut it, whole in their titles. */
const LONG_SOURCE_NAME = "Tidewater TeamCity for the payments team's release trains, EU data centre";
const LONG_STEP_WORDS = [
  "reconcile the payments team's ledger snapshots against the settlement bank's end-of-day statement",
  "publish the payments team's signed release candidate to the artifact repository for the audit trail",
  "run the payments team's contract tests against every carrier and payment service provider sandbox",
];
const longStep = (no: number) => `Release_Train_${String(no).padStart(2, "0")}: ${LONG_STEP_WORDS[no % LONG_STEP_WORDS.length]}`;
const LONG_CHAIN = [
  "Payout_Build",
  ...Array.from({ length: 62 }, (_, i) => longStep(i + 2)),
  "Payout_Docker",
  "Payout_Deploy_Staging",
  ...Array.from({ length: 4 }, (_, i) => longStep(i + 66)),
  "Payout_Deploy_Prod",
];
/** A long step's configuration id: its name up to the colon. */
const stepId = (step: string) => step.split(":")[0]!;
function longRepositories(repo: string): PipelineSettings["repositories"] {
  const address = `https://git.tidewater.example/payments-platform/${repo}-repository-name-that-is-long.git`;
  const scripts = "ssh://git@git.tidewater.example:7999/payments-platform/shared-build-scripts-repository-name-that-is-long.git";
  return [
    { name: `${address}#refs/heads/main`, url: address, default_branch: "refs/heads/main", entity_id: `mock:tidewater/${repo}` },
    { name: `${scripts}#refs/heads/main`, url: scripts, default_branch: "refs/heads/main", entity_id: null },
  ];
}

/**
 * `run_chain` (#857): each step's newest run on the run's branch, the run
 * itself for its own step, as prototype-6's `chainPanel` reads `runs`.
 */
export function fakeRunChain(runId: string, now: Date = new Date()): ChainStepRow[] {
  const runs = history(now);
  const config = [...runs.keys()].find((c) => runs.get(c)!.some((r) => r.entity_id === runId));
  if (config === undefined) return [];
  const run = runs.get(config)!.find((r) => r.entity_id === runId)!;
  return chainOf(config).map((step) => ({
    pipeline_id: `${SOURCE}:buildType:${stepId(step)}`,
    name: step,
    run: step === config ? run : ((runs.get(step) ?? []).find((r) => r.branch === run.branch) ?? null),
  }));
}

/** The configuration an id names, or a `not_found` like the command's. */
function configOf(id: string): string {
  const config = id.replace(`${SOURCE}:buildType:`, "");
  if (!id.startsWith(`${SOURCE}:buildType:`) || !Object.values(CONFIGURATIONS).some((list) => list.includes(config))) {
    throw { code: "not_found", message: `${id} is not a pipeline in the mirror`, source_id: null };
  }
  return config;
}

/**
 * `?fake-pipelines=<n>` (perf audit, #2200's class): `n` more configurations
 * on a second CI, *Bulk CI*, in projects of eight under twenty areas, each
 * with twelve runs, one in fifty failing last. Ids start `bulk:`.
 */
export function bulkPipelines(n: number, now: Date = new Date()): PipelineRow[] {
  const out: PipelineRow[] = [];
  for (let i = 0; i < n; i += 1) {
    const area = `Area ${String(i % 20).padStart(2, "0")}`;
    const project = `Service ${String(Math.floor(i / 8)).padStart(4, "0")}`;
    const config = `Bulk_${String(i).padStart(5, "0")}`;
    const recent: RunRow[] = [];
    for (let r = 0; r < 12; r += 1) {
      const st = r === 0 && i % 50 === 7 ? "failed" : "success";
      const row = runRow(config, 500 - r, st, "main", new Date(now.getTime() - (r * 90 + (i % 60)) * 60_000), 240);
      recent.push({ ...row, entity_id: `bulk:build:${config}-${500 - r}` });
    }
    out.push({
      entity_id: `bulk:buildType:${config}`,
      source_id: "bulk",
      source_name: "Bulk CI",
      name: config,
      group: project,
      path: [area, project],
      pinned: false,
      web_url: null,
      recent,
      unread: [],
    });
  }
  return out;
}

export function handlers(params: URLSearchParams = new URLSearchParams()): Handlers {
  const bulk = bulkPipelines(Number(params.get("fake-pipelines") ?? 0) || 0);
  const bulkById = new Map(bulk.map((p) => [p.entity_id, p]));
  // `fake-scale` (`fake-scale.ts`): a history thousands of runs long.
  const depth = params.has("fake-scale") ? SCALE_HISTORY : OLDER_DEPTH;
  longPaths = params.has("fake-long-paths");
  return {
    older_pipeline_runs: (args) => {
      const config = configOf(String(args["pipelineId"] ?? ""));
      const before = String(args["before"] ?? "");
      const limit = typeof args["limit"] === "number" ? args["limit"] : 50;
      const mirrored = history(new Date()).get(config) ?? [];
      // The whole history beyond the mirror, continued from its oldest run:
      // the same chain on every call, so pages join up.
      const all = [...mirrored, ...fakeOlderRuns(config, mirrored.at(-1)!, depth, Number(mirrored[0]?.number ?? 0), depth)];
      const at = all.findIndex((run) => run.entity_id === before);
      return at < 0 ? [] : all.slice(at + 1, at + 1 + limit);
    },
    pipeline_settings: (args) => fakeSettings(configOf(String(args["pipelineId"] ?? ""))),
    run_chain: (args) => fakeRunChain(String(args["runId"] ?? "")),
    run_presets: (args) => presets.get(String(args["pipelineId"] ?? "")) ?? [],
    save_run_preset: (args) => {
      const preset = args["preset"] as RunPreset;
      const name = preset.name.trim();
      const kept = (presets.get(preset.pipeline_id) ?? []).map((p) =>
        p.name.trim().toLowerCase() === name.toLowerCase() ? { ...preset, name } : p,
      );
      if (!kept.some((p) => p.name === name)) kept.push({ ...preset, name });
      presets.set(preset.pipeline_id, kept);
      return { ...preset, name };
    },
    delete_run_preset: (args) => {
      const id = String(args["pipelineId"] ?? "");
      const name = String(args["name"] ?? "").trim();
      const held = presets.get(id) ?? [];
      const kept = held.filter((p) => p.name.trim().toLowerCase() !== name.toLowerCase());
      if (kept.length === held.length) {
        throw { code: "not_found", message: `the pipeline has no preset named "${name}"`, source_id: null };
      }
      presets.set(id, kept);
      return null;
    },
    // CI's read by definition, so it narrows itself to the role's fillers, as the server's does (#1295).
    pipelines: () => {
      const fillers = new Set(fakeRoleScope(CI_ROLE).sources);
      return [...fakePipelines().filter((pipeline) => fillers.has(pipeline.source_id)), ...bulk.map((p) => ({ ...p, unread: unreadOf(p.recent) }))];
    },
    // Mark builds as read (ux18, ADR-0063): the fixture's own sets, as `knobas.run_read` holds them.
    read_runs: (): RunMarks => ({ read: [...readRuns], while_running: [...whileRunningRuns] }),
    mark_runs_read: (args): RunMarks => {
      const scope = (args["scope"] as Partial<MarkScope> | undefined) ?? {};
      const all = history(new Date());
      const every = [...all.values()].flat().concat(bulk.flatMap((p) => p.recent));
      const runsOf = (id: string) => [...(all.get(configOf(id)) ?? []), ...(bulkById.get(id)?.recent ?? [])];
      const inScope = scope.everything === true ? every : (scope.pipelines ?? []).flatMap(runsOf);
      const scoped = inScope.filter((run) => failedFake(run) || (run.started !== null && run.finished === null));
      const known = new Map(every.map((run) => [run.entity_id, run]));
      const wrote: RunMarks = { read: [], while_running: [] };
      const named = (scope.runs ?? []).map((id) => known.get(id) ?? { entity_id: id, finished: "", started: null });
      for (const run of new Map([...scoped, ...named].map((one) => [one.entity_id, one])).values()) {
        if (readRuns.has(run.entity_id)) continue;
        if (run.finished === null) {
          if (whileRunningRuns.has(run.entity_id)) continue;
          whileRunningRuns.add(run.entity_id);
          wrote.while_running.push(run.entity_id);
        } else {
          whileRunningRuns.delete(run.entity_id);
          readRuns.add(run.entity_id);
          wrote.read.push(run.entity_id);
        }
      }
      return wrote;
    },
    mark_run_opened: (args) => {
      const id = String(args["run"] ?? "");
      if (id === "" || readRuns.has(id)) return false;
      whileRunningRuns.delete(id);
      readRuns.add(id);
      return true;
    },
    mark_runs_unread: (args) => {
      const gone = ((args["runs"] as string[] | undefined) ?? []).filter((id) => readRuns.has(id) || whileRunningRuns.has(id));
      for (const id of gone) (readRuns.delete(id), whileRunningRuns.delete(id));
      return gone;
    },
    run_counts: (): RunCounts => {
      const fillers = new Set(fakeRoleScope(CI_ROLE).sources);
      const all = history(new Date());
      const drawn = fakePipelines().filter((pipeline) => fillers.has(pipeline.source_id));
      return fakeRunCounts(
        [
          ...drawn.map((pipeline) => ({ id: pipeline.entity_id, runs: all.get(configOf(pipeline.entity_id)) ?? pipeline.recent })),
          ...bulk.map((pipeline) => ({ id: pipeline.entity_id, runs: pipeline.recent })),
        ],
        params.get("fake-counts"),
      );
    },
    move_seen: (args) => fakeMoveSeen((args["scope"] as Partial<MarkScope> | undefined) ?? {}),
    put_back_seen: (args) => fakePutBackSeen(args["seen"] as SeenMove),
    builds_in_front: (args) => fakeBuildsInFront(args["front"] === true),
    set_pipeline_pinned: (args) => {
      const id = String(args["pipelineId"] ?? "");
      if (args["pinned"] === true) pins.add(id);
      else pins.delete(id);
      return pins.has(id);
    },
    job_pipelines: (args) => fakeJobPipelines(String(args["entityId"] ?? "")),
    // The repository's Builds tab: the pipelines that build it (prototype-6's `repoOfCfg`), each with its newest run per
    // branch, as the server reads the runs of the repository's commits. A queued run has built no commit yet.
    repo_pipelines: (args) => {
      const repo = String(args["repoId"] ?? "").replace(/^mock:tidewater\//, "");
      const fillers = new Set(fakeRoleScope(CI_ROLE).sources);
      const runs = history(new Date());
      const pipelines = fakePipelines()
        .filter((pipeline) => fillers.has(pipeline.source_id) && repositoryOf(configOf(pipeline.entity_id)) === repo)
        .map(({ recent: _recent, pinned: _pinned, ...pipeline }) => {
          const seen = new Set<string>();
          const latest: RunRow[] = [];
          for (const run of runs.get(configOf(pipeline.entity_id)) ?? []) {
            if (run.branch === null || run.started === null || seen.has(run.branch)) continue;
            seen.add(run.branch);
            latest.push(run);
          }
          return { ...pipeline, latest };
        });
      return { pipelines, pull_requests: PULL_HEADS[repo] ?? [] };
    },
    // The header's *New pull request*: the fixture's mirror says nothing of its branches, so the header reads each branch's
    // view as it did before the mirror could answer (the forge-rounds pass, 2026-11-15).
    // `?fake-branch-pulls` answers it as the real mirror does (`payoutBranchPulls`, the compare-first-open pass).
    repo_branch_pulls: (args) =>
      new URLSearchParams(globalThis.location?.search).has("fake-branch-pulls") ? payoutBranchPulls(String(args["repoId"] ?? "")) : [],
    // *By repository*: a fixture commit's id names its repository (`ranOn`), as the mirror's stored parent does.
    commit_repositories: (args) =>
      ((args["commitIds"] as string[] | undefined) ?? []).flatMap((commitId) => {
        const repo = /^mock:(tidewater\/[^@]+)@/.exec(commitId)?.[1];
        return repo === undefined ? [] : [{ commit_id: commitId, repo_id: `mock:${repo}`, repo_title: repo }];
      }),
    // Builds' *Recent* (2026-11-25): every drawn pipeline's runs, newest first by when each started or was queued, as
    // `run_made_idx` orders them, narrowed as the server narrows them, a page after `before`.
    recent_runs: (args) => {
      const filter = (args["filter"] ?? {}) as Partial<RecentFilter>;
      const before = (args["before"] ?? null) as RecentCursor | null;
      const limit = typeof args["limit"] === "number" ? args["limit"] : 200;
      const fillers = new Set(fakeRoleScope(CI_ROLE).sources);
      const all = history(new Date());
      const drawn = [...fakePipelines().filter((pipeline) => fillers.has(pipeline.source_id)), ...bulk].filter(
        (pipeline) =>
          ((filter.sources ?? []).length === 0 || filter.sources!.includes(pipeline.source_id)) &&
          // A folder's scope (#2570): its pipelines by id.
          ((filter.pipelines ?? []).length === 0 || filter.pipelines!.includes(pipeline.entity_id)),
      );
      const byId = new Map(drawn.map((pipeline) => [pipeline.entity_id, pipeline]));
      const runs = drawn
        .flatMap((pipeline) => (bulkById.get(pipeline.entity_id)?.recent ?? all.get(configOf(pipeline.entity_id)) ?? []).map((run) => ({ pipeline_id: pipeline.entity_id, run })))
        .filter(({ run }) => {
          const result = run.finished === null ? (run.started === null ? "Queued" : "Running") : run.state?.tone === "err" ? "Failed" : run.state?.tone === "ok" ? "Passed" : null;
          if ((filter.results ?? []).length > 0 && (result === null || !filter.results!.includes(result))) return false;
          if (filter.mine === true && !run.started_by_me) return false;
          return filter.branches == null || (run.branch !== null && filter.branches.includes(run.branch));
        })
        .sort((a, b) => recentOrder(a.run, b.run));
      const from = before === null ? 0 : runs.findIndex(({ run }) => run.entity_id === before.entity_id) + 1;
      const page = runs.slice(from, from + limit);
      const last = page.at(-1);
      const pipelines = [...new Set(page.map((one) => one.pipeline_id))].map((id) => {
        const { recent: _recent, pinned: _pinned, web_url: _web, ...named } = byId.get(id)!;
        return named;
      });
      return {
        runs: page,
        pipelines,
        next: page.length === limit && last !== undefined ? { made: last.run.started ?? last.run.updated_at ?? "-infinity", entity_id: last.run.entity_id } : null,
      };
    },
    pipeline_runs: (args) => {
      const id = String(args["pipelineId"] ?? "");
      const bulkRow = bulkById.get(id);
      if (bulkRow !== undefined) return bulkRow.recent;
      const config = id.replace(`${SOURCE}:buildType:`, "");
      const runs = history(new Date()).get(config);
      if (!id.startsWith(`${SOURCE}:buildType:`) || runs === undefined) {
        throw { code: "not_found", message: `${id} is not a pipeline in the mirror`, source_id: null };
      }
      const limit = typeof args["limit"] === "number" ? args["limit"] : 100;
      return runs.slice(0, limit);
    },
  };
}

keepSeed(pins, presets, readRuns, whileRunningRuns, seen);
