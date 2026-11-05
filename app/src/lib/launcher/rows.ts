/**
 * What `↑`/`↓` move over, as one flat list.
 *
 * The launcher draws four different panels — results in groups, the board, the
 * `>` palette, the `?` card — and every one of them is a list the arrow keys
 * walk and `Enter` opens. Flattening them to one array is what stops each
 * panel inventing its own selection model, and it is why a group heading is
 * *not* in here: a heading is not something you can open.
 *
 * Pure, and separate from the session that owns the data, so the selection
 * rules can be tested without a fake IPC bridge.
 */
import type { LauncherHome, ResultGroup, SearchHit, SearchResponse } from "../ipc";
import type { EntityRow, InboxItem } from "../ipc/entity";
import { type LauncherAction, matchingPlaces } from "./actions";
import type { SyntaxEntry } from "./syntax";

/**
 * The shell's live inbox (#1418): the count the top bar reads and the stream's
 * items, newest first. Handed in by the shell rather than carried on
 * `launcher_home`; the empty box no longer draws it (#2421), and `#i`
 * answers from it (#2423).
 */
export interface InboxPreview {
  count: number;
  items: InboxItem[];
}

/** What the shell hands in beside `LauncherHome`. */
export interface BoardExtras {
  inbox: InboxPreview | null;
  /**
   * The app's places, which a search with no filter offers by name above its
   * answer. The launcher's; a link picker built on the session has none, since
   * it is choosing a target and not going anywhere.
   */
  places?: readonly LauncherAction[];
  /** Whether an empty answer offers *Search everything*: the launcher's, as `places` is. */
  widens?: boolean;
}

/** One selectable line. */
export type LauncherRow =
  | { kind: "hit"; id: string; group: ResultGroup; hit: SearchHit }
  | { kind: "recent"; id: string; row: EntityRow }
  /**
   * One of the board's favourites (#2428): an entity, or a saved list drawn
   * as its `#l` hit is (kind and source `list`, its text as `path`).
   */
  | { kind: "favourite"; id: string; row: EntityRow }
  /** A line of the shell's inbox, behind `#i` (#2423): not an entity, a row of the inbox store. */
  | { kind: "inbox"; id: string; item: InboxItem }
  | { kind: "action"; id: string; action: LauncherAction }
  | { kind: "syntax"; id: string; entry: SyntaxEntry }
  /** *Search Docs for …* at the end of a `#d` answer (#2425): the Docs area, searching `words`. */
  | { kind: "docs"; id: string; words: string }
  /** *Search everything* under an empty answer: the same `words`, with the filters gone and every field searched. */
  | { kind: "widen"; id: string; words: string };

/**
 * Which panel the body draws.
 *
 * `"url"` is the one the *session* decides rather than `modeOf`: the box holds
 * an absolute URL, so what is on screen is a link being resolved, not a query
 * being answered (#496). It has no selectable rows — a hit navigates on its
 * own and a miss offers one button — so `flatten` returns nothing for it.
 */
export type LauncherMode = "board" | "results" | "actions" | "help" | "url";

/**
 * The panel a state is in.
 *
 * **Read off the backend's own interpretation**, never off the raw text
 * (ruling P2: the backend parses, the frontend renders what it was told). An
 * empty box is the one case the frontend decides for itself, and "is this
 * string empty" is not grammar.
 *
 * The consequence, which is a feature: typing `?` shows the card once the
 * response lands rather than on the keystroke. The alternative is a second
 * copy of the prefix table in TypeScript, drifting from `query.rs` the first
 * time either changes — which is exactly the failure the `?` card itself is
 * built to avoid.
 */
export function modeOf(raw: string, response: SearchResponse | null): LauncherMode {
  if (raw.trim() === "") return "board";
  switch (response?.interpreted.prefix) {
    case "help":
      return "help";
    case "action":
      return "actions";
    default:
      return "results";
  }
}

/** The selectable rows of one panel, in the order they are drawn. */
export function flatten(input: {
  mode: LauncherMode;
  response: SearchResponse | null;
  home: LauncherHome | null;
  actions: LauncherAction[];
  syntax: readonly SyntaxEntry[];
  extras?: BoardExtras;
  /** *Titles* rather than *Everything* (the query's `titles` flag): `#i` matches a line's title alone. */
  titles?: boolean;
  /** The box's text, which says whether anything narrows the search beyond its words. */
  raw?: string;
}): LauncherRow[] {
  switch (input.mode) {
    // A pasted link is not a list: the entity it names is opened, and a link
    // the mirror does not hold offers *Open in browser* and nothing to walk.
    case "url":
      return [];
    case "help":
      return input.syntax.map((entry) => ({
        kind: "syntax",
        id: `syntax:${entry.token}`,
        entry,
      }));
    case "actions":
      return input.actions.map((action) => ({
        kind: "action",
        id: `action:${action.id}`,
        action,
      }));
    case "board": {
      const home = input.home;
      if (!home) return [];
      // The favourites in their order (#2428), then the recent items
      // (#2421): no smart lists, no inbox, no time. A favourite that is also
      // recent is in both, under its own id in each.
      return [
        ...home.favourites.map(
          (row): LauncherRow => ({ kind: "favourite", id: `favourite:${row.kind === "list" ? "list" : "entity"}:${row.entity_id}`, row }),
        ),
        ...home.recent.map((row): LauncherRow => ({ kind: "recent", id: `recent:${row.entity_id}`, row })),
      ];
    }
    case "results": {
      const response = input.response;
      if (!response) return [];
      const hits = ticketsFirst(response).flatMap((group) =>
        group.hits.map(
          (hit): LauncherRow => ({
            kind: "hit",
            id: `hit:${group.kind}:${hit.entity_id}`,
            group,
            hit,
          }),
        ),
      );
      const answer = [...hits, ...inboxRows(response, input.extras?.inbox ?? null, input.titles ?? false), ...docsRow(response)];
      const words = response.interpreted.text;
      // Filters or a prefix mean the reader is narrowing a search, not naming a place.
      const filtered = (input.raw ?? words).trim() !== words.trim();
      const places = filtered ? [] : placeRows(input.extras?.places ?? [], words, answer.length === 0);
      const widen = answer.length === 0 && input.extras?.widens === true && (filtered || input.titles === true);
      return [...places, ...answer, ...(widen ? [{ kind: "widen", id: "widen", words } as const] : [])];
    }
  }
}

