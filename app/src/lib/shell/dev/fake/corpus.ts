/**
 * `?fake-ipc`: The demo corpus: the mirror the fixture holds, its stored contexts and its
 * one link graph, and the reads that list it (`list_entities`, the mini
 * board, the project census). Other feature modules read `CORPUS` from here.
 */
import type { AttentionGroup, CodeFacts, ContextKind, ContextRow, LaneField, Signal, StateChip } from "../../../ipc/entity";
import type { Handlers } from "../fake-tauri";
import { wordAlternatives } from "../../../tf/tickets/needles";
import { JOB_ROWS, fakeReach } from "./job-reach";
import { fakeJiraBoard, fakeLanes } from "./ticket-views";
import { keyOf } from "../../../time/format";

// -- the demo corpus --------------------------------------------------------

/**
 * Where the fixture pretends its items live.
 *
 * **Loopback, and not the mock adapter's own `tidewater.example` host.**
 * `house-rules.test.ts` refuses any non-loopback URL anywhere under `app/src/`
 * — including one that is only ever data, and including one that appears only
 * in a comment, since that rule scans raw text rather than stripping prose.
 * Both halves of that are right as they stand: a remote host named in the
 * frontend is a fetch waiting to happen, the rule fails *closed*, and
 * loosening a working lint so a fixture could spell a prettier hostname is
 * exactly how a lint stops catching things. Nothing serves this address, which
 * is the honest state of affairs: *Open in browser* under `?fake-ipc` proves
 * the button and its scheme guard, not the page at the other end.
 */
const FIXTURE_HOST = "http://127.0.0.1:5399";

/** When the fixture claims it last synced. Fixed, so screenshots are stable. */
export const SYNCED_AT = "2026-08-22T14:30:00Z";

/** Code's repositories (#837): name, last push, and prototype-6's team and language (#966). */
const CODE_REPOS: [name: string, updated: string, team: string, language: string][] = [
  ["psp-client", "2026-08-21T13:20:00Z", "Payments", "Rust"],
  ["refund-worker", "2026-08-22T07:05:00Z", "Payments", "Rust"],
  ["fx-rates", "2026-08-19T09:40:00Z", "Payments", "Kotlin"],
  ["payout-dashboard", "2026-08-20T15:10:00Z", "Payments", "TypeScript"],
  ["settlement-reports", "2026-08-14T10:00:00Z", "Payments", "Python"],
  ["rust-common", "2026-08-21T08:30:00Z", "Shared", "Rust"],
  ["api-contracts", "2026-08-18T16:45:00Z", "Shared", "Protobuf"],
  ["booking-api", "2026-08-22T12:15:00Z", "Booking", "Kotlin"],
  ["tracking-events", "2026-08-12T11:00:00Z", "Tracking", "Go"],
  ["auth-gateway", "2026-08-17T14:20:00Z", "Platform", "Go"],
];

/** Code's pull requests (#837): repository, number, title, state, author, updated, reviewers asked. */
const CODE_PRS: [string, number, string, "open" | "merged" | "closed", string, string, string[]?][] = [
  ["ledger", 139, "Fix ledger drift on partial refunds", "merged", "mara", "2026-08-21T16:00:00Z"],
  ["psp-client", 57, "Treat TEMP_UNAVAILABLE as transient", "merged", "mara", "2026-08-20T15:10:00Z"],
  ["fx-rates", 88, "Retry on 429", "open", "priya", "2026-08-22T09:10:00Z"],
  ["booking-api", 121, "Paginate list endpoint", "open", "jonas", "2026-08-21T17:40:00Z"],
  ["tracking-events", 64, "Add tracing spans", "open", "tomasz", "2026-08-12T10:30:00Z"],
  ["auth-gateway", 33, "Make timeouts configurable", "open", "lena", "2026-08-22T13:05:00Z", ["mara"]],
  ["payout-dashboard", 73, "Cache lookup results", "open", "priya", "2026-08-20T09:55:00Z"],
  ["rust-common", 41, "Bump dependencies", "closed", "sven", "2026-08-18T12:00:00Z"],
  ["refund-worker", 19, "Batch inserts", "merged", "aylin", "2026-08-22T07:00:00Z"],
];

/**
 * A slice of the Tidewater dataset, plus one kind nothing declares.
 *
 * Enough to fill the mockup's tiles and no more: this exists so a browser can
 * be pointed at the shell, not so the fixture can stand in for PostgreSQL. The
 * `incident` row is the §3a case — an *undeclared* kind, which must get its
 * own tile and the generic detail view rather than being dropped.
 */
