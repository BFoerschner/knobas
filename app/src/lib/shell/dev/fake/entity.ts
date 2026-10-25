/**
 * `?fake-ipc`: One entity: the detail and the job page's reads, the reachable
 * transitions, the conversation, and the URL resolver.
 */
import type { LinkEntry, LinkRow, SourceView, Thread, ViewAction, ViewSection, DiffLine } from "../../../ipc/entity";
import type { EntityRefresh } from "../../../ipc/sources";
import type { FakeBridge, Handlers } from "../fake-tauri";
import { keepSeed, onFakeReset } from "../fake-seed";
import { CORPUS, mirrorRow } from "./corpus";
import { FIXTURE_ESTATE } from "./estate";
import { FIXTURE_SOURCES } from "./sources";
import { fixtureBuildEntity, runView } from "./builds";
import { pullView } from "./views/pulls";
import { repoView } from "./views/repos";

/** A handful of log lines, so the History panel has something to draw. */
const FAKE_ACTIVITY = [
  {
    id: 3,
    at: "2026-08-22T14:30:00Z",
    actor: "sync:mock",
    verb: "synced",
    entity_id: "mock:PAY-231",
    detail: { upserted: 9 },
  },
  {
    id: 2,
    at: "2026-08-22T11:49:00Z",
    actor: "user",
    verb: "opened",
    entity_id: "mock:PAY-231",
    detail: {},
  },
  {
    id: 1,
    at: "2026-08-22T08:10:00Z",
    actor: "sync:mock",
    verb: "synced",
    entity_id: "mock:INC-1",
    detail: { upserted: 1 },
  },
];

/**
 * `resolve_url` (#496): what a pasted link names, over this fixture's corpus.
 *
 * A **second** implementation of the normalisation rule, and deliberately so:
 * the rule itself is SQL (`knobas_core::web_url_normalized!`) and there is no
 * database here. What this owes is not fidelity to the SQL but the two
 * outcomes a QA walk has to be able to see — a link that opens its entity, and
 * one that offers the browser — so it does the three things the rule does to
 * the URLs this corpus actually holds and no more.
 */
function resolveUrl(args: Record<string, unknown>) {
  const url = String(args["url"] ?? "");
  if (!/^https?:\/\//i.test(url)) {
    throw { code: "invalid", message: `${url} is not an http or https URL`, source_id: null };
  }
  const key = (raw: string) => {
    const parsed = new URL(raw);
    parsed.hash = "";
    parsed.pathname = parsed.pathname.replace(/\/+$/, "");
    return parsed.href.replace(/\/$/, "");
  };
  const wanted = key(url);
  const found =
    CORPUS.find((entry) => entry.web_url !== null && key(entry.web_url) === wanted) ??
    CORPUS.find((entry) => byForm(entry, wanted));
  return found ? { entity_id: found.entity_id, kind: found.kind } : null;
}

/** Where the fixture's Confluence serves its pages: `docs.ts`'s host. */
const CONFLUENCE = "http://127.0.0.1:5399/confluence";

/**
 * The forms the Confluence descriptor declares (#1475), for this corpus's
 * pages: `viewpage.action?pageId=<key>` and `/display/<space>/<title>`. Not
 * the declaration itself, which is Rust and read by a SQL statement; what a QA
 * walk needs is a page that opens from an address it is not stored under.
 */
function byForm(entry: (typeof CORPUS)[number], wanted: string): boolean {
  if (entry.kind !== "page" || !wanted.startsWith(`${CONFLUENCE}/`)) return false;
  const rest = wanted.slice(CONFLUENCE.length);
  const id = /^\/pages\/viewpage\.action\?pageId=([^&]+)/.exec(rest)?.[1];
  if (id !== undefined) return entry.entity_id === `confluence:${id}`;
  const display = /^\/display\/([^/?]+)\/([^/?]+)$/.exec(rest);
  if (!display) return false;
  try {
    const decoded = (text: string) => decodeURIComponent(text.replace(/\+/g, " "));
    return entry.payload["space"] === decoded(display[1]!) && entry.title === decoded(display[2]!);
  } catch {
    return false; // a malformed escape is a miss, as the Rust reading's is
  }
}

