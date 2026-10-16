/**
 * `?fake-ipc`: Ticket views (#812): `ticket_views`, `save_ticket_view` and
 * `delete_ticket_view`, over a session-long list seeded with prototype-6's own
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
import type { Handler } from "../fake-tauri";
import { fakeTicketCount } from "./corpus";

interface View {
  id: string;
  name: string;
  group: string;
  position: number;
  query: string;
  layout: "board" | "list";
  columns: { name: string; statuses: string[]; limit: number | null }[];
  lanes: string;
  card_fields: string[];
  list_columns: string[];
  list_sort: { column: string; direction: string } | null;
  jira_ref: string | null;
}

const CARD = ["type", "signals", "comments", "assignee"];
const LIST = ["key", "summary", "status", "priority", "signals", "assignee", "updated"];

function seeded(id: string, name: string, group: string, query: string, layout: "board" | "list" = "list"): View {
  return { id, name, group, position: 0, query, layout, columns: [], lanes: "none", card_fields: CARD, list_columns: LIST, list_sort: null, jira_ref: null };
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
].map((view, position) => ({ ...view, position }));

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
export function handlers(): Record<string, Handler> {
  return {
    ticket_views: () =>
      VIEWS.slice()
        .sort((a, b) => a.position - b.position)
        .map((view) => {
          const part = unreadable(view.query);
          return { ...view, count: part === null ? fakeTicketCount(view.query) : null, unreadable: part };
        }),
    save_ticket_view: (args) => {
      const draft = args["view"] as Omit<View, "id" | "position"> & { id: string | null; position: number | null };
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
      VIEWS.splice(at, 1);
      return null;
    },
  };
}
