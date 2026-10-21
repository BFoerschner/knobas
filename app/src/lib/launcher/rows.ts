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
import type { DayFigures } from "../tf/time/day-view";
import type { LauncherAction } from "./actions";
import type { SyntaxEntry } from "./syntax";

/**
 * The inbox as the board previews it (#1418): the count the top bar reads and
 * the stream's newest items, newest first. The shell's live inbox store is the
 * one source of both; the board shows at most {@link INBOX_PREVIEW} of them.
 */
export interface InboxPreview {
  count: number;
  items: InboxItem[];
}

export const INBOX_PREVIEW = 3;

/** Today's time as the board gives it (#1418): two of the Day view's figures, in whole minutes. */
export type TodayTime = Pick<DayFigures, "tracked" | "toLog">;

/**
 * What the board draws beside `LauncherHome`, handed in by the shell rather
 * than carried on `launcher_home` (#1418): both already have a live read in
 * the shell, and a field on the frozen board type would be a second one.
 */
export interface BoardExtras {
  inbox: InboxPreview | null;
  today: TodayTime | null;
}

/** One selectable line. */
export type LauncherRow =
  | { kind: "hit"; id: string; group: ResultGroup; hit: SearchHit }
  | { kind: "recent"; id: string; row: EntityRow }
  | { kind: "list"; id: string; list: SmartListSummary }
  | { kind: "inbox"; id: string; item: InboxItem }
  | { kind: "today"; id: string; today: TodayTime }
  | { kind: "action"; id: string; action: LauncherAction }
  | { kind: "syntax"; id: string; entry: SyntaxEntry };

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
      // In the order the board draws them (spec §4): the lists, the inbox,
      // today's time, the recent items. A block with nothing to show has no
      // rows, so the board draws no heading over nothing.
      const inbox = input.extras?.inbox;
      const today = input.extras?.today;
      return [
        ...home.smart_lists.map(
          (list): LauncherRow => ({ kind: "list", id: `list:${list.id}`, list }),
        ),
        ...(inbox && inbox.count > 0 ? inbox.items.slice(0, INBOX_PREVIEW) : []).map(
          (item): LauncherRow => ({ kind: "inbox", id: `inbox:${item.key}`, item }),
        ),
        ...(today && today.tracked > 0 ? [{ kind: "today" as const, id: "today", today }] : []),
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