/**
 * The link graph, in memory (#835, #867): prototype-6's `s1` and `s3` already
 * confirmed, over the entities this corpus holds (PR #142 implements PAY-231;
 * the retry design page documents it), so the job card's connections have rows. A link
 * made by `create_link` is `manual` and one `unlink` removes goes from both
 * ends at once, as the real graph is undirected. An estate asset is a link end
 * too, as it is a row in `knobas.entity` (#1500: the launcher's *Link to <key>*
 * on a `list:` result). Reload and it is the seed again. `delete_note` is
 * `notes.ts`' (#1504).
 */
let nextLink = 1;
function linkRow(from_id: string, to_id: string, relation: string, origin: LinkRow["origin"], note: string | null): LinkRow {
  return {
    id: `link:fake-made-${nextLink++}`,
    from_id,
    to_id,
    relation,
    origin,
    note,
    created_by: origin === "manual" ? "mara" : "knobas",
    created_at: "2026-08-22T10:30:00Z",
    confirmed_at: "2026-08-22T10:30:00Z",
    rule: null,
    rule_class: null,
    reason: null,
  };
}
const LINKS: LinkRow[] = [
  linkRow("mock:payout-service#142", "mock:PAY-231", "implements", "source", null),
  linkRow("confluence:sepa", "mock:PAY-231", "documents", "manual", null),
  // PAY-228 is blocked by OPS-77 (#818), so its job draws the red banner.
  linkRow("mock:OPS-77", "mock:PAY-228", "blocks", "source", "Staging verification needs the rotated credentials. Priya expects Tuesday."),
];

/** A link end the fixture knows: a corpus entity, or an estate asset. */
function linkEnd(id: string): { kind: string; title: string; deleted_at: string | null } | null {
  const entity = CORPUS.find((candidate) => candidate.entity_id === id);
  if (entity) return entity;
  const asset = FIXTURE_ESTATE.find((candidate) => candidate.id === id);
  return asset ? { kind: "asset", title: asset.name, deleted_at: null } : null;
}

/** One link's far end from `id`, resolved against the corpus and the estate. */
function entryFor(id: string, link: LinkRow): LinkEntry | null {
  const otherId = link.from_id === id ? link.to_id : link.from_id;
  const other = linkEnd(otherId);
  if (!other) return null;
  return { link, other: { entity_id: otherId, kind: other.kind, title: other.title, deleted_at: other.deleted_at } };
}

/** The confirmed links on an entity, newest first, from either end. */
function linksOf(id: string): LinkEntry[] {
  return LINKS.filter((link) => link.from_id === id || link.to_id === id)
    .map((link) => entryFor(id, link))
    .filter((entry): entry is LinkEntry => entry !== null)
    .reverse();
}

/**
 * The source a row came from, as `get_entity` names it: the fixture's source
 * of that id (#1476, so Jira's PAY-231 says *Tidewater Jira*), else the mock.
 */
function sourceRef(id: string) {
  const source = FIXTURE_SOURCES.find((candidate) => candidate.id === id);
  return source
    ? { id: source.id, display_name: source.display_name, adapter_kind: source.adapter_kind, enabled: true }
    : { id: "mock", display_name: "Tidewater (mock)", adapter_kind: "mock", enabled: true };
}

/**
 * The adapters that read one item by id on main (#2095-#2101), so the
 * detail's `refreshes` matches what their descriptors declare; Kestrel, and
 * a WASM component built against a world before 1.4.0, offer *Sync now*.
 */
const REFRESHING_ADAPTERS = new Set(["mock", "jira", "confluence", "teamcity", "gitea", "kuma", "wasm-fixture"]);

/**
 * The kinds those adapters' refresh answers *not offered*, as their
 * descriptors declare them (`KindInfo::refreshes`, #2146): `get_entity`
 * answers `refreshes` for the row's kind, not the adapter.
 */
const NOT_REFRESHED: Readonly<Record<string, ReadonlySet<string>>> = {
  teamcity: new Set(["build_config"]),
  gitea: new Set(["repo", "branch", "commit"]),
  jira: new Set(["jira_view"]),
};

/** Whether `adapterKind`'s refresh reads an item of `kind`. */
function refreshesKind(adapterKind: string, kind: string): boolean {
  return REFRESHING_ADAPTERS.has(adapterKind) && !(NOT_REFRESHED[adapterKind]?.has(kind) ?? false);
}

