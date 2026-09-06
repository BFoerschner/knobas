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
 * remembered — which is what #430's *addresses re-open at the path* builds on
 * and what a client-side tree cache could not have given it.
 *
 * **Everything #430 added is the same arithmetic on the same layout**, which
 * is why it is here and not in a module of its own: {@link walk} says which
 * asset a key press selects, {@link stripFor} says which columns collapse to
 * spines, and {@link estateQuery}/{@link matchesIn} say what the search box
 * asks for and which of the answers are the estate's. None of the three
 * touches a window or a bridge.
 */
import { type SearchQuery, type SearchResponse, noFilters } from "../ipc";
import type { AssetDetail, AssetRow, AssetStatus, Inherited, RouteRow } from "../ipc/assets";
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
 * Where a route's address lives — `#/route/<id>`, spec §2 (#432).
 *
 * {@link addressOf}'s twin, and it delegates to `hashFor` for the same reason:
 * one encoder, so a link pasted into a ticket and a copy taken from the pane
 * are the same string.
 */
export function routeAddressOf(route: Pick<RouteRow, "id">): string {
  return hashFor({ view: "assets", tab: "tree", assetId: null, routeId: route.id });
}

/**
 * How a route reaches the asset the pane is showing — story 31 in words.
 *
 * The backend answers *which* routes reach an asset and where each one lands
 * ({@link AssetDetail.reachable_via}); this is the one place that reads the
 * three cases off a row, so the pane's sentence and any wire drawn later
 * cannot disagree about which is which:
 *
 * * **here** — the route's target is this asset. Nothing to link to: the
 *   reader is already looking at it, which is {@link sourceOf}'s rule for the
 *   same situation.
 * * **through an ancestor** — the target is on the held-by path, so the route
 *   arrives at something that *holds* this asset. Story 31's dashed wire.
 * * **inside** — the target is neither, which by the read's own construction
 *   leaves one possibility: something this asset holds. A URL landing on a
 *   container is how the VM running it is reached, and naming the container is
 *   what makes that sentence checkable.
 */
export interface Landing {
  /** What the pane writes beside the route. */
  note: string;
  /**
   * Where a click on that note goes, or `null` for the two cases with nowhere
   * to send the reader: the route lands *here*, or on nothing at all.
   *
   * No `here` flag beside it, unlike {@link ValueSource}, and the difference
   * is that this one has no caller for it: both the sentence and the link are
   * answered above, so a boolean saying the same thing a third way would be a
   * field nothing reads.
   */
  goTo: string | null;
}

