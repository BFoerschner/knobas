/**
 * `?fake-ipc`: Ticket views (#812): `ticket_views`, `save_ticket_view` and
 * `delete_ticket_view` (and #878's `hide_saved_view` and `open_saved_view`, and #1208's `translate_ticket_view`),
 * over a session-long list seeded with prototype-6's own
 * views (its `views` array), each as search text in the grammar the real
 * command reads.
 *
 * **Fixture-only.** The counts are `fakeTicketCount`'s restatement of the
 * query over this corpus, and a text is unreadable here when it names a key
 * the fixture's matcher does not know, or `sprint:` with anything but
 * `current`: a rough copy of `knobas_search::query::parse`'s unknown tokens,
 * which certifies nothing. The real reading is witnessed in `knobas-app`'s
 * `ticket_views_ipc.rs`.
 *
 * prototype-6's *Assigned to me today* (`assigned:today`) and *Mine with a
 * failing build* read through keys the matcher passes over, so their counts
 * here are wider than the real ones would be. *Blocked* is `has:blocker`.
 */
import type { LaneColumn, LaneField, MiniBoardCard, MiniBoardLane, SourceStatuses } from "../../../ipc/entity";
import type { QueryKey, QueryTranslation, SavedViewSource, ViewLanes, CardField, ListColumn, ListSort } from "../../../ipc/search";
import { laneOf } from "../../../tf/tickets/tickets";
import type { Handlers } from "../fake-tauri";
import { keepSeed } from "../fake-seed";
import { fakeTicketCount, fakeTicketKeys } from "./corpus";

type LanedCard = Pick<
  MiniBoardCard,
  "entity_id" | "updated_at" | "path" | "assignee" | "priority" | "issue_type" | "labels"
>;
interface LanedBoard<C extends LanedCard> {
  columns: { status: string | null; cards: C[]; total: number }[];
  sources: SourceStatuses[];
}

/**
 * `mini_board`'s `lanes` (#939), over the fixture: a whole board put in
 * lanes after it is read. A restatement of `knobas_core::mini_board::in_lanes`,
 * which is the authority. Each lane's column keeps its first `perColumn`
 * cards in the column's order and counts them all; lanes rank biggest
 * first, a tie to the lane with the newer ticket, then by key; only the
 * first `first` are kept. The columns keep their totals and the kept cards.
 */
export function fakeLanes<C extends LanedCard>(
  whole: LanedBoard<C>,
  by: LaneField,
  perColumn: number | null,
  first: number | null,
): LanedBoard<C> & { lanes: MiniBoardLane[]; more_lanes: number } {
  const lanes = new Map<string, { count: number; newest: string; columns: LaneColumn[] }>();
  for (const column of whole.columns) {
    for (const card of column.cards) {
      const key = laneOf(card, by);
      const lane = lanes.get(key) ?? { count: 0, newest: "", columns: [] };
      lanes.set(key, lane);
      lane.count += 1;
      if ((card.updated_at ?? "") > lane.newest) lane.newest = card.updated_at ?? "";
      let cell = lane.columns.at(-1);
      if (cell === undefined || cell.status !== column.status) {
        cell = { status: column.status, ids: [], total: 0 };
        lane.columns.push(cell);
      }
      cell.total += 1;
      if (perColumn === null || cell.ids.length < perColumn) cell.ids.push(card.entity_id);
    }
  }
  const ranked = [...lanes]
    .sort(([aKey, a], [bKey, b]) => b.count - a.count || b.newest.localeCompare(a.newest) || aKey.localeCompare(bKey))
    .map(([key, lane]): MiniBoardLane => ({ key, count: lane.count, columns: lane.columns }));
  const sent = first === null ? ranked : ranked.slice(0, Math.max(1, first));
  const kept = new Set(sent.flatMap((lane) => lane.columns.flatMap((cell) => cell.ids)));
  return {
    columns: whole.columns.map((column) => ({
      ...column,
      cards: column.cards.filter((card) => kept.has(card.entity_id)),
    })),
    sources: whole.sources,
    lanes: sent,
    more_lanes: ranked.length - sent.length,
  };
}