let bridge: FakeBridge | null = null;

/** Hold the bridge, so a refresh can end on `item:refreshed` (#2107). */
export function attach(installed: FakeBridge): void {
  bridge = installed;
}
onFakeReset(() => {
  bridge = null;
});

/**
 * `refresh_entity` (#2107): a corpus row is read back as it stands, and the
 * refresh ends on `item:refreshed`, a roster monitor's too. `?fake-refresh-failed`
 * answers the failure line's reason instead, for a screenshot of it.
 */
function refreshEntity(args: Record<string, unknown>): EntityRefresh {
  const id = String(args["entityId"] ?? "");
  // The Monitors roster's monitors (`fake/monitors.ts`, #2138) are Kuma's,
  // which reads one, and are not in the corpus `get_entity` answers from.
  const rosterMonitor = id.startsWith("kuma:") && !CORPUS.some((entry) => entry.entity_id === id);
  if (!rosterMonitor) {
    const detail = getEntity(args);
    if (!detail.source.enabled) return { outcome: "turned_off" };
    if (!refreshesKind(detail.source.adapter_kind, detail.row.kind)) return { outcome: "not_offered" };
  }
  const failed = new URLSearchParams(location.search).has("fake-refresh-failed");
  setTimeout(() => bridge?.emit("item:refreshed", { entity_id: id, write_id: null, ended: failed ? "failed" : "refreshed" }), 0);
  return failed ? { outcome: "failed", reason: "502 Bad Gateway from the source" } : { outcome: "refreshed" };
}

/** A fixture build or build configuration the corpus does not hold, as a corpus row (#2137). */
function buildEntry(id: string): (typeof CORPUS)[number] | undefined {
  const found = fixtureBuildEntity(id);
  if (found === undefined) return undefined;
  return {
    entity_id: id,
    kind: found.kind,
    title: found.title,
    updated_at: null,
    body_text: "",
    author: null,
    web_url: null,
    deleted_at: null,
    payload: { source: found.source, path: found.path.split("/").join(" › ") },
  };
}

/** `get_entity`, including the two refusals the detail view branches on. */
function getEntity(args: Record<string, unknown>) {
  const id = String(args["entityId"] ?? "");
  if (!id.includes(":") || id.startsWith(":") || id.endsWith(":")) {
    throw { code: "invalid", message: `entity id ${id} has no ':' separating namespace from key`, source_id: null };
  }
  const entry = CORPUS.find((candidate) => candidate.entity_id === id) ?? buildEntry(id);
  if (!entry) {
    throw { code: "not_found", message: `${id} is not in the mirror`, source_id: null };
  }
  const row = mirrorRow(entry);
  const source = sourceRef(row.source_id);
  return {
    // A ticket's path is its parent chain, as Jira declares it: the flat
    // `epic` here, so *Save as a link set…* (#869) offers the epic.
    row: entry.kind === "ticket" && row.path === null ? { ...row, path: fakeText(entry.payload["epic"]) } : row,
    source,
    refreshes: refreshesKind(source.adapter_kind, entry.kind),
    // Null, where the real command now resolves the adapter's KindInfo from
    // the registry. Harmless divergence: the fallback path draws the same
    // words — knobas' own vocabulary (`kinds.ts` layer 2) matches the mock's
    // declared KindInfo for every kind in this corpus.
    kind_info: null,
    body_text: entry.body_text,
    author: entry.author,
    payload: entry.payload,
    web_url: entry.web_url,
    deleted_at: entry.deleted_at,
    links: linksOf(id),
    activity: FAKE_ACTIVITY.filter((line) => line.entity_id === id),
    // The mock corpus gives no build a commit, so nothing ran on any (#710).
    builds: [],
    // The job card's fields (#816), read from the fixture's flat mock shape:
    // the mock adapter declares `status` and `priority` there. Type and
    // estimate are this fixture's own (`type`, `estimate_h`), which the mock
    // adapter does not declare: a demo divergence, so the card has its whole
    // facts line under `?fake-ipc`.
    status: entry.kind === "ticket" && entry.deleted_at === null ? fakeText(entry.payload["status"]) : null,
    issue_type: fakeText(entry.payload["type"]),
    priority: fakeText(entry.payload["priority"]),
    estimate_seconds:
      typeof entry.payload["estimate_h"] === "number" ? Math.round(entry.payload["estimate_h"] * 3600) : null,
  };
}

