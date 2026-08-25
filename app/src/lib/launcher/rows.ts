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
import type { EntityRow } from "../ipc/entity";
import type { LauncherAction } from "./actions";
import type { SyntaxEntry } from "./syntax";

/** One selectable line. */
export type LauncherRow =
  | { kind: "hit"; id: string; group: ResultGroup; hit: SearchHit }
  | { kind: "recent"; id: string; row: EntityRow }
  | { kind: "list"; id: string; list: SmartListSummary }
  | { kind: "action"; id: string; action: LauncherAction }
  | { kind: "syntax"; id: string; entry: SyntaxEntry };

/** Which panel the body draws. */
export type LauncherMode = "board" | "results" | "actions" | "help";

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
}): LauncherRow[] {
  switch (input.mode) {
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
      return [
        ...home.smart_lists.map(
          (list): LauncherRow => ({ kind: "list", id: `list:${list.id}`, list }),
        ),
        ...home.recent.map(
          (row): LauncherRow => ({ kind: "recent", id: `recent:${row.entity_id}`, row }),
        ),
      ];
    }
    case "results": {
      const response = input.response;
      if (!response) return [];
      return response.groups.flatMap((group) =>
        group.hits.map(
          (hit): LauncherRow => ({
            kind: "hit",
            id: `hit:${group.kind}:${hit.entity_id}`,
            group,
            hit,
          }),
        ),
      );
    }
  }
}
