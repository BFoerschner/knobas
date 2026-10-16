/**
 * `?fake-ipc`: Builds (#848): `pipelines` and `pipeline_runs`, over
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
import type { PipelineRow, RunRow } from "../../../ipc/entity";
import type { Handler } from "../fake-tauri";

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
    web_url: null,
  };
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
      for (let i = 1; i < 34; i += 1) {
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
        web_url: null,
        recent: (runs.get(config) ?? []).slice(0, 12),
      })),
    )
    .sort((a, b) => a.name.toLowerCase().localeCompare(b.name.toLowerCase()));
}

export function handlers(): Record<string, Handler> {
  return {
    pipelines: () => fakePipelines(),
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
