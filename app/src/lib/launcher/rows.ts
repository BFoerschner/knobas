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
import type { LauncherHome, ResultGroup, SearchHit, SearchResponse, SmartListSummary } from "../ipc";
import type { EntityRow, InboxItem } from "../ipc/entity";
import type { LauncherAction } from "./actions";
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
}

/** One selectable line. */
export type LauncherRow =
  | { kind: "hit"; id: string; group: ResultGroup; hit: SearchHit }
  | { kind: "recent"; id: string; row: EntityRow }
  | { kind: "list"; id: string; list: SmartListSummary }
  | { kind: "inbox"; id: string; item: InboxItem }
  | { kind: "action"; id: string; action: LauncherAction }
  | { kind: "syntax"; id: string; entry: SyntaxEntry }
  /** *Search Docs for …* at the end of a `#d` answer (#2425): the Docs area, searching `words`. */
  | { kind: "docs"; id: string; words: string };

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
      // The recent items alone (#2421): no lists, no inbox, no time.
      return home.recent.map(
        (row): LauncherRow => ({ kind: "recent", id: `recent:${row.entity_id}`, row }),
      );
    }
    case "results": {
      const response = input.response;
      if (!response) return [];
      const hits = response.groups.flatMap((group) =>
        group.hits.map(
          (hit): LauncherRow => ({
            kind: "hit",
            id: `hit:${group.kind}:${hit.entity_id}`,
            group,
            hit,
          }),
        ),
      );
      return [...hits, ...docsRow(response)];
    }
  }
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