export const CORPUS: {
  entity_id: string;
  kind: string;
  title: string;
  updated_at: string | null;
  body_text: string;
  author: string | null;
  web_url: string | null;
  deleted_at: string | null;
  payload: Record<string, unknown>;
}[] = [
  row("mock:PAY-231", "ticket", "Retry failed SEPA payouts", "2026-08-22T11:48:00Z", "mara", {
    key: "PAY-231",
    // The shape a *source* writes a project in, not a shape of this file's
    // choosing: the mock adapter puts it at `fields.project` because Jira does
    // (#207), and `project_key_read!` reads it there. A flat `project` here
    // would make the fixture's rooms disagree with the real backend's.
    fields: { project: { key: "PAY", name: "Payments Platform" } },
    status: "In Progress",
    priority: "High",
    assignee: "mara",
    type: "Story",
    epic: "PAY-200",
    story_points: 3,
    estimate_h: 6,
    blocked: false,
    labels: ["sepa", "psp"],
    description:
      "Payouts to two SEPA banks fail with a 409 on retry. The retry window has to be idempotent before we can turn the scheduler back on.",
    comments: [{ by: "mara" }, { by: "jonas" }],
  }),
  // Deliberately in **no** project, key prefix notwithstanding: the miss
  // direction ADR-0010 fixes, and the one a QA pass has to be able to see. It
  // stays in *All work* and in the mock source's room, and appears in neither
  // project room.
  row("mock:PAY-228", "ticket", "Idempotency key on the payout endpoint", "2026-08-21T16:05:00Z", "jonas", {
    key: "PAY-228",
    status: "In Review",
    priority: "Normal",
    assignee: "jonas",
    type: "Bug",
  }),
  row("mock:OPS-77", "ticket", "Rotate the staging database credentials", "2026-08-19T09:00:00Z", "priya", {
    key: "OPS-77",
    fields: { project: { key: "OPS", name: "Operations" } },
    status: "To Do",
    priority: "Low",
    assignee: null,
    labels: ["on-call"],
  }),
  // The ticket filed from the standup protocol's first action item, titled by
  // the item's words as `create_action_item_ticket` titles it (#1433): the
  // fixture protocol links it, so the Standup tab reads it back on open.
  row("mock:OPS-131", "ticket", "Update the on-call runbook for the new ledger topology (Tomasz Wójcik)", "2026-08-21T10:05:00Z", "mara", {
    key: "OPS-131",
    fields: { project: { key: "OPS", name: "Operations" } },
    status: "To Do",
    priority: "Normal",
    assignee: null,
    type: "Task",
  }),
  // The Tickets view's board (#810) wants more than three tickets to look
  // like prototype-6's: these are its own, in its words, with the type, the
  // assignee and the epic its cards draw, flat like the rest of this corpus.
  ...(
    [
      ["PAY-240", "Payout dashboard latency", "To Do", "Medium", "Task", "mara", "PAY-200", "2026-08-22T08:12:00Z"],
      ["PAY-236", "Payout CSV export for finance", "To Do", "Low", "Story", "jonas", "PAY-200", "2026-08-20T10:00:00Z"],
      ["PAY-232", "Metrics for SEPA retry attempts", "To Do", "Medium", "Sub-task", "mara", "PAY-200", "2026-08-21T11:00:00Z"],
      ["PAY-233", "Document the manual-review SLA", "To Do", "Medium", "Sub-task", "mara", "PAY-200", "2026-08-22T09:20:00Z"],
      ["PAY-219", "Rotate PSP credentials", "Done", "Medium", "Task", "mara", "PAY-200", "2026-08-18T10:00:00Z"],
      ["PAY-244", "Refund webhooks retry", "In Progress", "High", "Story", "priya", "PAY-300", "2026-08-21T09:30:00Z"],
      ["PAY-251", "Validate payout batch limits", "In Review", "Medium", "Task", "jonas", "PAY-300", "2026-08-20T15:10:00Z"],
      ["PAY-205", "Fix FX rate cache", "Done", "Low", "Bug", "priya", "PAY-150", "2026-08-15T12:00:00Z"],
      ["PAY-210", "Export settlement report timezone", "Done", "Medium", "Story", "jonas", "PAY-150", "2026-08-14T12:00:00Z"],
      ["PAY-214", "Monitor PSP client rate limit", "Done", "High", "Task", "mara", "PAY-200", "2026-08-12T12:00:00Z"],
      ["PAY-216", "Clean up manual payout audit log", "Done", "Low", "Task", "priya", null, "2026-08-11T12:00:00Z"],
    ] as const
  ).map(([key, title, status, priority, type, assignee, epic, at]) =>
    row(`mock:${key}`, "ticket", title, at, assignee, {
      key,
      fields: { project: { key: "PAY", name: "Payments Platform" } },
      status,
      priority,
      type,
      assignee,
      ...(epic ? { epic } : {}),
      // prototype-6's one label outside PAY-231 and OPS-77 (#813).
      ...(key === "PAY-240" ? { labels: ["finance"] } : {}),
      description: `${title}. Finance asked for this; scope is the stage environment first.`,
    }),
  ),
  // The repo and one of its branches (#499). Both are here so a QA pass can
  // see the two checkout states the panel exists to keep apart: this repo has
  // a clone under the fixture's clones root, and `mock:ledger` deliberately
  // does not -- so one detail shows a path and the other shows the clone
  // command to copy. The keys follow interfaces §4.2's Gitea grammar
  // (`owner/repo`, `owner/repo@refs/heads/<name>`), which is what the real
  // backend resolves a branch's repository by.
  {
    ...row("mock:tidewater/payout-service", "repo", "tidewater/payout-service", "2026-08-22T10:30:00Z", "mara", {
      full_name: "tidewater/payout-service",
      description: "Payout scheduling and SEPA retries.",
      teams: ["Payments"],
      language: "Rust",
    }),
    // A forge-shaped URL rather than the fixture's default: it is what the
    // clone command is built from, and a person reading the panel should see
    // the command they would actually run.
    web_url: "https://gitea.example.com/tidewater/payout-service",
  },
  row(
    "mock:tidewater/payout-service@refs/heads/feat/PAY-231-idempotent-retry",
    "branch",
    "feat/PAY-231-idempotent-retry",
    "2026-08-22T09:40:00Z",
    "mara",
    // The parent path Gitea declares (#596), which its address reads to open the repository's Branches tab (#1440).
    { name: "feat/PAY-231-idempotent-retry", repository: "tidewater/payout-service", path: "tidewater/payout-service" },
  ),
  // A second repo, with no clone on this fixture's disk: the *no checkout*
  // arm, and the one that shows the clone command.
  {
    ...row("mock:tidewater/ledger", "repo", "tidewater/ledger", "2026-08-18T12:00:00Z", "jonas", {
      full_name: "tidewater/ledger",
      description: "The ledger service.",
      teams: ["Payments"],
      language: "Kotlin",
    }),
    web_url: "https://gitea.example.com/tidewater/ledger",
  },
  row("mock:payout-service#142", "pr", "Idempotent retry window", "2026-08-22T10:12:00Z", "mara", {
    num: 142,
    repo: "payout-service",
    state: "open",
    approvals: "1/2",
    // The state chip and path the real Gitea declarations give a pull
    // request (#817), so the job's Code section rolls up its open PR.
    chip: { text: "open", tone: "info" },
    path: "tidewater/payout-service",
  }),
  // A pull request that asks the fixture's user for a review (#815), so
  // PAY-236's card says *review asked of you* as prototype-6's does.
  row("mock:payout-service#150", "pr", "Payout CSV export", "2026-08-22T11:50:00Z", "jonas", {
    num: 150,
    repo: "payout-service",
    state: "open",
    requested_reviewers: ["mara"],
    // The chip and path Gitea declares (#837), so Code groups it.
    chip: { text: "open", tone: "info" },
    path: "tidewater/payout-service",
  }),
  // The commit and the build carry the ticket's key -- in the branch name and
  // in the message body -- because that is the evidence `FAKE_SUGGESTIONS`
  // below cites, and a proposal whose reason the corpus cannot bear out is a
  // fixture lying about the one thing the tray exists to show.
  row("mock:9f2c1ab", "commit", "payout: key the retry on the mandate id", "2026-08-22T09:40:00Z", "mara", {
    sha: "9f2c1ab",
    repo: "payout-service",
    branch: "feat/PAY-231-idempotent-retry",
    description: "A replayed payout with the same mandate id is now a no-op.\n\nRefs PAY-231.",
    // The parent path Gitea declares, which its address reads to open the commit under its repository (#1440).
    path: "tidewater/payout-service",
  }),
  row("mock:payout-service#318", "build", "payout-service #318", "2026-08-22T10:20:00Z", null, {
    cfg: "payout-service",
    num: 318,
    status: "failed",
    branch: "feat/PAY-231-idempotent-retry",
    // The build's parameters as a build source mirrors them into `body_text`
    // (`knobas_source_teamcity::map::build_item`): the branch, the status, the
    // trigger. That blob is what `build_parameter_key` reads.
    description: "branch feat/PAY-231-idempotent-retry, status failed, triggered by a push",
  }),
  // The page shares enough of PAY-231's vocabulary to clear `similar_text`'s
  // floor of five stems, so the `similarity` proposal below is one the real
  // detector would make of this pair rather than a badge painted on for QA.
  row("confluence:sepa", "page", "SEPA retry design", "2026-08-20T15:30:00Z", "priya", {
    source: "confluence",
    space: "ENG",
    id: "ENG-SEPA",
    section: "Payments",
    description:
      "How the payout scheduler retries failed SEPA payouts: the retry window, the 409 on a replayed payout, and the mandate id as the idempotency key.",
  }),
  // The §3a case: no adapter declares this kind, and it still has to work.
  row("mock:INC-1", "incident", "Payout queue backed up for 40 minutes", "2026-08-22T08:05:00Z", "priya", {
    id: "INC-1",
    severity: "SEV2",
    resolved: true,
    affected_services: ["payout-service", "ledger"],
  }),
  {
    ...row("mock:PAY-198", "ticket", "Legacy payout reconciliation (withdrawn)", null, null, {
      key: "PAY-198",
      status: "Deleted",
    }),
    web_url: null,
    deleted_at: "2026-08-22T12:00:00Z",
  },
  // What PAY-231's job draws beyond the rows above (#817): its builds,
  // monitors and pages. See `job-reach.ts`.
  ...JOB_ROWS,
  // The repository page (#838): the three commits the job's branch carries,
  // in the mirror, so their rows open their diffs (`repos.ts` draws them).
  ...(
    [
      ["c90d11e4a7b2", "PAY-231: jitter in backoff, cap at 5 attempts", "2026-08-22T09:42:00Z"],
      ["a41f2c08d3e9", "PAY-231 backoff jitter", "2026-08-22T08:02:00Z"],
      ["7be0e4f15c60", "PAY-231: retry SEPA payouts on transient PSP errors", "2026-08-21T15:30:00Z"],
    ] as const
  ).map(([sha, title, at]) =>
    row(`mock:tidewater/payout-service@${sha}`, "commit", title, at, "mara", { sha, path: "tidewater/payout-service" }),
  ),
  // Code (#837): enough repositories and pull requests beside the two above
  // to compare *Repositories* and *Pull requests* with prototype-6, every
  // pull request with the chip and parent path Gitea declares.
  ...CODE_REPOS.map(([name, updated, team, language]) => ({
    ...row(`mock:tidewater/${name}`, "repo", `tidewater/${name}`, updated, null, {
      full_name: `tidewater/${name}`,
      teams: [team],
      language,
    }),
    web_url: `https://gitea.example.com/tidewater/${name}`,
  })),
  ...CODE_PRS.map(([repo, num, title, state, author, updated, reviewers]) =>
    row(`mock:${repo}#${num}`, "pr", title, updated, author, {
      num,
      repo,
      state: state === "open" ? "open" : "closed",
      merged: state === "merged",
      ...(reviewers ? { requested_reviewers: reviewers } : {}),
      chip: { text: state, tone: state === "open" ? "info" : state === "merged" ? "ok" : "muted" } satisfies StateChip,
      path: `tidewater/${repo}`,
    }),
  ),
];

