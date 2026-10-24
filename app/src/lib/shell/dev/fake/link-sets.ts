/**
 * `?fake-ipc`: Link sets (#866): `link_sets`, `link_set`, `save_link_set`,
 * `delete_link_set`, `preview_link_set_rule`, `hide_link_set_member`,
 * `link_set_hints` (#870) and
 * `job_link_sets` (#867, the job's *Also on this job* and drawer groups), over
 * a session-long list seeded with prototype-6's five sets (its `sets` array),
 * each member a fixture item by id.
 *
 * **Fixture-only.** Rules match the corpus's flat `epic` key, the sub-task
 * map `job-reach.ts` keeps, and `fields.project.key`; *yours* is `mara`, the
 * fixture's reader. Patterns (#868, `preview_link_set_pattern`) read
 * `source:`, `project:`/`space:`, `under:` and `name:` here with a small
 * tokenizer: a pipeline's project is its last group, a page's space and
 * parents its `path`. Member rows' line and dot (#1008) come from the same
 * fixture rows the Builds, Docs, Assets and Monitors views draw. A
 * restatement that certifies nothing: the real matching is witnessed in
 * `knobas-app`'s `link_sets_ipc.rs` and `link_set_patterns_ipc.rs`.
 *
 * `fakeSetReach` is what `job_reach` adds for the sets (registered in
 * `job-reach.ts`'s `SET_REACH` when this module loads), so every *This job*
 * view under `?fake-ipc` sees set members.
 */
import type {
  JobLinkSets,
  LinkSetDetail,
  LinkSetDraft,
  LinkSetHint,
  LinkSetMemberKind,
  LinkSetMemberRow,
  LinkSetPattern,
  LinkSetPatternPreview,
  LinkSetPatternRow,
  LinkSetRow,
  LinkSetRule,
  LinkSetRuleKind,
  Reached,
  RuleTicket,
  StateChip,
} from "../../../ipc/entity";
import { keyOf } from "../../../time/format";
import type { Handlers } from "../fake-tauri";
import { keepSeed, onFakeReset, seedOf } from "../fake-seed";
import { fakePipelines } from "./builds";
import { CORPUS } from "./corpus";
import { FIXTURE_ESTATE } from "./estate";
import { PARENTS, SET_REACH, fakeParentOwn } from "./job-reach";
import { fakeMonitorRoster } from "./monitors";

interface FakeSet {
  id: string;
  name: string;
  description: string;
  members: { kind: LinkSetMemberKind; entity_id: string }[];
  patterns: LinkSetPattern[];
  rules: LinkSetRule[];
}

const p = (kind: LinkSetMemberKind, query: string): LinkSetPattern => ({ kind, query });

const m = (kind: LinkSetMemberKind, ...ids: string[]) => ids.map((entity_id) => ({ kind, entity_id }));
const r = (kind: LinkSetRuleKind, value: string): LinkSetRule => ({ kind, value });

/** prototype-6's `EPICS`: the epics the fixture names but does not mirror. */
const EPIC_TITLES: Record<string, string> = {
  "mock:PAY-200": "Payout reliability",
  "mock:PAY-300": "PSP migration",
  "mock:PAY-150": "Finance reporting",
};

