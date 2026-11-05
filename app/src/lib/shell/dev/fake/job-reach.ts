/**
 * `?fake-ipc`: what a job reaches (#817) and its *Start work* flow (#818).
 *
 * - `JOB_ROWS`: the mirror rows prototype-6's PAY-231 job draws beyond the
 *   demo corpus -- its builds, monitors and pages -- spread into `CORPUS` by
 *   `corpus.ts`, so `list_entities` and `get_entity` answer for them. A row's
 *   state chip and path ride in the payload as `chip` and `path`, and a row
 *   from another source names it as `source`: fixture-only keys `mirrorRow`
 *   reads, since the mock adapter declares no state.
 * - `fakeReach`: `job_reach`, as a table rather than a walk. PAY-231 reaches
 *   its code, builds, pages, the testenv estate's Gitea and Jira containers,
 *   and those containers' monitors; PAY-232 and PAY-233, sub-tasks of PAY-231,
 *   reaches its parent's own links labelled "Parent PAY-231" (R-7.3), unless
 *   the switch is off. A restatement, which certifies nothing: the walk is
 *   witnessed in `knobas-core`'s `tests/it/reach.rs`.
 * - The start-work flow's five commands, stateful for the session, so the
 *   stepper on PAY-240's job can be walked: proposing composes the four steps
 *   the backend proposes, and *Run all steps* leaves the writes queued and the
 *   link done, as prototype-6 draws a run.
 *
 * This module imports nothing from `corpus.ts`: `corpus.ts` imports it, and a
 * cycle would read `CORPUS` before it exists.
 */
import type { Reached, StartWorkStep } from "../../../ipc/entity";
import type { Handlers } from "../fake-tauri";
import { keepSeed, onFakeReset, seedOf } from "../fake-seed";
import { keyOf } from "../../../time/format";

const HOST = "http://127.0.0.1:5399";

/** One extra mirror row, in `CORPUS`'s shape. */
function row(
  entity_id: string,
  kind: string,
  title: string,
  updated_at: string,
  payload: Record<string, unknown>,
) {
  return {
    entity_id,
    kind,
    title,
    updated_at,
    author: null as string | null,
    body_text: title,
    web_url: `${HOST}/browse/${keyOf(entity_id)}`,
    deleted_at: null as string | null,
    payload,
  };
}

const ok = { text: "success", tone: "ok" };
const PAYOUTS = "Payments › Payouts";

/** The rows PAY-231's job draws that the demo corpus does not hold. */
export const JOB_ROWS = [
  row("teamcity:build:Payout_Build-1188", "build", "Payout_Build #1188", "2026-08-22T11:45:00Z", {
    path: PAYOUTS,
    chip: { text: "running", tone: "info" },
    source: "teamcity",
  }),
  row("teamcity:build:Payout_IntegrationTests-1187", "build", "Payout_IntegrationTests #1187", "2026-08-22T10:10:00Z", {
    path: PAYOUTS,
    chip: { text: "failed", tone: "err" },
    source: "teamcity",
  }),
  row("teamcity:build:Payout_Lint-904", "build", "Payout_Lint #904", "2026-08-22T11:44:00Z", { path: PAYOUTS, chip: ok, source: "teamcity" }),
  row("teamcity:build:Payout_Docker-611", "build", "Payout_Docker #611", "2026-08-22T11:52:00Z", { path: PAYOUTS, chip: ok, source: "teamcity" }),
  row("teamcity:build:Payout_Deploy_Staging-318", "build", "Payout_Deploy_Staging #318", "2026-08-22T12:04:00Z", {
    path: "Payments › Payouts › Deploy",
    chip: ok,
    source: "teamcity",
  }),
  row("teamcity:build:PspClient_Build-233", "build", "PspClient_Build #233", "2026-08-20T15:20:00Z", {
    path: "Payments › PSP client",
    chip: ok,
    source: "teamcity",
  }),
  row("teamcity:build:PspClient_Release-41", "build", "PspClient_Release #41", "2026-08-20T15:31:00Z", {
    path: "Payments › PSP client",
    chip: ok,
    source: "teamcity",
  }),
  // The roster's own monitors (`monitors.ts`), so the card's read and the roster's name one row.
  row("kuma:3", "monitor", "jira", "2026-08-22T12:40:00Z", {
    cell: "TCP",
    chip: { text: "down since 12:40", tone: "err" },
    source: "kuma",
  }),
  row("kuma:5", "monitor", "gitea", "2026-08-22T12:00:00Z", {
    cell: "HTTP",
    chip: { text: "paused", tone: "muted" },
    source: "kuma",
  }),
  row("confluence:psp-codes", "page", "PSP error codes", "2026-08-19T14:00:00Z", { path: "Engineering › Payments", source: "confluence" }),
  row("mock:tidewater/payout-service@refs/heads/feature/PAY-231-sepa-retry", "branch", "feature/PAY-231-sepa-retry", "2026-08-22T09:40:00Z", {
    name: "feature/PAY-231-sepa-retry",
    repository: "tidewater/payout-service",
    path: "tidewater/payout-service",
  }),
  row("mock:tidewater/payout-service@refs/heads/feature/PAY-231-temp-unavailable", "branch", "feature/PAY-231-temp-unavailable", "2026-08-22T09:40:00Z", {
    name: "feature/PAY-231-temp-unavailable",
    repository: "tidewater/payout-service",
    path: "tidewater/payout-service",
  }),
  row("confluence:manual-review", "page", "Manual review queue", "2026-08-18T10:00:00Z", { path: "Engineering › Payments", source: "confluence" }),
];