/**
 * The keys a second ticket source holds too (#1476): the Jira source mirrors
 * these as the mock does, as a demo profile does beside the testenv Jira. Two
 * tickets under one key, told apart by their source; Jira's synced earlier.
 */
export const SHARED_KEYS = ["PAY-231", "PAY-228", "OPS-77"] as const;
CORPUS.push(
  ...SHARED_KEYS.map((key) => {
    const twin = CORPUS.find((entry) => entry.entity_id === `mock:${key}`)!;
    return { ...twin, entity_id: `jira:${key}`, payload: { ...twin.payload, source: "jira" } };
  }),
);

/** One corpus entry, with the fields every row shares filled in. */
function row(
  entity_id: string,
  kind: string,
  title: string,
  updated_at: string | null,
  author: string | null,
  payload: Record<string, unknown>,
) {
  return {
    entity_id,
    kind,
    title,
    updated_at,
    author,
    body_text: String(payload["description"] ?? title),
    web_url: `${FIXTURE_HOST}/browse/${keyOf(entity_id)}`,
    deleted_at: null as string | null,
    payload,
  };
}

/** The stored contexts the fixture session holds. Starts with one unarchived
 * job context, PAY-231's (#1503: the *Open alerts on my jobs* smart list is
 * computed over the unarchived ones, and a fixture with none could not be
 * walked); *Promote* adds to it. */