/** A payload value that is non-blank text, trimmed, or `null` (ADR-0007's miss). */
function fakeText(value: unknown): string | null {
  return typeof value === "string" && value.trim() !== "" ? value.trim() : null;
}

/**
 * `reachable_transitions` (#816's status select). The mock adapter declares no
 * workflow, so the real command refuses it by name; the fixture answers with
 * prototype-6's workflow instead, so the select has somewhere to go.
 */
function fakeTransitions(args: Record<string, unknown>): string[] {
  const entry = CORPUS.find((candidate) => candidate.entity_id === String(args["entityId"] ?? ""));
  const status = fakeText(entry?.payload["status"]);
  const WORKFLOW: Record<string, string[]> = {
    "To Do": ["In Progress"],
    "In Progress": ["In Review", "To Do"],
    "In Review": ["Done", "In Progress"],
    Done: ["In Progress"],
  };
  return status === null ? [] : (WORKFLOW[status] ?? []);
}

/**
 * `entity_view` (#661): the demo commit draws an overview and a diff, so the
 * view's tabs and the diff viewer can be looked at in a browser, and one
 * source action (#1407) for the item page's toolbar; every other
 * entity draws nothing, which is the real mock adapter's answer for all of
 * them. A deliberate divergence, for the same reason `kind_info` above is one:
 * the shell under test is the same either way.
 */
function entityView(args: Record<string, unknown>): SourceView | null {
  const id = String(args["entityId"] ?? "");
  // A repository, its branches and its commits: the repository page (#838).
  const drawn = repoView(id, (args["section"] as string | null | undefined) ?? null);
  if (drawn !== undefined) return drawn;
  // A pull request: the pull request page (#841).
  const pull = pullView(id, (args["section"] as string | null | undefined) ?? null);
  if (pull !== undefined) return pull;
  // A run: the Run… dialog's *#N settings* (#852) and the run page (#853).
  const ran = runView(id, (args["section"] as string | null | undefined) ?? null);
  if (ran !== undefined) return ran;
  if (id !== "mock:9f2c1ab") return null;
  const section = (args["section"] as string | null | undefined) ?? null;
  const overview: ViewSection = {
    id: "overview",
    title: "Overview",
    count: null,
    deferred: false,
    blocks: [
      {
        type: "facts",
        facts: [
          { label: "Commit", value: "9f2c1ab0d4", tone: null },
          { label: "SHA", value: "9f2c1ab0d47e3c5a8b1f6e2d9c0a7b4e3f5d6c1a", tone: null },
          { label: "Author", value: "Mara Lindqvist <mara@tidewater.example>", tone: null },
          { label: "Authored", value: "2026-08-22 09:40 +02:00", tone: null },
        ],
      },
      {
        type: "entities",
        title: "Parent",
        rows: [{ entity_id: null, title: "41d0e7c2aa", meta: null, tone: null, chip: null, url: null }],
      },
      {
        type: "text",
        text: "payout: key the retry on the mandate id\n\nPAY-231. Two SEPA banks answer 409 on a retried payout\nbecause the idempotency key was the batch id.",
        markdown: false,
      },
    ],
  };
  const line = (kind: DiffLine["kind"], old: number | null, nu: number | null, text: string): DiffLine => ({ kind, old, new: nu, text });
  const diff: ViewSection = {
    id: "diff",
    title: "Diff",
    count: 2,
    deferred: false,
    blocks: [
      {
        type: "diff",
        truncated: false,
        commentable: null,
        gone: [],
        files: [
          {
            path: "src/payout/retry.rs",
            old_path: null,
            change: "modified",
            additions: 3,
            deletions: 1,
            truncated: false,
            threads: [],
            hunks: [
              {
                header: "@@ -41,7 +41,9 @@ impl Retry {",
                lines: [
                  line("context", 41, 41, "    fn key(&self, payout: &Payout) -> IdempotencyKey {"),
                  line("del", 42, null, "        IdempotencyKey::from(payout.batch_id)"),
                  line("add", null, 42, "        // One key per mandate: a retried batch is the same payout."),
                  line("add", null, 43, "        IdempotencyKey::from(payout.mandate_id)"),
                  line("context", 43, 44, "    }"),
                  line("context", 44, 45, ""),
                  line("add", null, 46, "    const MAX_ATTEMPTS: u32 = 5;"),
                ],
              },
            ],
          },
          {
            path: "docs/retry.md",
            old_path: "docs/backoff.md",
            change: "renamed",
            additions: 0,
            deletions: 0,
            truncated: false,
            hunks: [],
            threads: [],
          },
        ],
      },
    ],
  };
  const sections = section === null ? [overview, diff] : [overview, diff].filter((s) => s.id === section);
  return { sections, actions: [ADD_NOTE] };
}