interface View {
  id: string;
  name: string;
  group: string;
  position: number;
  query: string;
  layout: "board" | "list";
  columns: { name: string; statuses: string[]; limit: number | null }[];
  lanes: ViewLanes;
  card_fields: CardField[];
  list_columns: ListColumn[];
  list_sort: ListSort | null;
  saved_view: string | null;
}

const CARD: CardField[] = ["type", "signals", "comments", "assignee"];
const LIST: ListColumn[] = ["key", "summary", "status", "priority", "signals", "assignee", "updated"];

function seeded(id: string, name: string, group: string, query: string, layout: "board" | "list" = "list"): View {
  return { id, name, group, position: 0, query, layout, columns: [], lanes: "none", card_fields: CARD, list_columns: LIST, list_sort: null, saved_view: null };
}

/** prototype-6's views, in its order and words. */
const VIEWS: View[] = [
  seeded("my-work", "My work", "Mine", "assignee:@me", "board"),
  seeded("my-open-bugs", "My open bugs", "Mine", "assignee:@me issuetype:Bug -status:Done"),
  seeded("assigned-to-me-today", "Assigned to me today", "Mine", "assigned:1d"),
  seeded("mine-with-a-failing-build", "Mine with a failing build", "Mine", "assignee:@me has:failing-build"),
  seeded("payments-backlog", "Payments backlog", "Team", 'project:PAY status:"To Do"'),
  seeded("blocked", "Blocked", "Team", "has:blocker"),
  seeded("payout-reliability", "Payout reliability", "Epics", "under:PAY-200", "board"),
  seeded("psp-migration", "PSP migration", "Epics", "under:PAY-300", "board"),
  seeded("finance-reporting", "Finance reporting", "Epics", "under:PAY-150", "board"),
  seeded("observability", "Observability", "Epics", "under:OPS-40", "board"),
  seeded("payments-pay", "Payments (PAY)", "Projects", "project:PAY"),
  seeded("operations-ops", "Operations (OPS)", "Projects", "project:OPS"),
  seeded("customs-cus", "Customs (CUS)", "Projects", "project:CUS"),
  seeded("tracking-trk", "Tracking (TRK)", "Projects", "project:TRK"),
];

/**
 * prototype-6's Jira boards (its `JB`, #877), as the sync would project them:
 * a ticket view row each under *Jira boards*, Jira's columns and limits, and
 * where it comes from. `member` stands in for the keys Jira's query returns;
 * Jira's order here is the key's number.
 */
interface FakeSavedView {
  ref: string;
  member: (key: string) => boolean;
  /** `translate_ticket_view` (#1208): the Jira adapter's reading of its query, written out as it answers. */
  copy: QueryTranslation;
  /** The row's `saved_view_source`, typed by the mirror so the fake cannot drift from it (#1273). */
  source: SavedViewSource;
}
const READ = "2026-10-17T09:56:00Z";
const col = (name: string, statuses: string[], limit: number | null = null) => ({ name, statuses, limit });
const SAVED: FakeSavedView[] = [];
function jiraBoard(
  id: number,
  name: string,
  type: string,
  jql: string,
  columns: View["columns"],
  lanes: ViewLanes,
  member: (key: string) => boolean,
  copy: QueryTranslation,
  extra: { sub?: string; sprint?: string; outside?: { project: string; count: number } } = {},
) {
  const ref = `jira:board-${id}`;
  SAVED.push({
    ref,
    member,
    copy,
    source: {
      kind: "board",
      system: "Jira",
      board_type: type,
      source_query: jql,
      sub_query: extra.sub ?? null,
      sprint: extra.sprint ? { id, name: extra.sprint, end_date: "2026-10-22T16:00:00Z" } : null,
      last_read: READ,
      web_url: `https://jira.example/secure/RapidBoard.jspa?rapidView=${id}`,
      hidden: false,
      // prototype-6's `S.vnew`: the board that turned up at the last sync.
      new: id === 21,
      // prototype-6's `outside` (#879): tickets on the board in a project the source does not sync.
      unsynced: extra.outside ? [extra.outside] : [],
    },
  });
  VIEWS.push({ ...seeded(`jira-board-${id}`, name, "Jira boards", "", "board"), columns, lanes, saved_view: ref });
}