export const FAKE_CONTEXTS: ContextRow[] = [];

/**
 * The estate assets a context holds, by the context's anchor (#1503). The
 * fixture has no link graph, so this one table is all the asset membership
 * there is; `smart-lists.ts` reads it for *Open alerts on my jobs*. PAY-231
 * holds the TeamCity asset, which is the one the fixture's acked alert watches.
 */
export const FAKE_CONTEXT_ASSETS: Record<string, string[]> = {
  "mock:PAY-231": ["asset:knobas-teamcity"],
};

let nextContext = 1;
export function fakeContext(kind: ContextKind, title: string, anchor_id: string | null): ContextRow {
  return {
    id: `ctx:fake-${nextContext++}`,
    kind,
    title,
    anchor_id,
    created_at: new Date().toISOString(),
    archived_at: null,
  };
}

FAKE_CONTEXTS.push(fakeContext("ticket", "Retry failed SEPA payouts", "mock:PAY-231"));

/**
 * The mirror row `list_entities` returns. The path and the state chip are
 * the fixture's own `path` and `chip` payload keys where a row carries them
 * (#817's job rows), and `null` otherwise, as the mock adapter declares
 * neither; a row from another source names it as `source`.
 */
export function mirrorRow(entry: (typeof CORPUS)[number]) {
  const text = (value: unknown) => (typeof value === "string" ? value : null);
  const chip = entry.payload["chip"] as StateChip | undefined;
  return {
    entity_id: entry.entity_id,
    kind: entry.kind,
    source_id: text(entry.payload["source"]) ?? "mock",
    title: entry.title,
    updated_at: entry.updated_at,
    synced_at: SYNCED_AT,
    path: text(entry.payload["path"]),
    state: chip ?? null,
  };
}

/**
 * `mini_board` (#177), over the fixture.
 *
 * The dev bridge's own answer, so the Tickets tile has a board to draw under
 * `just dev` and `just demo`. Like `listEntities` below, it *restates* the
 * command's rule — the leading four, then the rest folded and alphabetical,
 * then the statusless column last — and a restatement is a copy that can
 * drift. The command is the authority; a divergence here is a bug in this
 * file, never a second opinion about the order. It reads the fixture's
 * payloads the way the command's second `coalesce` arm reads the mock source's
 * — a flat `status` and `priority` — because that is the shape this corpus is
 * in.
 */
function miniBoard(args: Record<string, unknown>) {
  // Asked for a role, the board is its fillers' (#1299), as the command's:
  // a role nobody fills is an empty board, never every source's. The
  // command takes no source list, so none is read off `args`.
  const role = args["role"];
  const sources = typeof role === "string" ? fakeRoleScope(role).sources : [];
  if (typeof role === "string" && sources.length === 0) return { columns: [], sources: [], lanes: [], more_lanes: 0 };
  const perColumn = (args["perColumn"] as number | null | undefined) ?? null;
  const lanes = (args["lanes"] as LaneField | null | undefined) ?? null;
  // A Jira view's board (#877): the whole board under the same words, cut
  // to the view's tickets in Jira's order under its own columns.
  const savedView = args["savedView"];
  const whole =
    typeof savedView === "string"
      ? fakeJiraBoard(savedView, boardOf({ ...args, sources: [], ctxId: null, project: null, perColumn: null }), null)
      : boardOf({ ...args, sources, perColumn: null });
  // In lanes (#939): each lane's column capped, the first lanes sent.
  if (lanes !== null) {
    return fakeLanes(whole, lanes, perColumn, (args["lanesFirst"] as number | null | undefined) ?? null);
  }
  return {
    columns: whole.columns.map((column) => ({
      ...column,
      cards: perColumn === null ? column.cards : column.cards.slice(0, perColumn),
    })),
    sources: whole.sources,
    lanes: [],
    more_lanes: 0,
  };
}

function boardOf(args: Record<string, unknown>) {
  // The fixture has no link graph, so a stored context's membership is
  // honestly empty — the same answer `list_entities` gives.
  if (args["ctxId"]) return { columns: [], sources: [] };

  const sources = (args["sources"] as string[] | undefined) ?? [];
  const wantsMock = !sources.length || sources.includes("mock");
  const wantsGitea = !sources.length || sources.includes("gitea");
  if (!wantsMock && !wantsGitea) return { columns: [], sources: [] };

  const project = args["project"] as string | null | undefined;
  const query = typeof args["query"] === "string" ? args["query"].trim() : "";
  const mockTickets = wantsMock
    ? CORPUS.filter(
        (entry) =>
          entry.kind === "ticket" &&
          entry.deleted_at === null &&
          inProject(entry, project) &&
          (query === "" || fakeQueryMatches(entry, query)),
      )
    : [];
  // The Gitea source's tickets (#1248) sit beside the corpus, not in it: they
  // are the board's rows only, and a project or a query (Jira's words) names
  // none of them.
  const giteaTickets = wantsGitea && !project && query === "" ? GITEA_TICKETS : [];
  const tickets = [...mockTickets, ...giteaTickets].sort((a, b) => (b.updated_at ?? "").localeCompare(a.updated_at ?? ""));

  const columns: { status: string | null; cards: ReturnType<typeof boardCard>[] }[] = [];
  for (const entry of tickets) {
    const status = ticketStatus(entry);
    const column = columns.find((candidate) => candidate.status === status);
    if (column) column.cards.push(boardCard(entry));
    else columns.push({ status, cards: [boardCard(entry)] });
  }

  const LEADING = ["To Do", "In Progress", "In Review", "Done"];
  const rank = (status: string | null): [number, number, string] => {
    if (status === null) return [2, 0, ""];
    const at = LEADING.findIndex((leading) => leading.toLowerCase() === status.toLowerCase());
    return at === -1 ? [1, 0, status.toLowerCase()] : [0, at, status.toLowerCase()];
  };
  columns.sort((a, b) => {
    const [aClass, aAt, aFolded] = rank(a.status);
    const [bClass, bAt, bFolded] = rank(b.status);
    return aClass - bClass || aAt - bAt || aFolded.localeCompare(bFolded);
  });

  const observed = [...new Set(tickets.map((entry) => ticketStatus(entry)))]
    .filter((status): status is string => status !== null)
    .sort((a, b) => {
      const [aClass, aAt, aFolded] = rank(a);
      const [bClass, bAt, bFolded] = rank(b);
      return aClass - bClass || aAt - bAt || aFolded.localeCompare(bFolded);
    });

  // The command's cap (#753): each column's newest `per_column`, and its total.
  const perColumn = args["perColumn"] as number | null | undefined;
  return {
    columns: columns.map((column) => ({
      status: column.status,
      cards: perColumn == null ? column.cards : column.cards.slice(0, perColumn),
      total: column.cards.length,
    })),
    sources: columns.length
      ? [
          ...(mockTickets.length ? [{ source_id: "mock", statuses: observed.filter((status) => mockTickets.some((entry) => ticketStatus(entry) === status)) }] : []),
          ...(giteaTickets.length ? [{ source_id: "gitea", statuses: observed.filter((status) => giteaTickets.some((entry) => ticketStatus(entry) === status)) }] : []),
        ]
      : [],
  };
}