/** The *Go to* rows over an answer: by name, and by what a place holds where nothing else answered. */
function placeRows(places: readonly LauncherAction[], words: string, nothingElse: boolean): LauncherRow[] {
  return matchingPlaces(places, words, nothingElse).map((action) => ({ kind: "action", id: `action:${action.id}`, action }));
}

/** Whether `rows` hold an answer, as against only the rows that lead elsewhere (*Go to*, *Search everything*). */
export function holdsAnswer(rows: readonly LauncherRow[]): boolean {
  return rows.some((row) => row.kind !== "action" && row.kind !== "widen");
}

/**
 * What a query that looks like a key is after is a ticket: *PAY* or *PAY-23*
 * lists the tickets first, ahead of the saved views that happen to hold the
 * letters. Any other query keeps the engine's order.
 */
export function ticketsFirst(response: SearchResponse): SearchResponse["groups"] {
  if (!/^[A-Z][A-Z0-9]*(-\d*)?$/i.test(response.interpreted.text.trim())) return response.groups;
  return [...response.groups].sort((a, b) => Number(b.kind === "ticket") - Number(a.kind === "ticket"));
}

/**
 * The inbox's lines (#2423, spec #2419 story 15): an answer the engine read
 * as `#i` (kind `inbox`, which nothing in the index emits) draws the shell's
 * inbox store, newest first, narrowed by the parse's words. A line is kept
 * when every word is in its title, ignoring case; under *Everything*
 * (`titles` off) its reason and its category count too, as a body does
 * (story 51): `#i review` finds the review requests either way. The kind is
 * read off the backend's parse, never the box (ruling P2); the words are the
 * store's to match, since an inbox line is no entity the index holds (ruled
 * on the review of spec #2419, S4).
 */
function inboxRows(response: SearchResponse, inbox: InboxPreview | null, titles: boolean): LauncherRow[] {
  if (!inbox || !response.interpreted.filters.kinds.includes("inbox")) return [];
  const words = response.interpreted.text.toLowerCase().split(/\s+/u).filter(Boolean);
  const holds = (item: InboxItem) => {
    const text = (titles ? item.title : `${item.title} ${item.reason} ${item.category.replaceAll("_", " ")}`).toLowerCase();
    return words.every((word) => text.includes(word));
  };
  return inbox.items
    .filter(holds)
    .slice()
    .sort((a, b) => Date.parse(b.occurred_at) - Date.parse(a.occurred_at))
    .map((item) => ({ kind: "inbox", id: `inbox:${item.key}`, item }));
}

/**
 * The *Search Docs for …* row (#2425): last in an answer the engine read as
 * `#d` (kind `page`) with words, whose search the Docs area runs with its own
 * outline and hits. Read off the backend's parse, never the box (ruling P2).
 */
function docsRow(response: SearchResponse): LauncherRow[] {
  const words = response.interpreted.text.trim();
  if (words === "" || !response.interpreted.filters.kinds.includes("page")) return [];
  return [{ kind: "docs", id: "docs", words }];
}

/** ↓ is one row on, ↑ one back; any other key `0`. */
export function stepOf(event: Pick<KeyboardEvent, "key">): -1 | 0 | 1 {
  return event.key === "ArrowDown" ? 1 : event.key === "ArrowUp" ? -1 : 0;
}

/**
 * ↓ or ↑ over a list of `count` rows from `selected`: the row it lands on,
 * held at the ends, the key's default prevented; any other key `null`. The
 * one walk the box's panels share (the suggestions, the overlay, the sheet
 * and its flows; the results walk through `session.move`).
 */
export function walk(event: Pick<KeyboardEvent, "key" | "preventDefault">, selected: number, count: number): number | null {
  const step = stepOf(event);
  if (step === 0) return null;
  event.preventDefault();
  return step === 1 ? Math.min(count - 1, selected + 1) : Math.max(0, selected - 1);
}
