/** *Boards and filters*' pure rules (#878): the stored choice, the source's views, their words. */
import { expect, test } from "vitest";

import type { SavedViewSource, TicketViewRow } from "../../ipc/search";
import type { KindPaths, SourceDescriptor } from "../../ipc/sources";
import { boardScope, favouriteFilters, savedViewsOf, savedViewSystem, unsyncedWords, viewType } from "./boards";

function savedViewSource(extra: Partial<SavedViewSource> = {}): SavedViewSource {
  return {
    kind: "board",
    system: "Jira",
    board_type: "kanban",
    source_query: "",
    sub_query: null,
    sprint: null,
    last_read: "",
    web_url: null,
    hidden: false,
    new: false,
    unsynced: [],
    ...extra,
  };
}

function view(id: string, position: number, ref: string | null, source: SavedViewSource | null): TicketViewRow {
  return {
    id,
    name: id,
    group: "Jira boards",
    position,
    query: "",
    layout: "board",
    columns: [],
    lanes: "none",
    card_fields: [],
    list_columns: ["key"],
    list_sort: null,
    saved_view: ref,
    count: 0,
    unreadable: null,
    saved_view_source: source,
  };
}

test("the stored choice, with D17's default where the config says nothing it knows", () => {
  expect(boardScope({})).toBe("synced");
  expect(boardScope({ boards: "all" })).toBe("all");
  expect(boardScope({ boards: "every" })).toBe("synced");
  expect(boardScope(null)).toBe("synced");
  expect(favouriteFilters({})).toBe(true);
  expect(favouriteFilters({ favourite_filters: false })).toBe(false);
});

test("a source's Jira views: its own only, boards before filters, each in list order", () => {
  const views = [
    view("filter", 0, "jira:filter-1", savedViewSource({ kind: "filter", board_type: null })),
    view("other", 1, "jira-eu:board-3", savedViewSource()),
    view("mine", 2, null, null),
    view("late", 5, "jira:board-9", savedViewSource()),
    view("early", 3, "jira:board-2", savedViewSource()),
  ];
  expect(savedViewsOf(views, "jira").map((row) => row.id)).toEqual(["early", "late", "filter"]);
});

test("a view's type and the tickets on it knobas does not mirror, in words", () => {
  expect(viewType(savedViewSource({ board_type: "scrum" }))).toBe("Scrum board");
  expect(viewType(savedViewSource({ board_type: "kanban" }))).toBe("Kanban board");
  expect(viewType(savedViewSource({ board_type: "simple" }))).toBe("Board");
  expect(viewType(savedViewSource({ kind: "filter", board_type: null }))).toBe("Filter");
  expect(unsyncedWords([])).toBeNull();
  expect(unsyncedWords([{ project: "WEB", count: 3 }])).toBe("and 3 in WEB, not synced");
  expect(
    unsyncedWords([
      { project: "WEB", count: 3 },
      { project: "MOB", count: 1 },
    ]),
  ).toBe("and 3 in WEB, 1 in MOB, not synced");
});

test("the ticket system a source's views come from is its saved-view kind's declared name, else the adapter's (#2032)", () => {
  const kind = (id: string) => ({ id, label: id, plural: id, monogram: "JV", full_sync_exhaustive: false });
  const adapter = (paths: Partial<KindPaths>[], kinds = ["ticket", "jira_view"]) =>
    ({ name: "Jira Data Center", entity_kinds: kinds.map(kind), payload_paths: paths as KindPaths[] }) as unknown as SourceDescriptor;
  expect(savedViewSystem(adapter([{ kind: "ticket", frame: null }, { kind: "jira_view", frame: "saved_view", system_name: "Jira" }]))).toBe("Jira");
  expect(savedViewSystem(adapter([{ kind: "jira_view", frame: "saved_view", system_name: null }]))).toBe("Jira Data Center");
  // A declaration on a kind the adapter does not emit is no declaration, as the sync reads it.
  expect(savedViewSystem(adapter([{ kind: "jira_view", frame: "saved_view", system_name: "Jira" }], ["ticket"]))).toBe("Jira Data Center");
  expect(savedViewSystem(adapter([]))).toBe("Jira Data Center");
});