/**
 * The project a fixture entry carries, at the path the real read looks at.
 *
 * A restatement of `project_key_read!`'s Jira arm and of its failure
 * direction: absence, never a guess. An entry with no readable key belongs to
 * no project room and stays in every other room it was in.
 */
function projectOf(entry: (typeof CORPUS)[number]): { key: string; name: string | null } | null {
  const fields = entry.payload["fields"];
  const project = (fields as Record<string, unknown> | undefined)?.["project"] as
    | Record<string, unknown>
    | undefined;
  const text = (value: unknown) =>
    typeof value === "string" && value.trim() !== "" ? value.trim() : null;
  const key = text(project?.["key"]);
  return key === null ? null : { key, name: text(project?.["name"]) };
}

/** Whether an entry is admitted by a room's project scope. */
function inProject(entry: (typeof CORPUS)[number], project: string | null | undefined): boolean {
  return !project || projectOf(entry)?.key === project;
}

/** `list_projects` (#209): the projects the fixture's live corpus shows. */
function listProjects() {
  const seen = new Map<string, { source_id: string; key: string; name: string | null }>();
  for (const entry of CORPUS) {
    if (entry.deleted_at !== null) continue;
    const project = projectOf(entry);
    if (project === null || seen.has(project.key)) continue;
    seen.set(project.key, { source_id: "mock", key: project.key, name: project.name });
  }
  return [...seen.values()].sort((a, b) => a.key.localeCompare(b.key));
}

/** One card of the fixture's board. */
function boardCard(entry: (typeof CORPUS)[number]) {
  const text = (value: unknown) =>
    typeof value === "string" && value.trim() !== "" ? value.trim() : null;
  if (entry.entity_id.startsWith("gitea:")) return giteaCard(entry);
  const assignee = text(entry.payload["assignee"]);
  return {
    entity_id: entry.entity_id,
    source_id: text(entry.payload["source"]) ?? "mock",
    key: keyOf(entry.entity_id),
    title: entry.title,
    // Its own status (#1423), whatever column a Jira view gathers it under.
    status: ticketStatus(entry),
    priority: text(entry.payload["priority"]),
    // The fields the Tickets view's cards draw (#810), read flat because
    // that is the shape this corpus is in.
    issue_type: text(entry.payload["type"]),
    assignee,
    updated_at: entry.updated_at,
    path: text(entry.payload["epic"]),
    parent_title: FAKE_EPIC_TITLES[text(entry.payload["epic"]) ?? ""] ?? null,
    mine: assignee !== null && assignee.toLowerCase() === FAKE_IDENTITY,
    labels: Array.isArray(entry.payload["labels"]) ? (entry.payload["labels"] as string[]) : [],
    // The count a source declares (#894); this corpus keeps its comments
    // flat, so the count is the list's length, and a ticket with no list has
    // none declared (a miss, `null`).
    comment_count: Array.isArray(entry.payload["comments"]) ? entry.payload["comments"].length : null,
    signals: fakeSignals(entry.entity_id),
  };
}

/**
 * The Gitea source's tickets (#1248), in the shape the real adapter mirrors an
 * issue: key `owner/repo/issues/<n>`, `state`, `knobas.priority`,
 * `assignee.login`, `labels[].name` and `repository.full_name`.
 */
const GITEA_TICKETS: (typeof CORPUS)[number][] = (
  [
    ["tidewater/payout-service", 143, "Retry window ignores the clock skew", "open", "high", "mara", ["bug"], "2026-08-22T10:40:00Z"],
    ["tidewater/payout-service", 138, "Document the payout webhook payload", "open", "low", null, ["docs"], "2026-08-20T13:00:00Z"],
    ["tidewater/ledger", 57, "Ledger export drops the last row", "open", "medium", "jonas", ["bug", "export"], "2026-08-21T08:30:00Z"],
    ["tidewater/ledger", 52, "Pin the Kotlin toolchain", "closed", "low", "priya", [], "2026-08-17T15:00:00Z"],
  ] as const
).map(([repo, n, title, state, priority, login, labels, at]) => {
  const key = `${repo}/issues/${n}`;
  return {
    ...row(`gitea:${key}`, "ticket", title, at, login, {
      source: "gitea",
      key,
      state,
      knobas: { priority },
      assignee: login ? { login } : null,
      labels: labels.map((name) => ({ name })),
      repository: { full_name: repo },
      description: title,
    }),
    web_url: `https://git.tidewater.example/${repo}/issues/${n}`,
  };
});