/** prototype-6's favourite filters (its `JF`, #878): a list each under *Jira filters*. */
function jiraFilter(id: number, name: string, jql: string, member: (key: string) => boolean, copy: QueryTranslation) {
  const ref = `jira:filter-${id}`;
  SAVED.push({
    ref,
    member,
    copy,
    source: {
      kind: "filter",
      system: "Jira",
      board_type: null,
      source_query: jql,
      sub_query: null,
      sprint: null,
      last_read: READ,
      web_url: `https://jira.example/issues/?filter=${id}`,
      hidden: false,
      new: false,
      unsynced: [],
    },
  });
  VIEWS.push({ ...seeded(`jira-filter-${id}`, name, "Jira filters", ""), saved_view: ref });
}
/** A condition, and a translation, as `knobas_source::query` spells them. */
const is = (key: QueryKey, ...values: string[]) => ({ key, negated: false, values });
const copied = (conditions: QueryTranslation["conditions"], kept: number, ...left: [string, string][]): QueryTranslation => ({
  conditions,
  kept,
  left_out: left.map(([clause, reason]) => ({ clause, reason })),
});
jiraBoard(14, "PAY board", "scrum", "project = PAY ORDER BY Rank ASC",
  [col("To Do", ["To Do"]), col("Doing", ["In Progress"]), col("Review", ["In Review"], 4), col("Done", ["Done"])],
  "epic", (key) => key.startsWith("PAY-"), copied([is("project", "PAY"), is("sprint", "current")], 2), { sprint: "Sprint 34" });
jiraBoard(21, "Payments and Ops flow", "kanban", "project in (PAY, OPS) AND labels != tech-debt ORDER BY Rank ASC",
  [col("Ready", ["To Do"]), col("In flight", ["In Progress", "In Review"], 6), col("Shipped", ["Done"])],
  "assignee", (key) => /^(PAY|OPS)-/.test(key),
  copied([is("project", "PAY", "OPS")], 1,
    ["labels != tech-debt", "knobas would also keep the tickets with no label"],
    ["fixVersion in unreleasedVersions() OR fixVersion is EMPTY", "knobas does not sync fix versions"]),
  { sub: "fixVersion in unreleasedVersions() OR fixVersion is EMPTY" });
jiraBoard(31, "Customs sprint board", "scrum", "project = CUS ORDER BY Rank ASC",
  [col("To Do", ["To Do"]), col("In Progress", ["In Progress"]), col("Done", ["Done"])],
  "none", (key) => key.startsWith("CUS-"), copied([is("project", "CUS"), is("sprint", "current")], 2), { sprint: "CUS sprint 12" });
jiraBoard(40, "Platform on-call", "kanban", "project in (OPS, WEB) AND component = on-call ORDER BY priority DESC",
  [col("Open", ["To Do"]), col("Working", ["In Progress", "In Review"], 3), col("Closed", ["Done"])],
  "priority", (key) => key.startsWith("OPS-"),
  copied([is("project", "OPS", "WEB")], 1, ["component = on-call", "knobas does not sync components"]),
  { outside: { project: "WEB", count: 9 } });
jiraBoard(52, "Tracking", "kanban", "project = TRK ORDER BY Rank ASC", [], "none", (key) => key.startsWith("TRK-"),
  copied([is("project", "TRK")], 1));
jiraFilter(10230, "My open bugs everywhere", "assignee = currentUser() AND issuetype = Bug AND resolution = Unresolved",
  (key) => keyNumber(key) % 5 === 0,
  copied([is("assignee", "me"), is("issuetype", "Bug")], 2, ["resolution = Unresolved", "knobas does not sync resolutions"]));
jiraFilter(10231, "High priority, not done", "priority = High AND statusCategory != Done ORDER BY updated DESC",
  (key) => keyNumber(key) % 3 === 0,
  copied([is("priority", "High")], 1, ["statusCategory != Done", "knobas does not sync status categories"]));