/** prototype-6's sets, in its words, over the fixture's items. */
let SETS: FakeSet[] = [
  {
    id: "payout-stack",
    name: "Payout stack",
    description: "What a change on the payout path touches: the repos, the Payouts builds, the PSP pages and the payout monitors.",
    members: [
      ...m("repo", "mock:tidewater/payout-service", "mock:tidewater/psp-client"),
      ...m("page", "confluence:psp-codes", "confluence:sepa"),
      ...m("asset", "asset:knobas-gitea", "asset:orbstack-docker"),
      ...m("monitor", "kuma:5"),
    ],
    patterns: [p("pipeline", "project:Payouts name:Payout_*"), p("pipeline", 'project:"PSP client" name:PspClient_*')],
    rules: [r("epic", "mock:PAY-200"), r("epic", "mock:PAY-300")],
  },
  {
    id: "staging",
    name: "Payments staging",
    description: "The stage stack the payout and ledger services run on.",
    members: [...m("asset", "asset:knobas-jira", "asset:knobas-jira-db", "asset:db-jira"), ...m("monitor", "kuma:3")],
    patterns: [],
    rules: [r("manual", "mock:PAY-231"), r("manual", "mock:PAY-240")],
  },
  {
    id: "ledger",
    name: "Ledger stack",
    description: "ledger-api and fx-rates, the Ledger builds, the reconciliation runbook.",
    members: [
      ...m("repo", "mock:tidewater/ledger", "mock:tidewater/fx-rates"),
      ...m("page", "confluence:manual-review"),
      ...m("asset", "asset:knobas-teamcity"),
      ...m("monitor", "kuma:2"),
    ],
    patterns: [p("pipeline", "project:Ledger")],
    rules: [r("epic", "mock:PAY-150"), r("manual", "mock:PAY-228")],
  },
  {
    id: "finance-docs",
    name: "Finance docs",
    description: "Every page in the Finance space, and the finance requests.",
    members: m("page", "confluence:sepa"),
    patterns: [],
    rules: [r("epic", "mock:PAY-150")],
  },
  {
    id: "oncall",
    name: "On-call kit",
    description: "The on-call handbook and every runbook.",
    members: m("page", "confluence:manual-review"),
    patterns: [p("page", "project:OPS under:Runbooks")],
    rules: [r("project", "mock:OPS")],
  },
];

/** `ticket|member` pairs hidden for the session (R-7.5). */
const HIDDEN = new Set<string>();

function title(id: string): string | null {
  const row = CORPUS.find((entry) => entry.entity_id === id);
  if (row) return row.title;
  const pipeline = fakePipelines().find((entry) => entry.entity_id === id);
  if (pipeline) return pipeline.name;
  const asset = FIXTURE_ESTATE.find((entry) => entry.id === id);
  if (asset) return asset.name;
  const monitor = fakeMonitorRoster().find((entry) => entry.entity_id === id);
  if (monitor) return monitor.name;
  return { "confluence:sepa": "SEPA payout retry design", "confluence:manual-review": "Manual review queue" }[id] ?? null;
}

const TONES: Record<string, StateChip["tone"]> = { up: "ok", warn: "warn", down: "err", pending: "info" };

/** Uptime Kuma's declared `state` chip (#1020): its four words, each in its tone. */
const MONITOR_TONES: Record<string, StateChip["tone"]> = { up: "ok", down: "err", pending: "warn", maintenance: "muted" };

/** A member's row (#1008): its title, its line and its dot, off the fixture rows the views draw. */
function memberRow(kind: LinkSetMemberKind, id: string): LinkSetMemberRow {
  const row: LinkSetMemberRow = { kind, entity_id: id, title: title(id), line: null, state: null };
  const pipeline = fakePipelines().find((entry) => entry.entity_id === id);
  if (pipeline) return { ...row, line: pipeline.path.join(" › ") || null, state: pipeline.recent[0]?.state ?? null };
  const asset = FIXTURE_ESTATE.find((entry) => entry.id === id);
  if (asset) {
    const tone = TONES[asset.status];
    return {
      ...row,
      line: asset.environment ? `${asset.type_id}, ${asset.environment}` : asset.type_id,
      state: tone ? { text: asset.status, tone } : null,
    };
  }
  const monitor = fakeMonitorRoster().find((entry) => entry.entity_id === id);
  if (monitor) {
    const tone = monitor.state ? MONITOR_TONES[monitor.state] : undefined;
    return {
      ...row,
      line: monitor.monitor_type || null,
      state: monitor.state && tone ? { text: monitor.state, tone } : null,
    };
  }
  const item = CORPUS.find((entry) => entry.entity_id === id);
  const payload = (item?.payload ?? {}) as { path?: string; chip?: StateChip; teams?: string[]; full_name?: string };
  if (item?.kind === "repo") {
    // #1020: the declared teams, else the owner Gitea's `owner.login` names.
    const teams = (payload.teams ?? []).join(", ");
    const owner = payload.full_name?.includes("/") ? payload.full_name.split("/")[0] : null;
    return { ...row, line: teams || owner || null };
  }
  return { ...row, line: payload.path ?? null, state: payload.chip ?? null };
}