/** What PAY-231 reaches, each entity with its first step. */
const PAY_231: [id: string, kind: string, step: string][] = [
  ["mock:tidewater/payout-service", "repo", "mock:tidewater/payout-service"],
  ["mock:tidewater/payout-service@refs/heads/feat/PAY-231-idempotent-retry", "branch", "mock:payout-service#142"],
  // The branches its runs are on (`builds.ts`), so a run's *This job* and the rail's *your branch* read one reach.
  ["mock:tidewater/payout-service@refs/heads/feature/PAY-231-sepa-retry", "branch", "mock:payout-service#142"],
  ["mock:tidewater/payout-service@refs/heads/feature/PAY-231-temp-unavailable", "branch", "mock:payout-service#142"],
  ["mock:payout-service#142", "pr", "mock:payout-service#142"],
  ["mock:9f2c1ab", "commit", "mock:payout-service#142"],
  ["teamcity:build:Payout_Build-1188", "build", "mock:payout-service#142"],
  ["teamcity:build:Payout_IntegrationTests-1187", "build", "mock:payout-service#142"],
  ["teamcity:build:Payout_Lint-904", "build", "mock:payout-service#142"],
  ["teamcity:build:Payout_Docker-611", "build", "mock:payout-service#142"],
  ["teamcity:build:Payout_Deploy_Staging-318", "build", "mock:payout-service#142"],
  ["teamcity:build:PspClient_Build-233", "build", "mock:tidewater/payout-service"],
  ["teamcity:build:PspClient_Release-41", "build", "mock:tidewater/payout-service"],
  ["confluence:sepa", "page", "confluence:sepa"],
  ["confluence:psp-codes", "page", "confluence:psp-codes"],
  ["confluence:manual-review", "page", "confluence:manual-review"],
  ["asset:knobas-gitea", "asset", "asset:knobas-gitea"],
  ["kuma:5", "monitor", "asset:knobas-gitea"],
  ["asset:knobas-jira", "asset", "asset:knobas-jira"],
  ["kuma:3", "monitor", "asset:knobas-jira"],
];

/** The parent each sub-task names (`fields.parent.key`). */
export const PARENTS: Record<string, string> = { "mock:PAY-232": "mock:PAY-231", "mock:PAY-233": "mock:PAY-231" };

/** R-7.3's switch, for the session. */
let parentLinks = true;

/**
 * What a ticket's link sets add to its reach (#866): `link-sets.ts` registers
 * its answer here when it loads, so this module imports nothing from it and
 * `corpus.ts`, which imports this one, stays out of a cycle.
 */
export const SET_REACH: ((ticketId: string) => Reached[])[] = [];

/** What `ticketId` reaches: its own table, then its link sets' members (#866), then its parent's direct links. */
export function fakeReach(ticketId: string): Reached[] {
  const own: Reached[] =
    ticketId === "mock:PAY-231"
      ? PAY_231.map(([entity_id, kind, step]) => ({ entity_id, kind, step, origin: { kind: "own" } }))
      : [];
  const sets = SET_REACH.flatMap((read) => read(ticketId)).filter((entry) => !own.some((mine) => mine.entity_id === entry.entity_id));
  const first = [...own, ...sets];
  const parent = PARENTS[ticketId];
  if (!parent || !parentLinks) return first;
  const inherited = ownReach(parent)
    .filter((entry) => !first.some((held) => held.entity_id === entry.entity_id))
    .map((entry) => ({ ...entry, step: parent, origin: { kind: "parent" as const, ticket: parent } }));
  return [...first, ...inherited];
}