jiraFilter(10244, "Blocked in PAY", 'project = PAY AND issueFunction in linkedIssuesOf("status != Done", "is blocked by")',
  (key) => key.startsWith("PAY-") && keyNumber(key) % 7 === 0,
  copied([is("project", "PAY")], 1,
    ['issueFunction in linkedIssuesOf("status != Done", "is blocked by")', "a ScriptRunner function, only Jira can answer it"]));
jiraFilter(10251, "Resolved this week", "project in (PAY, OPS) AND resolved >= -7d",
  (key) => /^(PAY|OPS)-/.test(key) && keyNumber(key) % 4 === 0,
  copied([is("project", "PAY", "OPS")], 1, ["resolved >= -7d", "knobas has no condition for resolved"]));
// A second ticket system's one saved view (#2032): Kestrel, fixture-only (`fake/sources.ts`),
// under *Kestrel boards* as the sync names a group, so Tickets draws a source that is not Jira.
SAVED.push({
  ref: "kestrel:board-7",
  member: (key) => key.startsWith("OPS-"),
  copy: copied([is("project", "OPS")], 1),
  source: {
    kind: "board",
    system: "Kestrel",
    board_type: "kanban",
    source_query: "project: OPS, state: open",
    sub_query: null,
    sprint: null,
    last_read: READ,
    web_url: "https://kestrel.tidewater.example/boards/7",
    hidden: false,
    new: false,
    unsynced: [],
  },
});
VIEWS.push({
  ...seeded("kestrel-board-7", "Ops triage", "Kestrel boards", "", "board"),
  columns: [col("Open", ["To Do"]), col("Working", ["In Progress", "In Review"], 3), col("Closed", ["Done"])],
  saved_view: "kestrel:board-7",
});
VIEWS.forEach((view, position) => (view.position = position));

const fixtureOf = (ref: string | null) => SAVED.find((saved) => saved.ref === ref) ?? null;
const keyNumber = (key: string) => Number.parseInt(key.split("-")[1] ?? "0", 10);

interface BoardCard {
  entity_id: string;
  key: string;
}
interface Board<Card extends BoardCard> {
  columns: { status: string | null; cards: Card[]; total: number }[];
  sources: { source_id: string; statuses: string[] }[];
}

/**
 * `mini_board` with `savedView` (#877): the uncapped board `whole` cut to the
 * view's tickets, in Jira's order, under its own columns, each column by its
 * name; one per status when it has none. A restatement of
 * `commands::entity::saved_view_board`, which is the authority.
 */
export function fakeJiraBoard<Card extends BoardCard>(ref: string, whole: Board<Card>, perColumn: number | null): Board<Card> {
  const jira = fixtureOf(ref);
  const view = VIEWS.find((candidate) => candidate.saved_view === ref);
  if (!jira || !view) throw { code: "not_found", message: `no Jira view is called ${ref}`, source_id: null };
  const ordered = (cards: Card[]) => cards.filter((card) => jira.member(card.key)).sort((a, b) => keyNumber(a.key) - keyNumber(b.key));
  const cut = (status: string | null, cards: Card[]) => ({
    status,
    cards: perColumn == null ? cards : cards.slice(0, perColumn),
    total: cards.length,
  });
  const columns =
    view.columns.length === 0
      ? whole.columns.map((column) => cut(column.status, ordered(column.cards))).filter((column) => column.total > 0)
      : view.columns.map((own) =>
          cut(own.name, ordered(whole.columns.filter((column) => column.status !== null && own.statuses.includes(column.status)).flatMap((column) => column.cards))),
        );
  return { columns, sources: whole.sources };
}

/** The keys the fixture's matcher reads, and the ones it passes over as the real grammar would read them. */
const KNOWN = new Set(["issuetype", "priority", "assignee", "status", "project", "under", "label", "has", "assigned", "sprint", "type", "source"]);