/** A ticket's column: a Gitea issue's `state`, the corpus's flat `status` otherwise. */
function ticketStatus(entry: (typeof CORPUS)[number]): string | null {
  const value = entry.entity_id.startsWith("gitea:") ? entry.payload["state"] : entry.payload["status"];
  return typeof value === "string" && value.trim() !== "" ? value.trim() : null;
}

/** A Gitea ticket's board card, read at the paths the real adapter writes. */
function giteaCard(entry: (typeof CORPUS)[number]) {
  const payload = entry.payload as {
    knobas: { priority: string };
    assignee: { login: string } | null;
    labels: { name: string }[];
  };
  const assignee = payload.assignee?.login ?? null;
  return {
    entity_id: entry.entity_id,
    source_id: "gitea",
    key: keyOf(entry.entity_id),
    title: entry.title,
    status: ticketStatus(entry),
    priority: payload.knobas.priority,
    issue_type: null,
    assignee,
    updated_at: entry.updated_at,
    path: null,
    parent_title: null,
    mine: assignee !== null && assignee.toLowerCase() === FAKE_IDENTITY,
    labels: payload.labels.map((label) => label.name),
    comment_count: null,
    signals: fakeSignals(entry.entity_id),
  };
}

/**
 * The epics' names (#894), prototype-6's `EPICS`. The real board reads the
 * mirrored epic item's title; this corpus holds no epic items (they would
 * stand on the board as cards of their own), so it keeps their names here.
 * An epic not named here draws its key alone, the miss direction.
 */
const FAKE_EPIC_TITLES: Record<string, string> = {
  "PAY-200": "Payout reliability",
  "PAY-300": "PSP migration",
  "PAY-150": "Finance reporting",
};

/**
 * The fixture's confirmed links for the signals (#815): PAY-231's pull
 * request and red build, OPS-77 blocking PAY-228, and PAY-236's pull
 * request. `[from, to, relation]`, ids from `CORPUS`.
 */
const FAKE_SIGNAL_LINKS: [string, string, string][] = [
  ["mock:PAY-231", "mock:payout-service#142", "related"],
  ["mock:payout-service#142", "mock:payout-service#318", "related"],
  ["mock:OPS-77", "mock:PAY-228", "blocks"],
  ["mock:PAY-236", "mock:payout-service#150", "related"],
];

/**
 * A ticket's signals, restating `knobas_core::signals` over
 * {@link FAKE_SIGNAL_LINKS} and the fixture's flat payloads: a red build in
 * reach (links and one hop), each `blocks` link ending at the ticket, each
 * open pull request it links, and a review asked of the user on one. A
 * restatement certifies nothing; `signals_ipc.rs` is the witness.
 */
function fakeSignals(id: string) {
  const near = (of: string) =>
    FAKE_SIGNAL_LINKS.flatMap(([from, to]) => (from === of ? [to] : to === of ? [from] : []));
  const entry = (of: string) => CORPUS.find((candidate) => candidate.entity_id === of);
  const direct = near(id);
  const reach = [...new Set([...direct, ...direct.flatMap(near)])].filter((other) => other !== id);
  const prs = direct.map(entry).filter((pr) => pr?.kind === "pr" && pr.payload["state"] === "open");
  const signals: Signal[] = [];
  if (reach.some((other) => entry(other)?.kind === "build" && entry(other)?.payload["status"] === "failed")) {
    signals.push({ kind: "failing-build", text: "build failed", tone: "err" });
  }
  for (const [from, to, relation] of FAKE_SIGNAL_LINKS) {
    if (relation === "blocks" && to === id) {
      signals.push({ kind: "blocker", text: `blocked by ${keyOf(from)}`, tone: "err" });
    }
  }
  for (const pr of prs) signals.push({ kind: "open-pr", text: `PR #${String(pr!.payload["num"])} open`, tone: "info" });
  const asked = (pr: (typeof prs)[number]) =>
    ((pr?.payload["requested_reviewers"] as string[] | undefined) ?? []).includes(FAKE_IDENTITY);
  if (prs.some(asked)) signals.push({ kind: "review-for-me", text: "review asked of you", tone: "warn" });
  return signals;
}

/** Who `@me` is in the fixture: the username the cards' `mine` matches. */
export const FAKE_IDENTITY = "mara";

/**
 * `mini_board`'s `query` (#810), restated over the fixture: the words must
 * appear in the key, the title, the assignee or the epic, ignoring case, as
 * `knobas_search::query::word_alternatives` reads them (`or`, `-word`, a
 * quoted phrase), and each ticket key the
 * Tickets view's chips and dropdowns write (`issuetype:`, `priority:`,
 * `assignee:`, `status:`, `project:`, `under:`), and `label:` and `has:`
 * (#813, #815), must hold, values of one key
 * meaning any of them. A restatement, which certifies nothing: the real
 * match is witnessed in `knobas-search`'s `ticket_keys.rs`.
 */