/** The live items of a member kind a pattern can match, with what it reads: project, parents, name. */
function candidates(kind: LinkSetMemberKind): { id: string; source: string; projects: string[]; under: string[]; names: string[] }[] {
  if (kind === "pipeline") {
    // A pipeline answers to every project of its chain (#1019).
    return fakePipelines().map((row) => ({
      id: row.entity_id,
      source: row.source_id,
      projects: row.path.length ? row.path : [row.group ?? ""],
      under: [],
      names: [row.name, keyOf(row.entity_id)],
    }));
  }
  if (kind !== "page") return [];
  return CORPUS.filter((row) => row.kind === "page" && row.deleted_at === null).map((row) => {
    const path = String((row.payload as { path?: string }).path ?? "").split(" › ").filter(Boolean);
    return { id: row.entity_id, source: row.entity_id.split(":")[0] ?? "", projects: path.slice(0, 1), under: [...path.slice(1), row.title], names: [row.title, keyOf(row.entity_id)] };
  });
}

/** A pattern's query read as `knobas_search::pattern_of` reads it, or why it is not one. */
function readPattern(query: string): { sources: string[]; projects: string[]; under: string[]; names: string[] } | string {
  const read = { sources: [] as string[], projects: [] as string[], under: [] as string[], names: [] as string[] };
  for (const token of query.match(/\S+:"[^"]*"|\S+/g) ?? []) {
    const at = token.indexOf(":");
    const key = token.slice(0, at).toLowerCase();
    const value = token.slice(at + 1).replace(/^"|"$/g, "").trim().toLowerCase();
    const list = at < 1 ? undefined : { source: read.sources, project: read.projects, space: read.projects, under: read.under, name: read.names }[key];
    if (!list) return "a pattern reads source:, project:, under: and name: and nothing else";
    if (value && !list.includes(value)) list.push(value);
  }
  if (!read.sources.length && !read.projects.length && !read.under.length && !read.names.length) return "a pattern needs a project, a parent or a name to match";
  return read;
}

function glob(pattern: string, text: string): boolean {
  const source = pattern.replace(/[.+^${}()|[\]\\]/g, "\\$&").replace(/\*/g, ".*").replace(/\?/g, ".");
  return new RegExp(`^${source}$`, "i").test(text);
}

/** The ids a pattern matches now, by title. */
function patternHits(pattern: LinkSetPattern): string[] | string {
  if (pattern.kind !== "pipeline" && pattern.kind !== "page") return `a pattern is for pipeline and page, not a ${pattern.kind}`;
  const read = readPattern(pattern.query);
  if (typeof read === "string") return read;
  const any = (wanted: string[], have: string[]) => !wanted.length || wanted.some((w) => have.some((h) => h.toLowerCase() === w));
  return candidates(pattern.kind)
    .filter((c) => any(read.sources, [c.source]) && any(read.projects, c.projects) && any(read.under, c.under))
    .filter((c) => !read.names.length || read.names.some((g) => c.names.some((n) => glob(g, n))))
    .map((c) => c.id)
    .sort((a, b) => (title(a) ?? a).toLowerCase().localeCompare((title(b) ?? b).toLowerCase()));
}

/** A set's fixed members, then what its patterns match now, each item once. */
function setItems(set: FakeSet): { kind: LinkSetMemberKind; entity_id: string }[] {
  const matched = set.patterns.flatMap((pattern) => {
    const hits = patternHits(pattern);
    return typeof hits === "string" ? [] : hits.map((entity_id) => ({ kind: pattern.kind, entity_id }));
  });
  return [...set.members, ...matched].filter((item, at, all) => all.findIndex((other) => other.entity_id === item.entity_id) === at);
}

function patternRow(pattern: LinkSetPattern): LinkSetPatternRow {
  const hits = patternHits(pattern);
  const ids = typeof hits === "string" ? [] : hits;
  return { ...pattern, count: ids.length, matches: ids.slice(0, 200).map((id) => memberRow(pattern.kind, id)) };
}