function unreadable(query: string): string | null {
  const bad = [...query.matchAll(/-?([a-z]+):("[^"]*"|\S*)/gi)]
    .filter(([, key, value]) => !KNOWN.has(key!.toLowerCase()) || (key!.toLowerCase() === "sprint" && value !== "current"))
    .map(([token]) => token);
  return bad.length > 0 ? bad.join(" ") : null;
}

function slug(name: string): string {
  return name.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "") || "view";
}

/** This feature's commands. */
export function handlers(): Handlers {
  return {
    ticket_views: () =>
      VIEWS.slice()
        .sort((a, b) => a.position - b.position)
        .map((view) => {
          const jira = fixtureOf(view.saved_view);
          if (jira) {
            const count = fakeTicketKeys().filter((key) => jira.member(key)).length;
            return { ...view, count, unreadable: null, saved_view_source: jira.source };
          }
          const part = unreadable(view.query);
          return { ...view, count: part === null ? fakeTicketCount(view.query) : null, unreadable: part, saved_view_source: null };
        }),
    save_ticket_view: (args) => {
      const sent = args["view"] as Omit<View, "id" | "position"> & { id: string | null; position: number | null };
      // *View settings* on a Jira view keeps what Jira decides (#877).
      const stored = VIEWS.find((view) => view.id === sent.id && fixtureOf(view.saved_view) !== null);
      const draft = stored
        ? { ...sent, name: stored.name, group: stored.group, query: stored.query, columns: stored.columns, saved_view: stored.saved_view }
        : sent;
      if (!draft.name.trim()) throw { code: "invalid", message: "a view needs a name", source_id: null };
      let id = draft.id;
      if (id === null) {
        const base = slug(draft.name);
        id = base;
        for (let n = 2; VIEWS.some((view) => view.id === id); n += 1) id = `${base}-${n}`;
      }
      const held = VIEWS.findIndex((view) => view.id === id);
      const position = draft.position ?? (held >= 0 ? VIEWS[held]!.position : Math.max(-1, ...VIEWS.map((v) => v.position)) + 1);
      const view: View = { ...draft, id, position, name: draft.name.trim(), group: draft.group.trim() };
      if (held >= 0) VIEWS[held] = view;
      else VIEWS.push(view);
      return view;
    },
    delete_ticket_view: (args) => {
      const at = VIEWS.findIndex((view) => view.id === args["id"]);
      if (at < 0) throw { code: "not_found", message: `no ticket view is called ${String(args["id"])}`, source_id: null };
      if (fixtureOf(VIEWS[at]!.saved_view))
        throw { code: "invalid", message: `${String(args["id"])} is a Jira view: it comes from Jira at every sync and cannot be deleted here`, source_id: null };
      VIEWS.splice(at, 1);
      return null;
    },
    // #878: *Hide from the list* / *Show*, and *new* until opened.
    hide_saved_view: (args) => {
      const view = VIEWS.find((each) => each.id === args["id"]);
      if (!view) throw { code: "not_found", message: `no ticket view is called ${String(args["id"])}`, source_id: null };
      const jira = fixtureOf(view.saved_view);
      if (!jira)
        throw { code: "invalid", message: `${view.id} is not a Jira view: only a Jira view hides from the list; delete your own`, source_id: null };
      jira.source.hidden = args["hidden"] === true;
      return null;
    },
    // #1208: the Jira adapter's reading, as `translate_ticket_view` answers it.
    translate_ticket_view: (args) => {
      const view = VIEWS.find((each) => each.id === args["id"]);
      if (!view) throw { code: "not_found", message: `no ticket view is called ${String(args["id"])}`, source_id: null };
      const jira = fixtureOf(view.saved_view);
      if (!jira) throw { code: "invalid", message: `${view.id} is a view of your own`, source_id: null };
      return jira.copy;
    },
    open_saved_view: (args) => {
      const view = VIEWS.find((each) => each.id === args["id"]);
      if (!view) throw { code: "not_found", message: `no ticket view is called ${String(args["id"])}`, source_id: null };
      const jira = fixtureOf(view.saved_view);
      if (jira) jira.source.new = false;
      return null;
    },
  };
}

keepSeed(VIEWS, SAVED);
