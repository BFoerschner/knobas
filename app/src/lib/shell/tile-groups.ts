/**
 * The Code tile's groups (#712): what needs me first.
 *
 * The backend decides which group a row is in and sends the groups' counts in
 * draw order (`EntityPage.groups`), with the rows arriving group by group. A
 * tile reads from offset 0, so its page is a prefix of that order and is cut
 * into its groups by the counts alone; the last group may be cut short by the
 * page's length, never reordered.
 *
 * Other branches are drawn only on the maximised tile: the grid's tile is for
 * what needs me, and every branch of the repo is not that.
 */
import type {
  AttentionGroup,
  EntityPage,
  EntityRow,
  GroupTally,
} from "../ipc/entity";

/** One drawn group: its header and the page's rows under it. */
export interface TileGroup {
  group: AttentionGroup;
  label: string;
  /** The whole set's count for this group, which can exceed `rows`. */
  count: number;
  rows: EntityRow[];
}

/** What each group's header says. */
const LABEL: Record<AttentionGroup, string> = {
  review_requested: "Review requested",
  my_pull_requests: "My pull requests",
  open_pull_requests: "Other open pull requests",
  my_branches: "My branches without a pull request",
  branches: "Branches",
  other_branches: "Other branches",
};

/** The groups drawn only on the maximised tile. */
const MAXIMISED_ONLY: ReadonlySet<AttentionGroup> = new Set(["other_branches"]);

/**
 * The groups a page draws, or `null` when the page is not grouped (no
 * identity, or another order) and the tile draws one list as before. A group
 * with no row is not drawn.
 */
export function tileGroups(
  page: Pick<EntityPage, "rows" | "groups">,
  maximised: boolean,
): TileGroup[] | null {
  if (!page.groups) return null;
  const drawn: TileGroup[] = [];
  let at = 0;
  for (const { group, count } of page.groups) {
    const rows = page.rows.slice(at, at + count);
    at += count;
    if (rows.length === 0 || (!maximised && MAXIMISED_ONLY.has(group)))
      continue;
    drawn.push({ group, label: LABEL[group], count, rows });
  }
  return drawn;
}

/** How many rows the grid's tile leaves for the maximised one. */
export function hiddenCount(
  groups: GroupTally[] | null,
  maximised: boolean,
): number {
  if (maximised || !groups) return 0;
  return groups
    .filter((line) => MAXIMISED_ONLY.has(line.group))
    .reduce((all, line) => all + line.count, 0);
}