function fakeQueryMatches(entry: (typeof CORPUS)[number], query: string): boolean {
  const tokens = [...query.matchAll(/(-?)([a-z]+):(?:"([^"]*)"|(\S+))|(\S+)/gi)];
  const keys = new Map<string, { negated: boolean; values: string[] }>();
  for (const [, minus, key, quoted, bare, word] of tokens) {
    if (word !== undefined) continue;
    const name = `${minus}${key!.toLowerCase()}`;
    const held = keys.get(name) ?? { negated: minus === "-", values: [] };
    held.values.push((quoted ?? bare ?? "").toLowerCase());
    keys.set(name, held);
  }
  const card = boardCard(entry);
  const haystack = `${card.key} ${card.title} ${card.assignee ?? ""} ${card.path ?? ""}`.toLowerCase();
  // The words as the grammar reads them (#1889): `or` separates alternatives,
  // `-word` and `-"phrase"` are left out of the one they stand in, a phrase is one needle.
  const alternatives = wordAlternatives(query);
  const admits = ({ holds, lacks }: { holds: string[]; lacks: string[] }) =>
    holds.every((word) => haystack.includes(word)) && !lacks.some((word) => haystack.includes(word));
  if (alternatives.length > 0 && !alternatives.some(admits)) return false;
  const field: Record<string, string | null> = {
    issuetype: card.issue_type,
    priority: card.priority,
    assignee: card.assignee,
    status: typeof entry.payload["status"] === "string" ? (entry.payload["status"] as string) : null,
    project: projectOf(entry)?.key ?? null,
    under: card.path,
  };
  for (const [name, { negated, values }] of keys) {
    const key = name.replace(/^-/, "");
    // `has:` (#815): the card's own signals, as the real grammar reads them.
    if (key === "has") {
      const hit = card.signals.some((signal) => values.includes(signal.kind));
      if (hit === negated) return false;
      continue;
    }
    if (key === "label") {
      const hit = card.labels.some((label) => values.includes(label.toLowerCase()));
      if (hit === negated) return false;
      continue;
    }
    if (!(key in field)) continue;
    const have = field[key]?.toLowerCase() ?? null;
    const wanted = values.map((value) => (value === "@me" ? FAKE_IDENTITY : value));
    const hit = have !== null && wanted.includes(have);
    if (hit === negated) return false;
  }
  return true;
}

/**
 * How many live tickets `query` admits, by {@link fakeQueryMatches}: a ticket
 * view's count in the views pane (#812), the number its board draws.
 */
export function fakeTicketCount(query: string): number {
  return CORPUS.filter(
    (entry) => entry.kind === "ticket" && entry.deleted_at === null && (query.trim() === "" || fakeQueryMatches(entry, query)),
  ).length;
}

/** Every live ticket's key: what a fake Jira view's members are picked from (#877). */
export function fakeTicketKeys(): string[] {
  return CORPUS.filter((entry) => entry.kind === "ticket" && entry.deleted_at === null).map(
    (entry) => keyOf(entry.entity_id),
  );
}

/**
 * A fake source as {@link fakeRoleScope} reads it: its adapter's claims, the
 * misses its sync recorded, the kinds it emits and the frames its adapter
 * declares them on.
 */
export interface FakeClaimer {
  id: string;
  roles: { role: string }[];
  role_misses: { role: string }[];
  kinds: { id: string }[];
  payload_paths: { kind: string; frame?: string | null }[];
}

/**
 * The fake sources, handed in by `sources.ts`. It reads `CORPUS` as it
 * loads, so this module cannot import it back.
 */
export const ROLE_CLAIMERS: (() => FakeClaimer[])[] = [];

/**
 * A built-in role's minimum kinds, required and optional, as
 * `knobas_source::role::MINIMUMS` names them: by id, and by the frame a source
 * declares a kind on. A copy, because this bridge stands in for the server and
 * cannot read its table; a changed minimum is a §10.8 entry (ADR-0029), which
 * is where this copy follows it.
 */
const ROLE_MINIMUMS: Record<string, { ids: string[]; frames: string[] }> = {
  "code-host": { ids: ["repo", "branch", "pr", "commit"], frames: [] },
  "ticket-system": { ids: ["ticket"], frames: ["saved_view"] },
  ci: { ids: [], frames: ["pipeline", "run"] },
  wiki: { ids: ["page"], frames: [] },
  monitoring: { ids: ["monitor"], frames: [] },
};

/** The kinds of a built-in role's minimum that `source` emits or may emit. */
function minimumKinds(minimum: { ids: string[]; frames: string[] }, source: FakeClaimer): string[] {
  return [
    ...minimum.ids,
    ...source.payload_paths.filter((paths) => minimum.frames.includes(paths.frame ?? "")).map((paths) => paths.kind),
  ];
}

/**
 * What a role scopes a faked read to (#1294), as `sources::crud::role_scope`
 * answers it: the sources that claim the role with no miss recorded for it,
 * and the role's kinds among them. A custom role's kinds are its sources'
 * kinds that are no built-in role's they claim. An unknown role has none.
 */
export function fakeRoleScope(role: string): { sources: string[]; kinds: string[] } {
  const fillers = ROLE_CLAIMERS.flatMap((claimers) => claimers()).filter(
    (source) =>
      source.roles.some((claim) => claim.role === role) && !source.role_misses.some((miss) => miss.role === role),
  );
  const kinds = fillers.flatMap((source) => {
    const minimum = ROLE_MINIMUMS[role];
    if (minimum) {
      // A pipeline is the `pipelines` read's, not the entity list's (#1344).
      const pipelines = source.payload_paths.filter((paths) => paths.frame === "pipeline").map((paths) => paths.kind);
      return minimumKinds(minimum, source).filter((kind) => !pipelines.includes(kind));
    }
    const builtin = new Set(
      source.roles.flatMap((claim) => {
        const other = ROLE_MINIMUMS[claim.role];
        return other ? minimumKinds(other, source) : [];
      }),
    );
    return source.kinds.map((kind) => kind.id).filter((kind) => !builtin.has(kind));
  });
  return { sources: fillers.map((source) => source.id), kinds: [...new Set(kinds)].sort() };
}

/** `list_entities`, filtered, ordered and paged the way the real one is. */
function listEntities(args: Record<string, unknown>) {
  const filter = (args["filter"] ?? {}) as {
    sources?: string[];
    kinds?: string[];
    order?: string;
    context?: string | null;
    project?: string | null;
    include_deleted?: boolean;
    reach?: string | null;
    role?: string | null;
  };
  const limit = Number(args["limit"] ?? 50);
  const offset = Number(args["offset"] ?? 0);

  // The tile orders (#680) count what they drew; the plain orders do not.
  // Spelled out rather than `tile-count.ts`'s `isTileOrder`: this bridge
  // stands in for the IPC that `ipc/entity.ts` invokes, and imports only its
  // types.
  const tileOrder = filter.order === "attention" || filter.order === "latest_per_parent";
  // A context's membership is `FAKE_MEMBERS`' (#816): the one small link
  // graph this fixture has, so a job's Code section has rows to draw.
  const members = filter.context ? fakeMembers(filter.context) : null;
  let rows = CORPUS.filter((entry) => filter.include_deleted || entry.deleted_at === null);
  if (members !== null) rows = rows.filter((entry) => members.includes(entry.entity_id));
  // What a ticket reaches (#817): `job-reach.ts`'s table.
  if (filter.reach) {
    const reached = new Set(fakeReach(filter.reach).map((entry) => entry.entity_id));
    rows = rows.filter((entry) => reached.has(entry.entity_id));
  }
  if (filter.sources?.length) rows = rows.filter((entry) => filter.sources?.includes(mirrorRow(entry).source_id));
  if (filter.kinds?.length) rows = rows.filter((entry) => filter.kinds?.includes(entry.kind));
  // A role (#1294): its fillers' rows, of its kinds.
  if (filter.role) {
    const scope = fakeRoleScope(filter.role);
    rows = rows.filter((entry) => scope.sources.includes(mirrorRow(entry).source_id) && scope.kinds.includes(entry.kind));
  }
  rows = rows.filter((entry) => inProject(entry, filter.project));

  rows = [...rows].sort((a, b) =>
    filter.order === "title_asc"
      ? a.title.localeCompare(b.title)
      : (b.updated_at ?? "").localeCompare(a.updated_at ?? ""),
  );

  // The fixture is the mock corpus, whose adapter declares no state: under
  // the tile orders that is §3a's recency fallback, so the rows above are
  // already the answer and the tally is a count per kind with no tone.
  // Code's pull requests (#837) carry a chip, so for a read of pull requests
  // alone `attention` does what the real one does (#712): it drops the
  // settled ones and groups the rest for the fixture's identity, `mara`.
  const groups = filter.order === "attention" && filter.kinds?.length === 1 && filter.kinds[0] === "pr" ? fakeGroups(rows) : null;
  if (groups !== null) rows = groups.rows;
  const mirrored = rows.map(mirrorRow);
  const tally = tileOrder
    ? [...new Set(mirrored.map((row) => row.kind))].map((kind) => ({
        kind,
        tone: null,
        count: mirrored.filter((row) => row.kind === kind).length,
      }))
    : null;
  return { rows: mirrored.slice(offset, offset + limit), total: rows.length, tally, groups: groups?.tally ?? null };
}

/** The attention groups of pull requests (#712), for the fixture's identity: rows group by group, newest first in each. */
function fakeGroups(rows: (typeof CORPUS)[number][]) {
  const ME = "mara";
  const settled = (entry: (typeof CORPUS)[number]) => ["ok", "muted"].includes(mirrorRow(entry).state?.tone ?? "");
  const groupOf = (entry: (typeof CORPUS)[number]) => {
    const asked = entry.payload["requested_reviewers"];
    if (Array.isArray(asked) && asked.includes(ME)) return "review_requested";
    return entry.author === ME ? "my_pull_requests" : "open_pull_requests";
  };
  const order: AttentionGroup[] = ["review_requested", "my_pull_requests", "open_pull_requests"];
  const open = rows.filter((entry) => !settled(entry));
  const grouped = order.map((group) => open.filter((entry) => groupOf(entry) === group));
  return {
    rows: grouped.flat(),
    tally: order.map((group, at) => ({ group, count: grouped[at]!.length })).filter((tally) => tally.count > 0),
  };
}

/**
 * Who is in a stored context (#816): the anchor, and what `FAKE_MEMBERS`
 * links to it. Every other context, and an unknown id, holds nothing.
 */
export function fakeMembers(ctxId: string): string[] {
  const anchor = FAKE_CONTEXTS.find((row) => row.id === ctxId)?.anchor_id ?? null;
  if (anchor === null) return [];
  return [anchor, ...(FAKE_MEMBERS[anchor] ?? [])];
}

/**
 * The fixture's one link graph (#816): what PAY-231 links to, so the job's
 * Code section draws its repository, branch, pull request and commit. Ids
 * from `CORPUS`; a promoted PAY-231 context holds exactly these.
 */
const FAKE_MEMBERS: Record<string, string[]> = {
  "mock:PAY-231": [
    "mock:tidewater/payout-service",
    "mock:tidewater/payout-service@refs/heads/feat/PAY-231-idempotent-retry",
    "mock:payout-service#142",
    "mock:9f2c1ab",
  ],
};

/**
 * `code_facts` (#966) over the fixture: the row's author and whether it is
 * the fixture's identity, and a repository's `language` and `teams`, which
 * ride in its payload under those names as the real declarations read them.
 */
function codeFacts(args: Record<string, unknown>): CodeFacts[] {
  const ids = new Set((args["entityIds"] as string[] | undefined) ?? []);
  const text = (value: unknown) => (typeof value === "string" && value.trim() !== "" ? value : null);
  const extra = CODE_FACTS.flatMap((answer) => answer(ids));
  const answered = new Set(extra.map((facts) => facts.entity_id));
  const own = CORPUS.filter(
    (entry) => ids.has(entry.entity_id) && !answered.has(entry.entity_id) && entry.deleted_at === null,
  ).map((entry) => ({
    entity_id: entry.entity_id,
    author: entry.author,
    mine: entry.author !== null && entry.author === FAKE_IDENTITY,
    language: text(entry.payload["language"]),
    teams: ((entry.payload["teams"] as unknown[] | undefined) ?? []).filter(
      (team): team is string => text(team) !== null,
    ),
  }));
  return [...own, ...extra];
}

/**
 * What another feature adds to `code_facts` (#966): rows the mirror would
 * hold that only that feature's fixture draws (the repository view's
 * branches, `views/repos.ts`). Each is asked for the ids wanted.
 */
export const CODE_FACTS: ((ids: ReadonlySet<string>) => CodeFacts[])[] = [];

/** This feature's commands. */
export function handlers(): Handlers {
  return {
    code_facts: (args) => codeFacts(args),
    list_entities: (args) => listEntities(args),
    mini_board: (args) => miniBoard(args),
    list_projects: () => listProjects(),
  };
}