/**
 * The demo commit's one source action (#1407): a write outside every role's
 * vocabulary, its id the fixed value, one field the source asks for, so the
 * item page's toolbar and its dialog can be looked at under `?fake-ipc`.
 */
const ADD_NOTE: ViewAction = {
  write_op: "source_action",
  label: "Add note…",
  fields: [{ name: "note", label: "Note", kind: { type: "long_text" }, value: null, required: true, placeholder: "What a reader of this commit should know" }],
  confirm: null,
  primary: false,
  danger: false,
  disabled: null,
  with_settings: false,
  values: { id: "add_note" },
  entity: null,
  line_comments: "none",
};

/**
 * The conversation (#819): prototype-6's PAY-231 thread, in its words, with
 * its *today* and *yesterday* read off the clock the page runs on, so the card
 * and the tab draw what the prototype draws on any day. Mentions are written
 * `@mara`, the account `@me` resolves to under this fixture, where the
 * prototype writes the display name. The prototype's pull-request events are
 * left out: knobas knows only the moves it sent (`entity_thread`), and one of
 * those stands in for *Mara moved this from To Do to In Progress*.
 */
const PEOPLE: Record<string, string> = {
  ML: "Mara Lindqvist",
  JB: "Jonas Becker",
  PN: "Priya Nair",
  TW: "Tomasz Wójcik",
  LH: "Lena Hoffmann",
};
const PAY_231_THREAD: [string, string, string][] = [
  ["LH", "2026-08-12 09:14", "Finance keeps chasing failed SEPA payouts by hand, about 30 a week. Can we retry the transient ones automatically?"],
  ["PN", "2026-08-12 09:40", "Yes. @mara can you pick this up after PAY-219? Keep it to transient PSP errors only."],
  ["ML", "2026-08-12 10:02", "Sure. Which codes count as transient? I only know 503."],
  ["PN", "2026-08-12 10:30", "503 and TEMP_UNAVAILABLE. The PSP docs list RATE_LIMITED too, but they say not to retry that one automatically."],
  ["TW", "2026-08-12 11:05", "If you retry, please don’t hammer the PSP. Last outage we got rate limited for an hour."],
  ["ML", "2026-08-13 16:20", "Plan: exponential backoff, base 30 s, factor 2. Max attempts still open."],
  ["LH", "2026-08-14 09:00", "Finance asks for at most 5 attempts. After that it should land in the manual review queue."],
  ["JB", "2026-08-14 09:12", "5 attempts from base 30 s is about 15 min until the last one. Fine for the 13:00 batch?"],
  ["LH", "2026-08-14 09:30", "Yes, the cutoff is 15:00."],
  ["ML", "2026-08-18 09:06", "Starting. The design goes on the SEPA payout retry design page."],
  ["PN", "2026-08-19 11:45", "Add jitter, otherwise all retries of a batch hit the PSP in the same second."],
  ["ML", "2026-08-19 12:10", "Will do. ±20 %?"],
  ["TW", "2026-08-20 08:50", "Staging PSP sandbox is flaky this week, don’t trust red integration tests blindly."],
  ["PN", "yesterday 14:20", "Please make sure the backoff policy matches the design page. Finance wants max 5 attempts."],
  ["LH", "yesterday 15:02", "@mara will the manual review queue get an SLA? Finance asked twice."],
  ["ML", "yesterday 15:30", "Priya, can you decide the SLA? I’ll document it."],
  ["ML", "yesterday 17:30", "First version pushed on feature/PAY-231-sepa-retry."],
  ["JB", "today 09:35", "Left a comment on the PR about the jitter width."],
  ["ML", "today 10:15", "Backoff + jitter implemented in a41f2c, integration test still red (#1187), investigating."],
  ["TW", "today 12:45", "flows-stage is down since 12:40, so sandbox calls from stage time out right now. Looking into it."],
];