function preview(pattern: LinkSetPattern): LinkSetPatternPreview {
  // Each project of a pipeline's chain, with the names down to it (#1019); a page's space has none.
  const nested = pattern.kind === "pipeline";
  const projects = [...new Set(candidates(pattern.kind).flatMap((c) => c.projects.map((_, at) => `${c.source}|${c.projects.slice(0, at + 1).join("|")}`)))]
    .map((joined) => {
      const [source_id = "", ...path] = joined.split("|");
      const key = path.at(-1) ?? "";
      return { source_id, key, name: key, path: nested ? path : [] };
    })
    .filter((p, at, all) => all.findIndex((o) => o.source_id === p.source_id && o.key === p.key) === at)
    .sort((a, b) => (a.path.join(" › ") || a.key).localeCompare(b.path.join(" › ") || b.key));
  const hits = patternHits(pattern);
  if (typeof hits === "string") return { count: 0, matches: [], projects, refusal: hits };
  return { count: hits.length, matches: hits.slice(0, 200).map((id) => memberRow(pattern.kind, id)), projects, refusal: null };
}

function tickets() {
  return CORPUS.filter((row) => row.kind === "ticket" && row.deleted_at === null);
}

function matches(rule: LinkSetRule, ticket: (typeof CORPUS)[number]): boolean {
  const payload = ticket.payload as { epic?: string; fields?: { project?: { key?: string } } };
  switch (rule.kind) {
    case "manual":
      return ticket.entity_id === rule.value;
    case "parent":
      return PARENTS[ticket.entity_id] === rule.value;
    case "epic": {
      if (payload.epic === keyOf(rule.value)) return true;
      const parent = tickets().find((row) => row.entity_id === PARENTS[ticket.entity_id]);
      return parent !== undefined && parent !== ticket && matches(rule, parent);
    }
    case "project":
      return `mock:${payload.fields?.project?.key ?? ""}` === rule.value;
  }
}

function ruleTickets(rules: readonly LinkSetRule[]): RuleTicket[] {
  return tickets()
    .filter((ticket) => rules.some((rule) => matches(rule, ticket)))
    .map((ticket) => ({ entity_id: ticket.entity_id, title: ticket.title, mine: (ticket.payload as { assignee?: string | null }).assignee === "mara" }))
    .sort((a, b) => a.entity_id.length - b.entity_id.length || a.entity_id.localeCompare(b.entity_id));
}

function rowOf(set: FakeSet): LinkSetRow {
  const on = ruleTickets(set.rules);
  return {
    id: set.id,
    name: set.name,
    description: set.description,
    members: set.members.map((member) => memberRow(member.kind, member.entity_id)),
    patterns: set.patterns.map(patternRow),
    rules: set.rules.map((rule) => ({ ...rule, title: EPIC_TITLES[rule.value] ?? title(rule.value) })),
    tickets: on.length,
    mine: on.filter((ticket) => ticket.mine).length,
  };
}

function detailOf(set: FakeSet): LinkSetDetail {
  return { ...rowOf(set), shows_on: ruleTickets(set.rules) };
}

function found(id: string): FakeSet {
  const set = SETS.find((entry) => entry.id === id);
  if (!set) throw { code: "not_found", message: `no link set is called ${id}`, source_id: null };
  return set;
}

function slug(name: string): string {
  const made = name.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "") || "set";
  let candidate = made;
  for (let n = 2; SETS.some((set) => set.id === candidate); n += 1) candidate = `${made}-${n}`;
  return candidate;
}

/** What `job_reach` adds for `ticketId`'s sets: each member once, the set first in name order. */
export function fakeSetReach(ticketId: string): Reached[] {
  const ticket = tickets().find((row) => row.entity_id === ticketId);
  if (!ticket) return [];
  const out: Reached[] = [];
  for (const set of [...SETS].sort((a, b) => a.id.localeCompare(b.id))) {
    if (!set.rules.some((rule) => matches(rule, ticket))) continue;
    for (const member of setItems(set)) {
      if (HIDDEN.has(`${ticketId}|${member.entity_id}`) || out.some((row) => row.entity_id === member.entity_id)) continue;
      out.push({ entity_id: member.entity_id, kind: member.kind, step: member.entity_id, origin: { kind: "link_set", set: set.id, name: set.name } });
    }
  }
  return out;
}

SET_REACH.push(fakeSetReach);

