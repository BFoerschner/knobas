/**
 * `?fake-ipc`: Link sets (#866): `link_sets`, `link_set`, `save_link_set`,
 * `delete_link_set`, `preview_link_set_rule` and `hide_link_set_member`, over
 * a session-long list seeded with prototype-6's five sets (its `sets` array),
 * each member a fixture item by id.
 *
 * **Fixture-only.** Prototype-6's two patterns (*Payout_* and PspClient_**,
 * *Pages under OPS › Runbooks*) are fixed members here: patterns are #868's.
 * Rules match the corpus's flat `epic` key, the sub-task map `job-reach.ts`
 * keeps, and `fields.project.key`; *yours* is `mara`, the fixture's reader.
 * A restatement that certifies nothing: the real matching is witnessed in
 * `knobas-app`'s `link_sets_ipc.rs`.
 *
 * `fakeSetReach` is what `job_reach` adds for the sets (registered in
 * `job-reach.ts`'s `SET_REACH` when this module loads), so every *This job*
 * view under `?fake-ipc` sees set members.
 */
import type {
  LinkSetDetail,
  LinkSetDraft,
  LinkSetMemberKind,
  LinkSetRow,
  LinkSetRule,
  LinkSetRuleKind,
  Reached,
  RuleTicket,
} from "../../../ipc/entity";
import type { Handler } from "../fake-tauri";
import { fakePipelines } from "./builds";
import { CORPUS } from "./corpus";
import { FIXTURE_ESTATE } from "./estate";
import { PARENTS, SET_REACH } from "./job-reach";
import { fakeMonitorRoster } from "./monitors";

interface FakeSet {
  id: string;
  name: string;
  description: string;
  members: { kind: LinkSetMemberKind; entity_id: string }[];
  rules: LinkSetRule[];
}

const tc = (config: string) => `teamcity:buildType:${config}`;
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
      ...m("pipeline", ...["Payout_Build", "Payout_IntegrationTests", "Payout_Lint", "Payout_Docker", "PspClient_Build", "PspClient_Release"].map(tc)),
      ...m("page", "mock:ENG-PSP-codes", "confluence:sepa"),
      ...m("asset", "asset:knobas-gitea", "asset:orbstack-docker"),
      ...m("monitor", "kuma:5"),
    ],
    rules: [r("epic", "mock:PAY-200"), r("epic", "mock:PAY-300")],
  },
  {
    id: "staging",
    name: "Payments staging",
    description: "The stage stack the payout and ledger services run on.",
    members: [...m("asset", "asset:knobas-jira", "asset:knobas-jira-db", "asset:db-jira"), ...m("monitor", "kuma:3")],
    rules: [r("manual", "mock:PAY-231"), r("manual", "mock:PAY-240")],
  },
  {
    id: "ledger",
    name: "Ledger stack",
    description: "ledger-api and fx-rates, the Ledger builds, the reconciliation runbook.",
    members: [
      ...m("repo", "mock:tidewater/ledger", "mock:tidewater/fx-rates"),
      ...m("pipeline", ...["Ledger_Build", "Ledger_Tests", "Ledger_Migrations_Check", "FxRates_Build"].map(tc)),
      ...m("page", "mock:OPS-manual-review"),
      ...m("asset", "asset:knobas-teamcity"),
      ...m("monitor", "kuma:2"),
    ],
    rules: [r("epic", "mock:PAY-150"), r("manual", "mock:PAY-228")],
  },
  {
    id: "finance-docs",
    name: "Finance docs",
    description: "Every page in the Finance space, and the finance requests.",
    members: m("page", "mock:ENG-SEPA"),
    rules: [r("epic", "mock:PAY-150")],
  },
  {
    id: "oncall",
    name: "On-call kit",
    description: "The on-call handbook and every runbook.",
    members: m("page", "mock:OPS-manual-review", "confluence:manual-review"),
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

function tickets() {
  return CORPUS.filter((row) => row.kind === "ticket" && row.deleted_at === null);
}

function keyOf(id: string): string {
  return id.slice(id.indexOf(":") + 1);
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
    members: set.members.map((member) => ({ ...member, title: title(member.entity_id) })),
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
    for (const member of set.members) {
      if (HIDDEN.has(`${ticketId}|${member.entity_id}`) || out.some((row) => row.entity_id === member.entity_id)) continue;
      out.push({ entity_id: member.entity_id, kind: member.kind, step: member.entity_id, origin: { kind: "link_set", set: set.id, name: set.name } });
    }
  }
  return out;
}

SET_REACH.push(fakeSetReach);

/** This feature's commands. */
export function handlers(): Record<string, Handler> {
  return {
    link_sets: () => [...SETS].sort((a, b) => a.name.toLowerCase().localeCompare(b.name.toLowerCase())).map(rowOf),
    link_set: (args) => detailOf(found(String(args["id"] ?? ""))),
    save_link_set: (args) => {
      const draft = args["draft"] as LinkSetDraft;
      if (!draft.name.trim()) throw { code: "invalid", message: "a link set needs a name", source_id: null };
      const set: FakeSet = {
        id: draft.id ?? slug(draft.name),
        name: draft.name.trim(),
        description: draft.description.trim(),
        members: draft.members.map((member) => ({ ...member })),
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
    hide_link_set_member: (args) => {
      const pair = `${String(args["ticketId"])}|${String(args["memberId"])}`;
      if (args["hidden"]) HIDDEN.add(pair);
      else HIDDEN.delete(pair);
      return Boolean(args["hidden"]);
    },
  };
}