/** A prototype time (`today 09:35`, `yesterday 17:41`, `2026-08-12 09:14`) on the page's own clock. */
function fixtureTime(said: string): string {
  const [day, clock] = said.split(" ") as [string, string];
  const [hours, minutes] = clock.split(":").map(Number) as [number, number];
  let at: Date;
  if (day === "today" || day === "yesterday") {
    at = new Date();
    if (day === "yesterday") at.setDate(at.getDate() - 1);
  } else {
    const [year, month, date] = day.split("-").map(Number) as [number, number, number];
    at = new Date(year, month - 1, date);
  }
  at.setHours(hours, minutes, 0, 0);
  return at.toISOString();
}

/** A thread change with nothing said yet (#929): each line fills in its own. */
const QUIET = { status: null, from: null, by: null, mine: false, number: null, repo: null };

function entityThread(args: Record<string, unknown>): Thread {
  const id = String(args["entityId"] ?? "");
  if (!CORPUS.some((candidate) => candidate.entity_id === id)) {
    throw { code: "not_found", message: `${id} is not in the mirror`, source_id: null };
  }
  if (id !== "mock:PAY-231") return { comments: [], changes: [], me: ["mara"] };
  return {
    comments: PAY_231_THREAD.map(([who, at, body]) => ({
      author: PEOPLE[who] ?? who,
      at: fixtureTime(at),
      body,
      mine: who === "ML",
      mentions_me: /@mara(?![\p{L}\p{N}_.-])/u.test(body),
    })),
    changes: [
      { ...QUIET, at: fixtureTime("2026-08-14 16:20"), kind: "opened", number: "142", repo: "payout-service" },
      { ...QUIET, at: fixtureTime("2026-08-18 09:05"), kind: "status", status: "In Progress", mine: true },
      {
        ...QUIET,
        at: fixtureTime("2026-08-19 11:40"),
        kind: "status",
        status: "In Review",
        from: "In Progress",
        by: "Priya Nair",
      },
      { ...QUIET, at: fixtureTime("2026-08-20 15:10"), kind: "merged", number: "57", repo: "psp-client" },
    ],
    me: ["mara"],
  };
}

/**
 * `visit_entity` (#819): the reader was last here at 09:00 today, as in
 * prototype-6, on every open; the fixture records nothing.
 */
function visitEntity(args: Record<string, unknown>): string | null {
  const id = String(args["entityId"] ?? "");
  if (!CORPUS.some((candidate) => candidate.entity_id === id)) {
    throw { code: "not_found", message: `${id} is not in the mirror`, source_id: null };
  }
  return fixtureTime("today 09:00");
}

/** This feature's commands. */
export function handlers(): Handlers {
  return {
    get_entity: (args) => getEntity(args),
    refresh_entity: (args) => refreshEntity(args),
    // A reload starts the fixture over, so nothing filed before it is owed (#1953).
    owed_tickets: () => [],
    entity_thread: (args) => entityThread(args),
    visit_entity: (args) => visitEntity(args),
    reachable_transitions: (args) => fakeTransitions(args),
    entity_view: (args) => entityView(args),
    // Every fake source's credential may make every write it declares (#2040).
    refused_writes: () => [],
    resolve_url: (args) => resolveUrl(args),
    create_link: (args) => {
      const from = String(args["fromId"]);
      const to = String(args["toId"]);
      for (const end of [from, to]) {
        if (linkEnd(end) === null) {
          throw { code: "not_found", message: `${end} is not in the mirror`, source_id: null };
        }
      }
      const made = linkRow(from, to, String(args["relation"] ?? "related"), "manual", (args["note"] as string | null) ?? null);
      LINKS.push(made);
      return made;
    },
    unlink: (args) => {
      const at = LINKS.findIndex((link) => link.id === args["linkId"]);
      if (at === -1) throw { code: "not_found", message: `${String(args["linkId"])} is not a link`, source_id: null };
      LINKS.splice(at, 1);
      return null;
    },
  };
}

keepSeed(LINKS);