/** `job_link_sets`: every set whose rules match, by name, with its first matching rule, and the parent. */
export function fakeJobLinkSets(ticketId: string): JobLinkSets {
  const ticket = tickets().find((row) => row.entity_id === ticketId);
  const sets = ticket
    ? [...SETS]
        .sort((a, b) => a.name.toLowerCase().localeCompare(b.name.toLowerCase()) || a.id.localeCompare(b.id))
        .flatMap((set) => {
          const via = set.rules.find((rule) => matches(rule, ticket));
          if (!via) return [];
          return [
            {
              id: set.id,
              name: set.name,
              via: { ...via, title: EPIC_TITLES[via.value] ?? title(via.value) },
              members: setItems(set)
                .filter((member) => member.entity_id !== ticketId)
                .map((member) => ({
                  ...member,
                  entity_kind: member.kind === "pipeline" ? "build_config" : member.kind,
                  title: title(member.entity_id),
                  hidden: HIDDEN.has(`${ticketId}|${member.entity_id}`),
                })),
            },
          ];
        })
    : [];
  const parent = fakeParentOwn(ticketId);
  return {
    sets,
    parent: parent && {
      ticket: parent.ticket,
      title: title(parent.ticket),
      links: parent.reached.map((row) => ({ entity_id: row.entity_id, kind: row.kind, title: title(row.entity_id) })),
    },
  };
}

/** This feature's commands. */
/**
 * `link_set_hints` (#870, R-7.10): prototype-6's one hint, Dashboard_Build
 * linked by hand to three tickets under PAY-200, offered to *Payout stack*
 * while that set exists and holds no such item. Fixture-only: the real
 * query over links is witnessed in `link_sets_ipc.rs`.
 */
function fakeHints(): LinkSetHint[] {
  const set = SETS.find((entry) => entry.id === "payout-stack");
  const build = fakePipelines().find((row) => row.name === "Dashboard_Build");
  if (!set || !build) return [];
  if (setItems(set).some((item) => item.entity_id === build.entity_id)) return [];
  return [
    {
      set_id: set.id,
      set_name: set.name,
      epic: "mock:PAY-200",
      member: { kind: "pipeline", entity_id: build.entity_id },
      title: "Dashboard_Build",
      tickets: ["mock:PAY-232", "mock:PAY-236", "mock:PAY-240"],
    },
  ];
}

export function handlers(): Handlers {
  return {
    link_sets: () => [...SETS].sort((a, b) => a.name.toLowerCase().localeCompare(b.name.toLowerCase())).map(rowOf),
    link_set: (args) => detailOf(found(String(args["id"] ?? ""))),
    save_link_set: (args) => {
      const draft = args["draft"] as LinkSetDraft;
      if (!draft.name.trim()) throw { code: "invalid", message: "a link set needs a name", source_id: null };
      if (draft.id !== null) found(draft.id);
      for (const pattern of draft.patterns ?? []) {
        const hits = patternHits(pattern);
        if (typeof hits === "string") throw { code: "invalid", message: `\`${pattern.query}\` is not a pattern: ${hits}`, source_id: null };
      }
      const set: FakeSet = {
        id: draft.id ?? slug(draft.name),
        name: draft.name.trim(),
        description: draft.description.trim(),
        members: draft.members.map((member) => ({ ...member })),
        patterns: (draft.patterns ?? []).map((pattern) => ({ kind: pattern.kind, query: pattern.query.trim() })),
        rules: draft.rules.map((rule) => ({ ...rule })),
      };
      SETS = [...SETS.filter((entry) => entry.id !== set.id), set];
      return detailOf(set);
    },
    delete_link_set: (args) => {
      const set = found(String(args["id"] ?? ""));
      SETS = SETS.filter((entry) => entry !== set);
      return null;
    },
    preview_link_set_rule: (args) => ruleTickets([args["rule"] as LinkSetRule]),
    job_link_sets: (args) => fakeJobLinkSets(String(args["entityId"] ?? "")),
    preview_link_set_pattern: (args) => preview(args["pattern"] as LinkSetPattern),
    link_set_hints: () => fakeHints(),
    hide_link_set_member: (args) => {
      const pair = `${String(args["ticketId"])}|${String(args["memberId"])}`;
      if (args["hidden"]) HIDDEN.add(pair);
      else HIDDEN.delete(pair);
      return Boolean(args["hidden"]);
    },
  };
}

const seededSets = seedOf(SETS);
onFakeReset(() => {
  SETS = seededSets();
});
keepSeed(HIDDEN);
