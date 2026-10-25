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
import type {
  ChainStepRow,
  JobPipeline,
  ParameterSpec,
  PipelineReason,
  PipelineRow,
  PipelineSettings,
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
import { keepSeed } from "../fake-seed";
import { fakeRoleScope } from "./corpus";
import { keyOf } from "../../../time/format";

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

/** Each configuration's runs, newest first: its story, then 33 more. */
function history(now: Date): Map<string, RunRow[]> {
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

/** `pipelines`: every configuration by name, each with its last twelve runs. */
export function fakePipelines(now: Date = new Date()): PipelineRow[] {
  const runs = history(now);
  return Object.entries(CONFIGURATIONS)
    .flatMap(([path, configs]) =>
      configs.map((config) => ({
        entity_id: `${SOURCE}:buildType:${config}`,
        source_id: SOURCE,
        source_name: SOURCE_NAME,
        name: config,
        group: path.split("/").at(-1) ?? path,
        path: path.split("/"),
        pinned: pins.has(`${SOURCE}:buildType:${config}`),
        web_url: `https://ci.tidewater.example/buildConfiguration/${config}`,
        recent: (runs.get(config) ?? []).slice(0, 12),
      })),
    )
    .sort((a, b) => a.name.toLowerCase().localeCompare(b.name.toLowerCase()));
}

/** `job_pipelines`: *your branch* first, then *PR check*, as the read orders them. */
export function fakeJobPipelines(entityId: string, now: Date = new Date()): JobPipeline[] {
  const job = JOBS[keyOf(entityId)];
  if (job === undefined) return [];
  const runs = history(now);
  return fakePipelines(now)
    .map((pipeline) => {
      const config = pipeline.name;
      const reasons: PipelineReason[] = [];
      const built = new Set((runs.get(config) ?? []).map((run) => run.branch));
      for (const branch of job.branches) if (built.has(branch)) reasons.push({ kind: "branch", branch });
      if (job.checks.includes(config)) reasons.push({ kind: "pr_check", pr: job.pr });
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
export function fakeOlderRuns(config: string, before: RunRow, limit: number, story: number): RunRow[] {
  const random = seeded(Number(before.number ?? 1) * 31 + config.length);
  const base = before.duration_seconds ?? 180;
  const out: RunRow[] = [];
  let no = Number(before.number ?? 0);
  let at = new Date(before.started ?? before.updated_at ?? Date.now()).getTime();
  while (out.length < limit) {
    no -= 1 + Math.floor(random() * 2);
    if (no <= story - OLDER_DEPTH) break;
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
  if (config === "Payout_IntegrationTests") return [row("test-report/", "2 files", false), row("logs/", "2 files", false), row("coverage/", "1 files", false)];
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
    chain: chainOf(config).map((step) => ({ pipeline: `${SOURCE}:buildType:${step}`, name: step })),
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
  return CHAINS.find((chain) => chain.includes(config)) ?? [];
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
    pipeline_id: `${SOURCE}:buildType:${step}`,
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

export function handlers(): Handlers {
  return {
    older_pipeline_runs: (args) => {
      const config = configOf(String(args["pipelineId"] ?? ""));
      const before = String(args["before"] ?? "");
      const limit = typeof args["limit"] === "number" ? args["limit"] : 50;
      const mirrored = history(new Date()).get(config) ?? [];
      // The whole history beyond the mirror, continued from its oldest run:
      // the same chain on every call, so pages join up.
      const all = [...mirrored, ...fakeOlderRuns(config, mirrored.at(-1)!, OLDER_DEPTH, Number(mirrored[0]?.number ?? 0))];
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
      return fakePipelines().filter((pipeline) => fillers.has(pipeline.source_id));
    },
    set_pipeline_pinned: (args) => {
      const id = String(args["pipelineId"] ?? "");
      if (args["pinned"] === true) pins.add(id);
      else pins.delete(id);
      return pins.has(id);
    },
    job_pipelines: (args) => fakeJobPipelines(String(args["entityId"] ?? "")),
    pipeline_runs: (args) => {
      const id = String(args["pipelineId"] ?? "");
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

keepSeed(pins, presets);