export function landingOf(detail: AssetDetail, route: RouteRow): Landing {
  if (route.target_id === null) {
    // Not reachable from this list — a route with no target reaches nobody —
    // so this is the total function's honest answer rather than a case the
    // pane draws.
    return { note: "lands on nothing", goTo: null };
  }
  if (route.target_id === detail.asset.id) return { note: "lands here", goTo: null };
  const above = detail.held_by.some((held) => held.id === route.target_id);
  const name = route.target_name ?? route.target_id;
  return {
    note: above ? `through ${name}` : `inside, on ${name}`,
    goTo: addressOf({ id: route.target_id }),
  };
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

/**
 * Which column the cursor is in: the last one with something selected.
 *
 * `-1` — nothing selected anywhere — is the bare `#/assets/tree`, and it is a
 * *position* rather than an error: the reader is standing in front of the top
 * column with no row chosen, which is what the first arrow press acts on.
 */
export function focusedColumn(path: ColumnPath): number {
  let found = -1;
  for (let column = 0; column < path.selected.length; column += 1) {
    if (path.selected[column] !== null) found = column;
  }
  return found;
}

/**
 * What one key press does to the walk (story 29).
 *
 * Three outcomes and no fourth: select an asset, clear the selection back to
 * the estate's top, or do nothing. `nowhere` is a value rather than a silence
 * because the view has to know whether to **consume** the key — an Escape the
 * Tree did not answer belongs to the shell's ladder (`shell/keys.ts`), which
 * takes it back to the room, and a Tree that swallowed every Escape would be
 * the one view a reader cannot leave with the keyboard.
 */
export type Step = { go: "asset"; id: string } | { go: "top" } | { go: "nowhere" };

/** The press did nothing here. */
const NOWHERE: Step = { go: "nowhere" };

/**
 * The keyboard walk: a key, the layout, and what each column holds, in — the
 * asset to select, out.
 *
 * **It answers with an asset and not with a column index**, which is what
 * keeps the walk on the same rails as a click: the view turns a step into an
 * *address*, the address is read back, and the columns follow the answer
 * ({@link columnPathFor}). A walk that moved a cursor of its own would be a
 * second owner of the selection, and the two would disagree the first time a
 * read was slow.
 *
 * The ladder, in the mockup's order (`E1-miller-columns.html:857-874`):
 *
 * | Key            | What it means                                          |
 * |----------------|--------------------------------------------------------|
 * | `↓` / `↑`      | the next / previous row of the column the cursor is in  |
 * | `→` / `Enter`  | the first row of the column under the selection         |
 * | `←`            | the asset that holds the selection                      |
 * | `Esc`          | the same one step, and the top of the estate after that |
 *
 * Neither list wraps. A cursor that jumps from the last row back to the first
 * loses the reader — `Session.move`'s rule in the launcher, for its reason.
 *
 * @param key `KeyboardEvent.key`.
 * @param path the layout, from {@link columnPathFor}.
 * @param columns what each column holds, aligned with `path.parents`.
 */
export function walk(key: string, path: ColumnPath, columns: AssetRow[][]): Step {
  const focused = focusedColumn(path);

  if (key === "ArrowDown" || key === "ArrowUp") {
    const column = Math.max(0, focused);
    const rows = columns[column] ?? [];
    if (rows.length === 0) return NOWHERE;
    const down = key === "ArrowDown";
    // A column with no row chosen is entered from the end the key comes from:
    // `↓` lands on the first row, `↑` on the last.
    const at = rows.findIndex((row) => row.id === selectionIn(path, column));
    const from = at < 0 ? (down ? -1 : rows.length) : at;
    const to = Math.min(rows.length - 1, Math.max(0, from + (down ? 1 : -1)));
    if (to === at) return NOWHERE;
    return { go: "asset", id: rows[to]!.id };
  }

  if (key === "ArrowRight" || key === "Enter") {
    // `focused + 1` reads the same on a bare tree, where the cursor is in
    // front of the top column and the column "under the selection" is it.
    const rows = columns[focused + 1] ?? [];
    const first = rows[0];
    if (first === undefined) return NOWHERE;
    return { go: "asset", id: first.id };
  }

  if (key === "ArrowLeft" || key === "Escape") {
    if (focused > 0) {
      const holder = selectionIn(path, focused - 1);
      return holder === null ? NOWHERE : { go: "asset", id: holder };
    }
    // The rung the two keys do not share. `←` is a move between columns and
    // there is no column left of the first; `Esc` is an unwind, and the step
    // above a top-level asset is the estate with nothing selected.
    return key === "Escape" && focused === 0 ? { go: "top" } : NOWHERE;
  }

  return NOWHERE;
}

/**
 * How wide the Tree draws one full column, in pixels — `.colw`'s own width.
 *
 * A copy of a CSS number, and the one place it is copied. The collapse is a
 * question about *layout* — does the next column still fit beside the fixed
 * pane — and the only honest way to answer it in a pure function is with the
 * width the stylesheet uses. `AssetsView.svelte` sets `.colw` and `.spine` from
 * these two constants' values and says so beside them. It is the *wrapper*
 * and not the `ol` inside it because #429 gave the column a header: what the
 * strip lays out is the wrapper, and the rows scroll under it.
 */
export const COLUMN_WIDTH = 220;

/** How wide a collapsed column is drawn — `.spine`'s width. */
export const SPINE_WIDTH = 30;

/**
 * Which columns are drawn in full. Everything outside the window is a spine.
 *
 * Half-open: `from` is the first full column and `to` is one past the last,
 * so `to - from` is how many fit and the empty case cannot be spelled.
 */
export interface Strip {
  from: number;
  to: number;
}

/**
 * The spine collapse (story 28): *older columns collapse into labelled spines
 * as the path deepens, so the pane never leaves the screen.*
 *
 * The rule is the mockup's (`E1-miller-columns.html:571-577`): shrink the
 * number of full columns until the strip fits, counting the collapsed ones at
 * a spine's width, and keep the **newest** — the ones the reader just walked
 * into. One column is always full: a strip of nothing but spines would be a
 * Tree with nowhere to stand.
 *
 * It is **width-driven and not depth-driven**, which is what story 28 asks
 * for and is why there is no "collapse past level four" constant. Where the
 * collapse begins therefore depends on the window: at knobas' narrowest
 * (`minWidth: 1100`, about 780px of strip beside the pane) the fourth column
 * collapses, and at the default 1280 the fifth does.
 *
 * @param columns how many columns the layout has.
 * @param available the strip's width in pixels; `0` where nothing has been
 *   measured yet, which collapses nothing.
 * @param anchor the column a reader clicked a spine to re-expand, or `null`
 *   for the deepest window — clamped, never trusted.
 */
export function stripFor(columns: number, available: number, anchor: number | null): Strip {
  if (columns <= 0) return { from: 0, to: 0 };
  if (available <= 0) return { from: 0, to: columns };

  let fits = columns;
  while (fits > 1 && (columns - fits) * SPINE_WIDTH + fits * COLUMN_WIDTH > available) {
    fits -= 1;
  }
  const deepest = columns - fits;
  const from = anchor === null ? deepest : Math.min(Math.max(0, anchor), deepest);
  return { from, to: from + fits };
}

/**
 * How many matches the Tree's box asks for.
 *
 * Smaller than the launcher's 40: this list is a way *into* the columns and
 * not a place to read results in, and a reader who cannot see their asset in
 * ten types another letter.
 */
export const MATCH_LIMIT = 10;

/**
 * The query the Tree's search box sends — story 30.
 *
 * **The launcher's engine, narrowed to one kind.** `corpus::ASSET` (#428)
 * indexes an asset's name at weight A and its ancestor path at weight B, which
 * is exactly what a search in the Tree wants: "pve-02" finds the hypervisor
 * first and the containers under it after. A second search of its own here
 * would be a second answer to *what matches*, and it would be the one that
 * could not see an asset whose column has not been opened.
 *
 * Two things the engine does with this query are worth knowing at the call
 * site, because neither is a bug to be fixed here:
 *
 * * the box's own grammar **wins** over this filter (`query::merge` takes the
 *   typed kinds if there are any), so `#ticket` typed in here comes back as
 *   tickets — dropped by {@link matchesIn}, which offers assets only;
 * * `asset:` typed in here comes back **empty**, because the engine still
 *   short-circuits that prefix until the launcher can draw an asset hit
 *   (#436). The Tree's box needs no prefix: everything it asks for is an
 *   asset already.
 *
 * **`raw` empty is a browse**, and that is the second caller (#437): the
 * engine answers a text-free query carrying only a filter without ranking or
 * excerpts, ordered by recency, so ⌘T's picker offers the assets a person has
 * touched most recently through this same query. `limit` is a parameter for
 * that caller and for no other reason — {@link MATCH_LIMIT} is what a box a
 * reader can type more into wants, and a list with no box is a different
 * number. There is one query builder because there is one question: *which
 * assets*.
 */
export function estateQuery(raw: string, limit: number = MATCH_LIMIT): SearchQuery {
  // `noFilters()` and then the one dimension, rather than the literal shape:
  // a dimension added to `SearchFilters` is one edit in the bridge and none
  // here.
  return { raw, limit, filters: { ...noFilters(), kinds: ["asset"] } };
}

/** One asset the search box offers, in the engine's own order. */
export interface Match {
  id: string;
  name: string;
  /**
   * Where it sits, as the corpus wrote it — ancestor names, outermost first.
   * `null` for an asset at the top of the estate, which sits nowhere.
   */
  path: string | null;
}

/**
 * The assets in one search answer, best first.
 *
 * Rank order is the engine's, and it is kept: the *first* match is what Enter
 * reveals, and re-sorting here would make the box and the launcher disagree
 * about which asset a query means.
 */
export function matchesIn(response: SearchResponse): Match[] {
  const group = response.groups.find((candidate) => candidate.kind === "asset");
  if (group === undefined) return [];
  return group.hits.map((hit) => ({
    id: hit.entity_id,
    name: hit.title,
    path: hit.path,
  }));
}
