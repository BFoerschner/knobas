/**
 * The Tree's column arithmetic — pure, so it can be read and tested without a
 * database or a window.
 *
 * `CONTEXT.md`, **Tree**: *the Assets view's first tab: the estate as Miller
 * columns, one column per level*. A Miller layout is fully described by two
 * aligned lists — which parent each column lists, and which row is selected in
 * it — and everything else the view does is fetching and drawing. That
 * arithmetic is here.
 *
 * **The path comes from the backend, not from a cache.** `AssetDetail.held_by`
 * is the ancestors of the selected asset, outermost first, computed by one
 * recursive read. Deriving the columns from it means the Tree is correct the
 * first time it is opened at a deep address, with nothing walked and nothing
 * remembered — which is what #430's *addresses re-open at the path* will build
 * on and what a client-side tree cache could not have given it.
 *
 * The spine collapse of story 28 and the keyboard walk of story 29 are #430's;
 * this module is the shape they will collapse and walk.
 */
import type { AssetDetail, AssetRow, AssetStatus, Inherited } from "../ipc/assets";
import { hashFor } from "../shell/router.svelte";

/** The top of the estate: the column whose parent is nobody. */
export const TOP: null = null;

/** One Miller layout. The two lists are always the same length. */
export interface ColumnPath {
  /**
   * The parent each column lists, left to right. `null` is the estate's top
   * level, so `parents[0]` is always `null`.
   */
  parents: (string | null)[];
  /**
   * The asset selected in each column, aligned with {@link parents}. `null`
   * means nothing in that column is chosen — which is only ever the last one.
   */
  selected: (string | null)[];
}

/** Nothing selected: one column, the top of the estate. */
export function emptyPath(): ColumnPath {
  return { parents: [TOP], selected: [null] };
}

/**
 * The columns to draw for a selected asset.
 *
 * One column per ancestor plus one for the asset itself, and **one more only
 * when the asset holds something**: an empty trailing column is a promise of a
 * level that is not there, and the reader has already been told by the absence
 * of a chevron.
 *
 * `null` — nothing selected — is the estate's top level and one column.
 */
export function columnPathFor(detail: AssetDetail | null): ColumnPath {
  if (detail === null) return emptyPath();

  const ancestors = detail.held_by.map((held) => held.id);
  const parents: (string | null)[] = [TOP, ...ancestors];
  const selected: (string | null)[] = [...ancestors, detail.asset.id];

  if (detail.asset.has_children) {
    parents.push(detail.asset.id);
    selected.push(null);
  }
  return { parents, selected };
}

/**
 * The held-by line the pane draws: the ancestors' names and then the asset's
 * own, outermost first.
 *
 * The asset is included, and that is the difference between this and
 * `held_by`: the pane's line is *where this thing is*, which ends at the thing.
 * `held_by` is *what holds it*, which does not.
 */
export function heldByPath(detail: AssetDetail): string[] {
  return [...detail.held_by.map((held) => held.name), detail.asset.name];
}

/**
 * Which asset a column's row should read as selected.
 *
 * A helper rather than an inline comparison so the view has one answer: a
 * column past the end of the path has nothing selected, which is the state a
 * freshly opened column is in while its read is still out.
 */
export function selectionIn(path: ColumnPath, column: number): string | null {
  return path.selected[column] ?? null;
}

/**
 * Where an asset's address lives — `#/asset/<id>`, spec §2.
 *
 * A named helper because the Tree builds one of these for every row it draws,
 * including the ones nobody has selected; it **delegates to `hashFor`**, so
 * there is still exactly one encoder and a link from a ticket and a click in a
 * column cannot end up in different places.
 */
export function addressOf(row: Pick<AssetRow, "id">): string {
  return hashFor({ view: "assets", tab: "tree", assetId: row.id });
}

/**
 * The "N problems inside" badge for one column row — story 32, issue #431.
 *
 * `null` is **no badge**, and it is the answer for every row holding nothing
 * wrong. A badge reading zero would be a mark the eye stops on to learn there
 * is nothing to learn, which is the opposite of what a badge on a *closed*
 * branch is for.
 *
 * The tone comes from `inside` and not from `health`, and the difference shows
 * the moment an asset is worse than what it holds: a `down` VM holding one
 * `warn` container is a red row with an **amber** badge, because the badge
 * counts and colours what is inside it. `inside` can only be `"warn"` or
 * `"down"` when the count is above zero — the backend's count is over exactly
 * those two statuses — so the fallback below is unreachable rather than a
 * third tone.
 */
export interface ProblemBadge {
  /** How many descendants carry `warn` or `down`. Always above zero. */
  count: number;
  /** Amber or red. */
  tone: Extract<AssetStatus, "warn" | "down">;
}

export function problemBadge(row: AssetRow): ProblemBadge | null {
  if (row.problems_inside <= 0) return null;
  return { count: row.problems_inside, tone: row.inside === "down" ? "down" : "warn" };
}

/**
 * Where a value in force on the asset comes from — story 10.
 *
 * One answer to the pane's whole question, rather than a boolean the view then
 * re-derives a sentence and a link from: *is it set here*, *what do I write
 * beside the value*, and *where does a click go*. `goTo` is `null` exactly
 * when the value is set here, so there is never a link that leads back to the
 * asset the reader is already looking at.
 */
export interface ValueSource {
  here: boolean;
  note: string;
  goTo: string | null;
}

export function sourceOf(detail: AssetDetail, from: Inherited<unknown>): ValueSource {
  const here = from.source_id === detail.asset.id;
  return {
    here,
    note: here ? "set here" : `inherited from ${from.source_name}`,
    goTo: here ? null : addressOf({ id: from.source_id }),
  };
}