/** The parent whose own links `ticketId` shows while R-7.3's switch is on, and those links (#867's drawer group). */
export function fakeParentOwn(ticketId: string): { ticket: string; reached: Reached[] } | null {
  const parent = PARENTS[ticketId];
  return parent && parentLinks ? { ticket: parent, reached: ownReach(parent) } : null;
}

/** The parent's own links: its direct ones, never its sets' or its own parent's. */
function ownReach(ticketId: string): Reached[] {
  return ticketId === "mock:PAY-231"
    ? PAY_231.filter(([entity_id, , step]) => entity_id === step).map(([entity_id, kind, step]) => ({ entity_id, kind, step, origin: { kind: "own" } }))
    : [];
}

// ---- the start-work flow (#44), for the session ----

const FLOWS = new Map<string, StartWorkStep[]>();
let nextStep = 1;

/** `plan::slug`: ASCII-alphanumeric runs joined by hyphens, lowercased. */
function slug(title: string): string {
  return title
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

/**
 * The titles of the fixture's To Do tickets, which a proposal names. Here
 * rather than read off `CORPUS`, for this module's no-cycle reason.
 */
const TODO_TITLES: Record<string, string> = {
  "mock:PAY-240": "Payout dashboard latency",
  "mock:PAY-236": "Payout CSV export for finance",
  "mock:PAY-232": "Metrics for SEPA retry attempts",
  "mock:PAY-233": "Document the manual-review SLA",
  "mock:OPS-77": "Rotate the staging database credentials",
};

function propose(ticketId: string, title: string, repo: string): StartWorkStep[] {
  const key = keyOf(ticketId);
  const branch = `feature/${key}-${slug(title)}`;
  const payloads: [StartWorkStep["step"], unknown][] = [
    ["create_branch", { CreateBranch: { entity: repo, name: branch, from_ref: "main" } }],
    [
      "create_pull_request",
      { CreatePullRequest: { entity: repo, title: `WIP: ${title}`, body: `${key}\n\n${title}\n\nStarted from knobas.`, head: branch, base: "main" } },
    ],
    ["link_pull_request", { relation: "implements" }],
    ["transition", { Transition: { entity: ticketId, status: "In Progress" } }],
  ];
  return payloads.map(([step, payload], position) => ({
    id: nextStep++,
    ticket_id: ticketId,
    step,
    position,
    outcome: "pending",
    payload,
    write_id: null,
    detail: null,
    updated_at: "2026-08-22T14:30:00Z",
  }));
}

function stepById(id: number): StartWorkStep {
  for (const steps of FLOWS.values()) {
    const found = steps.find((step) => step.id === id);
    if (found) return found;
  }
  throw { code: "not_found", message: `no step ${id}`, source_id: null };
}

function flowOf(step: StartWorkStep): StartWorkStep[] {
  return FLOWS.get(step.ticket_id) ?? [];
}

/** This feature's commands. */
export function handlers(): Handlers {
  return {
    job_reach: (args) => fakeReach(String(args["entityId"] ?? "")),
    parent_links: () => parentLinks,
    set_parent_links: (args) => {
      parentLinks = Boolean(args["enabled"]);
      return parentLinks;
    },
    start_work_flow: (args) => {
      const ticket = String(args["entityId"] ?? "");
      const repo = (args["repoId"] as string | null | undefined) ?? null;
      const held = FLOWS.get(ticket);
      if (held) return held;
      if (repo === null) return [];
      const steps = propose(ticket, TODO_TITLES[ticket] ?? keyOf(ticket), repo);
      FLOWS.set(ticket, steps);
      return steps;
    },
    start_work_run: (args) => {
      const steps = FLOWS.get(String(args["entityId"] ?? "")) ?? [];
      for (const step of steps) {
        if (step.outcome === "pending") step.outcome = step.step === "link_pull_request" ? "succeeded" : "queued";
      }
      return steps;
    },
    start_work_retry: (args) => flowOf(stepById(Number(args["stepId"]))),
    start_work_skip: (args) => {
      const step = stepById(Number(args["stepId"]));
      step.outcome = "skipped";
      return flowOf(step);
    },
    start_work_amend: (args) => {
      const step = stepById(Number(args["stepId"]));
      step.payload = args["payload"];
      return flowOf(step);
    },
  };
}

const atLoad = seedOf({ parentLinks });
onFakeReset(() => {
  ({ parentLinks } = atLoad());
});
keepSeed(FLOWS);
