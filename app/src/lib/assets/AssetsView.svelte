<!--
  The asset tree: the estate as Miller columns with a fixed preview beside
  them (issue #428, spec #427 stories 27 and 33), at `#/assets` and, with an
  asset picked, `#/assets/<id>`.

  **The tree is for where; the asset's page is for what.** The tree finds an
  asset, shows where it sits and what it holds, and adds assets at the level
  they belong. Reading an asset whole and editing it is its page,
  `#/asset/<id>` (`tf/asset/TfAsset.svelte`): *Open asset* in the preview, a
  double click on a row, or `Enter` on a row with nothing under it. The
  preview says enough to know it is the right one: what is in force on it,
  what is filled in, and the routes, whose wires run to the columns.

  **Never "board"** (ADR-0009, `CONTEXT.md` **Tree**).

  **One column per level, and each column is one read.** `assetTree(parentId)`
  answers with what one asset holds; the columns are those answers side by
  side, and the path to a deep asset comes back with the asset itself
  (`AssetDetail.held_by`), so opening `#/assets/<id>` cold needs no walk.

  **The preview is fixed and the columns scroll under it** (story 33): the
  path stays visible while the reader reads.

  **The keyboard walks the same rails a click does** (#430, stories 28-30).
  Every arrow, `Enter` and `Esc` answers with an *address* (`tree.ts`'s
  `walk`), so a walked path and a clicked one are the same path read back the
  same way. The columns that no longer fit beside the preview collapse to
  spines, and the search box is the launcher's own engine narrowed to
  `corpus::ASSET`: it hands the first match's id to the shell and the columns
  follow.

  **Creating is here, and goes back through the address** (#429): *New asset*
  in the header adds to the rightmost open column, a plus on every column
  header adds at that level, and *Import…* loads an estate file. A write is
  followed by a **re-read**, never a patch.

  **What is wrong right now** (#444, story 58) is a strip above the columns:
  every open alert in the estate, one row per asset the broken monitor
  watches, each a click to that asset. The same store the top strip counts,
  so the badge and this list cannot be a moment apart.
-->
<script lang="ts">
  import { ipcErrorMessage, type SearchResponse } from "../ipc";
  import {
    applyEstateImport as realApplyEstateImport,
    assetTree as realAssetTree,
    assetTypes as realAssetTypes,
    createAsset as realCreateAsset,
    editAsset as realEditAsset,
    getAsset as realGetAsset,
    previewEstateImport as realPreviewEstateImport,
    produceEstateFile as realProduceEstateFile,
    type AssetDetail,
    type AssetRow,
    type AssetType,
    type Environment,
    type Inherited,
  } from "../ipc/assets";
  import { search as realSearch } from "../ipc/search";
  // The launcher's own debounce, imported rather than copied: two search
  // boxes in one app that wait different amounts of time feel like two apps.
  import { DEBOUNCE_MS } from "../launcher";
  import { alerts as sharedAlerts, type Alerts } from "./alerts.svelte";
  import { latestRead } from "../shell/latest-read";
  import { ago } from "../shell/time";
  import { Windowed } from "../shell/windowed.svelte";
  import { hashFor } from "../shell/router";
  import { openExternal as realOpenExternal } from "../shell/open-external";
  import { filledOf, isCode, readingOf, shownValue } from "../tf/asset/asset-page";
  import CreateDialog from "./CreateDialog.svelte";
  import ImportDialog from "./ImportDialog.svelte";
  import {
    addressOf,
    columnPathFor,
    emptyPath,
    estateQuery,
    landingOf,
    matchesIn,
    pageOf,
    problemBadge,
    selectionIn,
    sourceOf,
    stripFor,
    walk,
    wireKey,
    wiresFor,
    workBadge,
    type AssetsNav,
    type ColumnPath,
    type Match,
    type Wire,
  } from "./tree";

  /**
   * The bridge this view needs, injectable so a test needs no Tauri — the
   * shape `StandupView`'s `StandupPorts` and `DayReview`'s `DayPorts` have.
   */
  interface AssetPorts {
    assetTree: typeof realAssetTree;
    getAsset: typeof realGetAsset;
    /** The launcher's engine. The tree narrows it to assets (`estateQuery`). */
    search: typeof realSearch;
    assetTypes: typeof realAssetTypes;
    createAsset: typeof realCreateAsset;
    /** The properties *New asset* asks for, written onto the asset it made. */
    editAsset: typeof realEditAsset;
    /** The OS browser, for a route's URL in the preview. */
    openExternal: typeof realOpenExternal;
    /** The Import's two halves (#439), and running an importer (#509). */
    previewEstateImport: typeof realPreviewEstateImport;
    applyEstateImport: typeof realApplyEstateImport;
    produceEstateFile: typeof realProduceEstateFile;
  }

  let {
    nav,
    ports,
    alerts = sharedAlerts,
    now = () => new Date(),
    stripWidth,
  }: {
    nav: AssetsNav;
    ports?: Partial<AssetPorts>;
    /**
     * The live open-alert store this view lists (#444).
     *
     * A prop with the shell's singleton as its default — the shape the top
     * strip's `alerts`, `inbox` and `health` have — and **the same store the
     * top strip counts**, which is what makes the badge and this list one
     * answer rather than two reads taken a moment apart. Not one of `ports`,
     * because the ports are commands this view calls and this is a store the
     * shell already keeps live.
     */
    alerts?: Alerts;
    /** Injectable clock — what the history's *ago* readings are relative to. */
    now?: () => Date;
    /**
     * How wide the column strip is, in pixels — measured from the element
     * itself unless a caller says otherwise.
     *
     * A seam because the collapse is a question about a *window*: jsdom
     * measures every element as zero, so without this no test could put a
     * reader in front of a narrow one. Zero is a real state as well as the
     * test default — nothing has been laid out yet — and it collapses
     * nothing (`stripFor`).
     */
    stripWidth?: () => number;
  } = $props();

  // svelte-ignore state_referenced_locally
  // Read once, at init: production omits this prop, and a bridge swapped
  // mid-life would leave what is on screen read through one set of ports and

  // svelte-ignore state_referenced_locally
  // Read once, at init: production omits this prop, and a bridge swapped
  // mid-life would leave what is on screen read through one set of ports and
  // re-read through another.
  const io: AssetPorts = {
    assetTree: realAssetTree,
    getAsset: realGetAsset,
    search: realSearch,
    assetTypes: realAssetTypes,
    createAsset: realCreateAsset,
    editAsset: realEditAsset,
    openExternal: realOpenExternal,
    previewEstateImport: realPreviewEstateImport,
    applyEstateImport: realApplyEstateImport,
    produceEstateFile: realProduceEstateFile,
    ...ports,
  };

  /** The asset the address picks, or `null` for the bare `#/assets/tree`. */
  const selectedId = $derived(nav.route.view === "assets" ? nav.route.assetId : null);

  /** The preview's read. `null` until something is picked and read. */
  let detail = $state<AssetDetail | null>(null);
  /** One list per column, aligned with `path.parents`. */
  let columns = $state<AssetRow[][]>([]);
  let failure = $state<string | null>(null);
  /** Nothing has come back yet — told apart from "an estate with nothing in it". */
  let loaded = $state(false);

  /**
   * The built-in type table, read once. Empty until it lands, which is what
   * disables the pluses: a create dialog with no type to offer cannot create.
   */
  let types = $state<AssetType[]>([]);
  /** Why the type table is not there, when it is not: said under the header, since a plus cannot say it. */
  let typesFailure = $state<string | null>(null);

  /**
   * Bumped by a create or an import, and read by the reads below: a write
   * that leaves the address alone changes only this.
   */
  let revision = $state(0);

  /** The level *New asset* or a plus was pressed for. */
  let creating = $state<CreateAt | null>(null);
  /** Whether the Import is open (#439): a whole-estate capability, so on the header. */
  let importing = $state(false);
  /** Why a route's URL would not open, said in the preview beside it. */
  let openFailure = $state<string | null>(null);

  /**
   * The layout, derived from the preview's read and from nothing else: a
   * click sets the address, the address is read back, and the columns follow
   * the answer. That is what makes an address opened cold draw the same thing
   * as an address clicked into.
   */
  const path = $derived<ColumnPath>(
    detail === null || detail.asset.id !== selectedId ? emptyPath() : columnPathFor(detail),
  );

  const detailRead = latestRead<AssetDetail>();
  const columnsRead = latestRead<AssetRow[][]>();

  /** The type table, once: `asset_types` answers a constant, before the database is up. */
  $effect(() => {
    void io
      .assetTypes()
      .then((answer) => {
        types = answer;
      })
      .catch((cause: unknown) => {
        typesFailure = ipcErrorMessage(cause);
      });
  });

  /** Which asset the open dialog and the preview's line belong to: they go when the pick moves. */
  let editorsFor: string | null = null;

  /** The address picks an asset: read it, or clear the preview when it picks none. */
  $effect(() => {
    const id = selectedId;
    void revision;
    if (id !== editorsFor) {
      editorsFor = id;
      creating = null;
      openFailure = null;
    }
    if (id === null) {
      detail = null;
      failure = null;
      return;
    }
    void detailRead(() => io.getAsset(id), {
      ok: (answer) => {
        detail = answer;
        failure = null;
      },
      fail: (cause) => {
        // The pick is dropped rather than kept: an asset that is not there has
        // no path, and a preview still showing the last one would be the view
        // lying about what the address names.
        detail = null;
        failure = ipcErrorMessage(cause);
      },
    });
  });

  /**
   * Fetch every column the layout calls for and swap them in **together**:
   * a half-drawn Miller layout is a path with a hole in it. `revision` is
   * read too, so an import (which creates under an unchanged address) and a
   * create draw their new rows.
   */
  $effect(() => {
    const parents = path.parents;
    void revision;
    void columnsRead(() => Promise.all(parents.map((parent) => io.assetTree(parent))), {
      ok: (lists) => {
        columns = lists;
        loaded = true;
      },
      fail: (cause) => {
        columns = [];
        failure = ipcErrorMessage(cause);
        loaded = true;
      },
    });
  });

  function select(row: AssetRow) {
    nav.go(addressOf(row));
  }

  // -- the walk (story 29) --------------------------------------------------

  /**
   * The keys the Tree answers. Everything else — `Tab` above all — is left
   * alone, so the walk cannot take the one key that leaves it.
   */
  const WALK_KEYS = ["ArrowDown", "ArrowUp", "ArrowLeft", "ArrowRight", "Enter"];

  /** The strip and the pane, which is where the focus effect looks for a row. */
  let tree = $state<HTMLElement | null>(null);
  /** The column strip, which is the element whose width decides the collapse. */
  let stripEl = $state<HTMLElement | null>(null);
  /** The measured width of the column strip; `0` until it is laid out. */
  let measured = $state(0);
  /**
   * Whether the next redraw should take focus.
   *
   * A plain variable and not `$state`: the effect that reads it also clears
   * it, and a reactive flag would make that a dependency on itself. It is set
   * by the keyboard and by a revealed match — never by a click, which has
   * already put focus where the reader put it.
   */
  let wantFocus = false;

  /** How wide the strip is: what a caller says, or what the element measures. */
  const available = $derived(stripWidth === undefined ? measured : stripWidth());

  /**
   * Measure the strip on mount and whenever the window changes size.
   *
   * A resize listener rather than `bind:clientWidth`, which is a
   * `ResizeObserver` — an API jsdom does not implement, so binding would make
   * every component test of this view fail on a browser feature none of them
   * are about. The strip's width is the window's minus the fixed pane, so a
   * resize is the only thing that changes it: the columns' *content* cannot,
   * because the strip scrolls on its own axis.
   */
  $effect(() => {
    const element = stripEl;
    if (element === null) return;
    const measure = () => {
      measured = element.clientWidth;
    };
    measure();
    window.addEventListener("resize", measure);
    return () => window.removeEventListener("resize", measure);
  });

  /**
   * The spine a reader clicked to re-expand, **and the selection they clicked
   * it in**.
   *
   * The second half is what makes the anchor expire on its own. An anchor is
   * a look at one path; carried into another it would collapse the columns
   * the reader had just walked into, at a level they never clicked a spine
   * on. Stored beside the column rather than cleared by an effect, so there
   * is no reset to forget and no order for it to run in.
   */
  let anchored = $state<{ at: number; of: string | null } | null>(null);

  /** The anchor, if it still belongs to the path on screen. */
  const anchor = $derived(anchored !== null && anchored.of === selectedId ? anchored.at : null);

  /** Which columns are drawn in full; the rest are spines. */
  const strip = $derived(stripFor(columns.length, available, anchor));

  /** Is this column collapsed? */
  function collapsed(column: number): boolean {
    return column < strip.from || column >= strip.to;
  }

  /**
   * The asset a spine is labelled with: the step of the path that runs through
   * that column.
   *
   * The *selection*, not the column's parent. A reader who has walked four
   * levels wants the spine to say which VM they came through; "top level"
   * would name the column and tell them nothing about their own path. A
   * column with nothing selected in it — only ever one an anchor collapsed
   * from the other end — falls back to what it lists.
   */
  function spineRow(column: number): AssetRow | null {
    const chosen = selectionIn(path, column);
    const rows = columns[column] ?? [];
    return rows.find((row) => row.id === chosen) ?? rows[0] ?? null;
  }

  // -- a long column (#2210) --------------------------------------------------

  /**
   * Each open column's rows, windowed: a column of thousands draws only the
   * rows near its own scroll. One per column position, made as the column
   * first opens; the rows are measured by id, so a column that shows another
   * asset's children keeps nothing wrong. A row is `--row`, 1 apart.
   */
  const ROW_PX = 30;
  const columnWindows = new Map<number, Windowed>();
  function windowOf(index: number): Windowed {
    let win = columnWindows.get(index);
    if (win === undefined) {
      win = new Windowed({ count: () => columns[index]?.length ?? 0, key: (at) => columns[index]![at]!.id, estimate: ROW_PX, gap: 1, scrolls: true });
      columnWindows.set(index, win);
    }
    return win;
  }

  /** The pane's two lists that run to thousands, *Holds* and *Depends on this*, windowed the same way; a line is 18 px, 2 apart. */

  // -- the wires (story 31) -------------------------------------------------

  /** The pane, whose left edge is where every wire starts. */
  let paneEl = $state<HTMLElement | null>(null);

  /**
   * Which route row is wired to which asset, and how — `tree.ts`' answer.
   *
   * Guarded the way {@link path} is: a pane read for the *previous* address
   * would wire this address' route rows to the last one's columns for the
   * frame between the two reads.
   */
  const wireEnds = $derived<Wire[]>(
    detail === null || detail.asset.id !== selectedId ? [] : wiresFor(detail, columns, strip),
  );

  /** One wire, measured: where it lands and the curve that gets there. */
  interface DrawnWire {
    /** {@link wireKey}: which of the pane's lists, and which route. */
    key: string;
    /**
     * Where it lands — an asset's id, or `spine:<column>`.
     *
     * On the element as `data-to`, which is how both halves of this ticket's
     * fourth criterion read a wire: the component tests, where jsdom lays
     * nothing out and a curve is all zeroes, and the browser pass, which reads
     * the landing and the curve together over CDP.
     */
    to: string;
    /** Drawn dashed, arithmetic and measurement together. */
    dashed: boolean;
    /** The curve. */
    d: string;
    /** The landing point, where the dot goes. */
    x: number;
    y: number;
  }

  /**
   * The wires as they are drawn now, re-measured rather than kept.
   *
   * Every redraw builds the whole list from the elements on screen, which is
   * what makes *no stale lines* structural: a column that changed under a
   * wire cannot leave one behind, because the line is not stored anywhere
   * between two measurements.
   */
  let drawnWires = $state<DrawnWire[]>([]);

  /** One decimal is a tenth of a pixel; the rest is noise in a diff. */
  function round(value: number): number {
    return Math.round(value * 10) / 10;
  }

  /**
   * Measure every wire the arithmetic asked for, in the tree's own pixels.
   *
   * **The only part of the wires that reads the DOM**, and it reads it rather
   * than remembering it: a row's position is a fact about a scroll offset and
   * a font, and the two elements it needs are found by the ids the arithmetic
   * answered with.
   *
   * `laid` is what makes the same code honest before anything has been laid
   * out — a first frame, and every frame under jsdom, measures every
   * rectangle as zero. There is no geometry to clip against there, so nothing
   * is clipped and nothing is dropped: the wires are drawn degenerate, which
   * is what lets a component test assert *which* row is wired to *which*
   * asset while a browser certifies where the curve actually goes.
   */
  function measureWires(): void {
    const frame = tree;
    const pane = paneEl;
    const cols = stripEl;
    if (frame === null || pane === null || cols === null) {
      drawnWires = [];
      return;
    }
    const box = frame.getBoundingClientRect();
    const paneBox = pane.getBoundingClientRect();
    const colsBox = cols.getBoundingClientRect();
    const laid = box.width > 0 && box.height > 0;
    // The pane's own edge: every wire leaves the strip at the same x, so the
    // bundle reads as one thing arriving from the pane rather than as lines
    // starting at whatever indent a route's name happens to have.
    const startX = paneBox.left - box.left;
    // The part of the pane the reader can see: its box, cut to the tree's and
    // to the window's. The tree page is viewport-high and the pane its own
    // scroller (#1460), so the cut is the pane's box itself; it stays a cut
    // so that a page laid out taller than the window still draws no wire from
    // a row below its bottom edge.
    const seenTop = Math.max(paneBox.top, box.top, 0);
    const seenBottom = Math.min(paneBox.bottom, box.bottom, window.innerHeight);

    const next: DrawnWire[] = [];
    for (const wire of wireEnds) {
      // `CSS.escape` on both ids, the way `AssetsView.write.test.svelte.ts`
      // reaches for a row by one: an id is minted as `asset:<uuid>` today and
      // no selector metacharacter can reach one, but an import that took an
      // id from a source system would turn a wire into a thrown
      // `SyntaxError` inside an effect, which takes the whole Tree with it.
      const from = pane.querySelector(`li[data-wire=${CSS.escape(wire.key)}]`);
      const to = landing(cols, wire);
      if (from === null || to === null) continue;

      const fromBox = from.getBoundingClientRect();
      // A route row scrolled out of the pane, or below the window, draws
      // nothing: a line leaving an edge would start at a row the reader cannot
      // see (#1460).
      if (laid && (fromBox.bottom < seenTop + 2 || fromBox.top > seenBottom - 2)) continue;

      // The band the far end may be drawn in: the column's own scrolling list,
      // or the spine, which scrolls with nothing.
      const { box: toBox, band } = to;
      const wanted = {
        x: toBox.right - box.left - 2,
        y: toBox.top - box.top + toBox.height / 2,
      };
      // Held inside the band it may be drawn in: a row scrolled out of its own
      // column, or a column scrolled out of the strip sideways, is pointed at
      // where it *would* be rather than off the edge of the tree.
      const held = laid
        ? {
            x: Math.max(wanted.x, colsBox.left - box.left + 2),
            y: Math.min(
              Math.max(wanted.y, band.top - box.top + 5),
              band.bottom - box.top - 5,
            ),
          }
        : wanted;
      const clipped = held.x !== wanted.x || held.y !== wanted.y;
      // Nowhere left to draw: the column is under the pane's own edge.
      if (laid && held.x >= startX - 4) continue;

      const startY = fromBox.top - box.top + fromBox.height / 2;
      // Out of the pane level and into the column steeply, so the curve clears
      // the spine labels it passes over -- `E1-miller-columns.html:728`.
      const reach = Math.max(36, Math.min(130, (startX - held.x) / 2.2));
      const dive = Math.min(reach, 52);
      next.push({
        key: wire.key,
        to: wire.rowId ?? `spine:${wire.column}`,
        // A landing that had to be held inside its band is not a direct one
        // either -- the mockup's `exact && !clipped`
        // (`E1-miller-columns.html:711`).
        dashed: wire.dashed || clipped,
        d:
          `M ${round(startX)} ${round(startY)} ` +
          `C ${round(startX - reach)} ${round(startY)}, ` +
          `${round(held.x + dive)} ${round(held.y)}, ${round(held.x)} ${round(held.y)}`,
        x: round(held.x),
        y: round(held.y),
      });
    }
    drawnWires = next;
  }

  /**
   * Where a wire lands, and the band it may be drawn in: the spine, the
   * row in its column's list, or, for a row its long column has not drawn
   * (#2210), where that row would be. That row is outside the drawn ones, so
   * off the column's screen, and the band holds the wire at the column's
   * edge, dashed, as it would a row scrolled away.
   */
  function landing(cols: HTMLElement, wire: Wire): { box: Pick<DOMRect, "top" | "right" | "height">; band: DOMRect } | null {
    if (wire.rowId === null) {
      const spine = cols.querySelector(`button.tf-et-spine[data-spine="${wire.column}"]`);
      return spine === null ? null : { box: spine.getBoundingClientRect(), band: spine.getBoundingClientRect() };
    }
    const drawn = cols.querySelector(`button.tf-et-row[data-asset=${CSS.escape(wire.rowId)}]`);
    if (drawn !== null) return { box: drawn.getBoundingClientRect(), band: (drawn.closest("ol.tf-et-col") ?? drawn).getBoundingClientRect() };
    const list = cols.querySelector(`ol.tf-et-col[data-col="${wire.column}"]`);
    const at = columns[wire.column]?.findIndex((row) => row.id === wire.rowId) ?? -1;
    if (list === null || at < 0) return null;
    const band = list.getBoundingClientRect();
    return { box: { top: windowOf(wire.column).topOf(at), right: band.right, height: ROW_PX }, band };
  }

  /**
   * Re-measure after every redraw that could have moved a wire's ends.
   *
   * `$effect` runs after the DOM is updated, so the rows and spines the
   * measurement looks for are the ones this frame draws.
   *
   * {@link wireEnds} is the dependency that carries the *shape* of the
   * drawing: it is derived from the pane's read, the columns and the strip,
   * so a selection, a column and a spine collapsing each reach this through
   * it. Read here as well as inside {@link measureWires} so that the day the
   * measurement takes its list from somewhere else, the dependency does not
   * quietly leave with it.
   *
   * It is not the *only* one, and the difference is worth naming rather than
   * being discovered: {@link measureWires} also reads `tree`, `paneEl` and
   * `stripEl`, which are `$state` too, so this effect re-runs when the three
   * elements bind. That is what makes the first frame — where the bindings
   * land after the effect's first pass — draw anything at all.
   */
  $effect(() => {
    void wireEnds;
    measureWires();
  });

  /**
   * Scrolling and resizing move the ends without changing any of that.
   *
   * Capturing, because none of the three scrolls that matter bubble: the strip
   * scrolls sideways, a column's rows scroll under its header, and the pane
   * scrolls past the route lists themselves.
   */
  $effect(() => {
    const frame = tree;
    if (frame === null) return;
    const redraw = () => measureWires();
    frame.addEventListener("scroll", redraw, true);
    window.addEventListener("resize", redraw);
    return () => {
      frame.removeEventListener("scroll", redraw, true);
      window.removeEventListener("resize", redraw);
    };
  });

  /**
   * Bring a row into view, honouring *reduce motion*.
   *
   * `Flap.svelte`'s reading of the query, and read per press rather than once
   * at mount: a reader who changes the system setting while the app is open
   * has changed their mind about this scroll, not about the next launch.
   * `scrollIntoView` is guarded because jsdom does not implement it.
   */
  function bring(row: Element | null): void {
    if (row === null || typeof row.scrollIntoView !== "function") return;
    const reduce =
      typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;
    row.scrollIntoView({
      block: "nearest",
      inline: "nearest",
      behavior: reduce ? "auto" : "smooth",
    });
  }

  /**
   * Put focus on the selected row once the columns the walk asked for have
   * been drawn.
   *
   * The walk moves the *address*; the row it names exists only after the
   * reads land, so this is driven off the columns rather than off the press.
   * `preventScroll` and then {@link bring}: the browser's own scroll on focus
   * is instant and cannot be told about reduced motion.
   */
  $effect(() => {
    // Read into a variable rather than mentioned: a bare `columns;` is a
    // statement with no effect, and a compiler is entitled to drop it — with
    // the dependency this effect exists to have.
    const drawn = columns;
    if (!wantFocus || drawn.length === 0) return;
    // The deepest marked row: every column marks the path's row in it, and
    // the selection is the last. A long column draws only the rows near its
    // scroll (#2210), so the selection is brought into its drawn rows first,
    // and this runs again once they are redrawn with it.
    let row: Element | null = null;
    for (let index = drawn.length - 1; index >= 0; index -= 1) {
      const chosen = selectionIn(path, index);
      const at = chosen === null ? -1 : (drawn[index]?.findIndex((one) => one.id === chosen) ?? -1);
      if (chosen === null || at < 0) continue;
      const win = windowOf(index);
      win.reveal(at);
      void win.start;
      row = tree?.querySelector(`.tf-et-cols button.tf-et-row[data-asset=${CSS.escape(chosen)}]`) ?? null;
      break;
    }
    // Kept, not cleared, while the row is missing. The columns are redrawn
    // twice on the way to a new selection — once emptied while the pane's read
    // is out, once with the answer — and a flag cleared on the first of those
    // would leave focus on the row the reader walked *away* from.
    if (!(row instanceof HTMLElement)) return;
    wantFocus = false;
    row.focus({ preventScroll: true });
    bring(row);
  });

  /**
   * One press of the keyboard on the Tree.
   *
   * Two rules decide whether the press is the walk's, and they are different
   * on purpose:
   *
   * * `Esc` is answered from **anywhere in the view** — it is an unwind, and
   *   a reader reading the pane means the same thing by it as one standing in
   *   a column. The press the Tree has no step for is *not* consumed, and the
   *   shell's ladder takes it back to the room (`shell/keys.ts`).
   * * the arrows and `Enter` are answered everywhere **except** the two
   *   places with keys of their own: the pane, which is full of buttons
   *   (*Held by*, *Holds*, a history) whose activation `Enter` would
   *   otherwise swallow; and the search box, whose arrows walk its offers.
   *   Everywhere else in the Tree — a column row, a spine, the column strip
   *   itself — the arrows are the walk's.
   */
  /**
   * One press of the keyboard on the tree.
   *
   * * `Esc` is answered from **anywhere in the view** — it is an unwind. The
   *   press the tree has no step for is *not* consumed, and the shell's
   *   ladder takes it back to Dependencies.
   * * the arrows and `Enter` are answered everywhere **except** the preview,
   *   which is full of buttons whose activation `Enter` would otherwise
   *   swallow, and the search box, whose arrows walk its offers.
   * * `Enter` on a row with nothing under it opens the asset's page: there is
   *   no column to walk into, and the page is where the walk goes on.
   */
  const KEEPS_ITS_OWN_KEYS = ".tf-et-pane, .tf-et-find";

  function onkeydown(event: KeyboardEvent): void {
    if (event.key !== "Escape" && !WALK_KEYS.includes(event.key)) return;
    const from = event.target;
    const elsewhere = from instanceof Element && from.closest(KEEPS_ITS_OWN_KEYS) !== null;
    if (event.key !== "Escape" && elsewhere) return;

    const step = walk(event.key, path, columns);
    if (step.go === "nowhere" && event.key === "Escape") return;
    if (step.go === "nowhere" && event.key === "Enter" && selectedId !== null) {
      event.preventDefault();
      event.stopPropagation();
      nav.go(pageOf({ id: selectedId }));
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    if (step.go === "asset") {
      wantFocus = true;
      nav.go(addressOf({ id: step.id }));
    } else if (step.go === "top") {
      // No focus to want: nothing is picked, and the flag would be spent on
      // whatever the reader picked next.
      nav.go(hashFor({ view: "assets", tab: "tree", assetId: null }));
    }
  }


  // -- the search box (story 30) -------------------------------------------

  /** What is in the box. Empty is the ordinary state, and it asks nothing. */
  let query = $state("");
  /** What the estate answered, best first, and which one `Enter` reveals. */
  let matches = $state<Match[]>([]);
  let active = $state(0);
  /** The search's own failure, kept apart from the columns' (`failure`). */
  let searchFailure = $state<string | null>(null);
  /** An answer has landed for what is in the box now. */
  let answered = $state(false);
  let typing: ReturnType<typeof setTimeout> | undefined;

  const searchRead = latestRead<SearchResponse>();

  /** A pending keystroke must not outlive the view that scheduled it. */
  $effect(() => () => clearTimeout(typing));

  /** A keystroke: remember it, and ask once the typing stops. */
  function typed(raw: string): void {
    query = raw;
    clearTimeout(typing);
    if (raw.trim() === "") {
      matches = [];
      active = 0;
      answered = false;
      searchFailure = null;
      return;
    }
    typing = setTimeout(() => {
      void searchRead(() => io.search(estateQuery(query)), {
        ok: (response) => {
          matches = matchesIn(response);
          // The cursor goes back to the top on every answer: the row that was
          // under it is not in the new list (`Session.run`'s rule).
          active = 0;
          answered = true;
          searchFailure = null;
        },
        fail: (cause) => {
          matches = [];
          answered = true;
          searchFailure = ipcErrorMessage(cause);
        },
      });
    }, DEBOUNCE_MS);
  }

  /** Empty the box without asking anything. */
  function clearQuery(): void {
    clearTimeout(typing);
    query = "";
    matches = [];
    active = 0;
    answered = false;
    searchFailure = null;
  }

  /**
   * Reveal a match: open its address, and let the columns follow.
   *
   * The whole of story 30 is this line. The box knows an id and nothing about
   * where the asset lives; the pane's read answers with `held_by`, and
   * `columnPathFor` turns that into the open columns.
   */
  function reveal(match: Match): void {
    clearQuery();
    wantFocus = true;
    nav.go(addressOf({ id: match.id }));
  }

  /** The box's own keys — the launcher's, on a shorter list. */
  function onboxkey(event: KeyboardEvent): void {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      if (matches.length === 0) return;
      event.preventDefault();
      // The columns keep still while the box has the keyboard: two lists, and
      // the one in front of the reader is the one that answers.
      event.stopPropagation();
      const last = matches.length - 1;
      active = Math.min(last, Math.max(0, active + (event.key === "ArrowDown" ? 1 : -1)));
      return;
    }
    if (event.key === "Enter") {
      const match = matches[active];
      if (match === undefined) return;
      event.preventDefault();
      event.stopPropagation();
      reveal(match);
      return;
    }
    if (event.key === "Escape") {
      // An empty box has nothing to unwind, so the press falls through to the
      // Tree's own ladder and then to the shell's.
      if (query === "") return;
      event.preventDefault();
      event.stopPropagation();
      clearQuery();
    }
  }

  /**
   * Put every open editor and dialog away, and the failure with them.
   *
   * The dialogs go too, and that is not tidiness: *Move to…* and the delete
   * confirmation are **about the selected asset**, so one still standing after
   * the selection moved would be a dialog offering to move or delete something
   * other than the asset it named — which is what a QA walk that changed the
   * address with the picker open turned up.
   */

  interface CreateAt {
    /** What `create_asset` is given as `parentId`. */
    id: string | null;
    /** The asset that id names, for the dialog's subtitle and conventions. */
    parent: AssetRow | null;
  }

  /** The asset a column lists the children of — `null` for the first column. */
  function parentOfColumn(index: number): AssetRow | null {
    const parentId = path.parents[index] ?? null;
    if (parentId === null) return null;
    return columns[index - 1]?.find((row) => row.id === parentId) ?? null;
  }

  /** What a column's header calls it. */
  function headingFor(index: number): string {
    return parentOfColumn(index)?.name ?? "Estate";
  }

  /**
   * What the plus says on hover — and what it says when it cannot work.
   *
   * A disabled control has to explain itself, and the two reasons are
   * different: the table has not landed *yet*, or the read failed and it never
   * will. Saying the first when the second is true is the plainest kind of lie
   * a tooltip can tell.
   */
  function plusTitle(index: number): string {
    if (typesFailure !== null) return `The type table could not be read: ${typesFailure}`;
    if (types.length === 0) return "The type table has not been read yet";
    return `New asset in ${headingFor(index)}`;
  }

  /** Story 34: the plus, at the level whose header it sits on. */
  function openCreate(index: number) {
    creating = { id: path.parents[index] ?? null, parent: parentOfColumn(index) };
  }

  /** The type table's entry for `typeId`, or nothing for a type this build lost. */

  /** *New asset* in the header: into the rightmost open column, the level in front of the reader. */
  function openCreateHere() {
    openCreate(Math.max(0, columns.length - 1));
  }

  /** *New asset inside* in the preview: under the picked asset, which opens a column for it. */
  function openCreateInside(asset: AssetRow) {
    creating = { id: asset.id, parent: asset };
  }

  /** Follow a `ValueSource`'s link, where it has one: `null` is set here, and a link back here is no link. */
  function goToSource(hash: string | null) {
    if (hash !== null) nav.go(hash);
  }

  /**
   * Hand a route's URL to the OS browser — spec §12.2's *open URL*. A route is
   * a URL *or an endpoint*, so `postgres://10.0.0.20:5432` is a good route no
   * browser can open, and the refusal is shown rather than swallowed.
   */
  async function openInBrowser(url: string) {
    openFailure = null;
    try {
      await io.openExternal(url);
    } catch (rejection) {
      openFailure = `Could not open the link: ${ipcErrorMessage(rejection)}`;
    }
  }

  /** What is in force, and where it comes from, for the preview's two lines. */
  type Forced = Inherited<string | Environment> | null;
</script>
<!--
  `Esc` and the arrows are read here, at the top of the view, and not on
  `window`: the shell's handler is on `window` (`shell/keys.ts`), so a press
  the tree answers is stopped before it gets there, and a press it has no
  answer for arrives there on its own. The view is not a control and takes no
  focus of its own: what the reader focuses is a row, a spine or the box.
-->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<section class="tf-et" onkeydown={onkeydown}>
  <header class="tf-fhead tf-et-head">
    <div>
      <h1>Asset tree</h1>
      <p class="tf-sub">Where everything in the estate sits. Pick an asset to see what it holds; open it to read and edit it.</p>
    </div>
    <div class="tf-acts">
      <button
        class="tf-btn tf-primary"
        disabled={types.length === 0}
        title={typesFailure !== null ? `The type table could not be read: ${typesFailure}` : undefined}
        onclick={openCreateHere}>New asset</button
      >
      <!-- On the header, not on a column: an import is about the whole estate, and the address that most needs it has no column yet. -->
      <button class="tf-btn" onclick={() => (importing = true)}>Import…</button>
    </div>
  </header>

  {#if typesFailure}
    <p class="tf-banner tf-err" role="alert">Creating is unavailable: {typesFailure}</p>
  {/if}

  <!--
    The estate's search (story 30), as a page body's toolbar. Its offers hang
    over the columns rather than pushing them down while a reader types.
  -->
  <div class="tf-fbar tf-et-bar">
    <div class="tf-fsearch tf-et-find">
      <svg viewBox="0 0 16 16" width="13" height="13" aria-hidden="true"
        ><circle cx="7" cy="7" r="4.5" fill="none" stroke="currentColor" stroke-width="1.5" /><path
          d="M10.5 10.5 14 14"
          stroke="currentColor"
          stroke-width="1.5"
          stroke-linecap="round"
        /></svg
      >
      <input
        type="search"
        value={query}
        placeholder="Find an asset"
        aria-label="Find an asset in the estate"
        autocomplete="off"
        spellcheck="false"
        oninput={(event) => typed(event.currentTarget.value)}
        onkeydown={onboxkey}
      />
      {#if query.trim() !== ""}
        <ol class="tf-et-res">
          {#each matches as match, index (match.id)}
            <li>
              <button
                class="tf-et-hit"
                class:tf-et-on={index === active}
                aria-current={index === active ? "true" : undefined}
                onclick={() => reveal(match)}
              >
                <span class="tf-et-nm">{match.name}</span>
                <!-- Where it lives: two containers called `postgres` are told apart by their path alone. -->
                <span class="tf-et-pth">{match.path ?? "top level"}</span>
              </button>
            </li>
          {/each}
          {#if searchFailure}
            <li class="tf-et-none tf-ifail">{searchFailure}</li>
          {:else if answered && matches.length === 0}
            <li class="tf-et-none">Nothing in the estate matches “{query.trim()}”.</li>
          {/if}
        </ol>
      {/if}
    </div>
  </div>

  <!--
    **What is wrong right now** (story 58, #444): every open alert, one row
    per asset the broken monitor watches, each a click to that asset in the
    tree. Absent when there is nothing wrong: a permanently empty box is a
    place the eye keeps checking.
  -->
  {#if alerts.count > 0}
    <section class="tf-et-alerts" aria-label="Open alerts">
      <ul>
        {#each alerts.open as alert (alert.id)}
          {#if alert.assets.length === 0}
            <li>
              <span class="tf-dot tf-{alert.state === 'down' ? 'err' : 'warn'}" aria-hidden="true"></span>
              <span class="tf-et-mn">{alert.monitor_name}</span>
              <span class="tf-dim">watching nothing</span>
              <span class="tf-dim tf-et-wh">{ago(alert.opened_at, now())}</span>
            </li>
          {:else}
            {#each alert.assets as watched (watched.id)}
              <li>
                <span class="tf-dot tf-{alert.state === 'down' ? 'err' : 'warn'}" aria-hidden="true"></span>
                <button class="tf-et-go" onclick={() => nav.go(addressOf({ id: watched.id }))}>
                  <span class="tf-et-mn">{alert.monitor_name}</span>
                  <span class="tf-et-nm">{watched.name}</span>
                  <span class="tf-dim tf-et-pth">{watched.path ?? "top level"}</span>
                </button>
                <!-- Acked is drawn and not hidden: an ack clears the inbox item and leaves the alert open. -->
                {#if alert.acked_at}
                  <span class="tf-chip">acked</span>
                {/if}
                <span class="tf-dim tf-et-wh">{ago(alert.opened_at, now())}</span>
              </li>
            {/each}
          {/if}
        {/each}
      </ul>
    </section>
  {/if}

  <div class="tf-et-tree" bind:this={tree}>
    <div class="tf-et-cols" bind:this={stripEl}>
      {#if failure}
        <p class="tf-et-blank tf-ifail">{failure}</p>
      {:else if loaded && columns.length > 0 && (columns[0]?.length ?? 0) === 0}
        <!--
          The estate with nothing in it, which is what a fresh install has until
          the import (#439) or a hand-made asset. It says so and carries the two
          ways out of it.
        -->
        <div class="tf-empty tf-et-empty">
          <p>Nothing in the estate yet. Add the machines you work on by hand, or import an estate file.</p>
          <div class="tf-et-emptyacts">
            <button class="tf-btn tf-sm tf-primary" disabled={types.length === 0} onclick={() => openCreate(0)}>New asset</button>
            <button class="tf-btn tf-sm" onclick={() => (importing = true)}>Import…</button>
          </div>
        </div>
      {:else}
        {#each columns as column, index (index)}
          {@const chosen = selectionIn(path, index)}
          {#if collapsed(index)}
            <!--
              A spine (story 28, `CONTEXT.md`): a button, because clicking it
              re-expands the column, and it says which asset of the reader's
              own path runs through it, turned on its side.
            -->
            {@const step = spineRow(index)}
            <button
              class="tf-et-spine"
              data-spine={index}
              onclick={() => (anchored = { at: index, of: selectedId })}
              title="Show what {step?.name ?? 'the estate'} holds"
            >
              <span class="tf-et-smg" aria-hidden="true">{step?.monogram ?? "··"}</span>
              <span class="tf-et-vn">{step?.name ?? "top level"}</span>
            </button>
          {:else}
            {@const win = windowOf(index)}
            <div class="tf-et-colw">
              <!--
                Story 34: a plus on **every** column header, so creating at that
                level is one click. The header names the level, which is what
                makes the plus unambiguous.
              -->
              <header class="tf-et-colh">
                <span class="tf-et-colt">{headingFor(index)}</span>
                <span class="tf-et-colc">{column.length}</span>
                <button
                  class="tf-iconbtn tf-sm"
                  disabled={types.length === 0}
                  aria-label="New asset in {headingFor(index)}"
                  title={plusTitle(index)}
                  onclick={() => openCreate(index)}
                >
                  <svg viewBox="0 0 16 16" aria-hidden="true"
                    ><path d="M8 3.5v9M3.5 8h9" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" /></svg
                  >
                </button>
              </header>
              <ol class="tf-et-col" data-col={index} {@attach win.list}>
                {#if win.before > 0}<li class="tf-wgap" aria-hidden="true" {@attach win.room("before")}></li>{/if}
                {#each win.slice(column) as row (row.id)}
                  {@const badge = problemBadge(row)}
                  {@const work = workBadge(row)}
                  {@const own = readingOf(row.status)}
                  <li {@attach win.row(row.id)}>
                    <button
                      class="tf-et-row"
                      class:tf-et-on={chosen === row.id}
                      class:tf-sel={chosen === row.id && row.id === selectedId}
                      aria-current={chosen === row.id ? "true" : undefined}
                      data-asset={row.id}
                      title={row.type_label}
                      onclick={() => select(row)}
                      ondblclick={() => nav.go(pageOf(row))}
                    >
                      <span class="tf-dp-mono">{row.monogram}</span>
                      <span class="tf-et-nm">{row.name}</span>
                      {#if own.tone === "err" || own.tone === "warn"}
                        <!-- Its own status, beside the name: the row is red because of the row. -->
                        <span class="tf-dot tf-{own.tone}" title="Own status {own.word}"></span>
                      {/if}
                      {#if work !== null}
                        <!--
                          Story 32's first badge: how much work is linked to this
                          asset. Toneless: a linked ticket is neither good nor bad.
                        -->
                        <span class="tf-et-badge tf-et-work" title="{work} linked {work === 1 ? 'work item' : 'work items'}">{work}</span>
                      {/if}
                      {#if badge}
                        <!--
                          Story 32: what a *closed* branch is hiding, counted and
                          coloured by the worst of it, so a down row holding one
                          warning reads red with an amber badge.
                        -->
                        <span
                          class="tf-et-badge tf-et-{badge.tone}"
                          title="{badge.count} {badge.count === 1 ? 'problem' : 'problems'} inside"
                        >{badge.count}</span>
                      {/if}
                      {#if row.has_children}
                        <!-- The only thing that says there is a next column, so no empty trailing column is drawn. -->
                        <span class="tf-et-chev" aria-hidden="true">›</span>
                      {/if}
                    </button>
                  </li>
                {/each}
                {#if win.after > 0}<li class="tf-wgap" aria-hidden="true" {@attach win.room("after")}></li>{/if}
              </ol>
            </div>
          {/if}
        {/each}
      {/if}
    </div>

    <!--
      The wires (story 31, #433): one layer over the whole tree, from a route
      row in the preview across every column between to the row or spine at
      its far end. `aria-hidden`: every fact it draws is a line in the route's
      own row. Before the preview in document order and painted under it.
    -->
    <svg class="tf-et-wires" aria-hidden="true">
      {#each drawnWires as wire (wire.key)}
        <!-- A halo in the columns' own background under the wire, so a line crossing text stays a line. -->
        <path class="tf-et-halo" d={wire.d} />
        <path class="tf-et-wire" class:tf-et-dashed={wire.dashed} data-wire={wire.key} data-to={wire.to} d={wire.d} />
        <circle class="tf-et-land" cx={wire.x} cy={wire.y} r="3" />
      {/each}
    </svg>

    <aside class="tf-et-pane" bind:this={paneEl} aria-label="Preview">
      {#if detail === null || detail.asset.id !== selectedId}
        <p class="tf-et-blank">
          {loaded ? "Pick an asset to see what it holds and how it is reached." : "Reading…"}
        </p>
      {:else}
        {@const on = detail}
        {@const asset = on.asset}
        {@const health = readingOf(asset.health)}
        {@const set = on.properties.filter((property) => property.value !== null)}
        {@const filled = filledOf(on.properties)}
        <header class="tf-et-ph">
          <span class="tf-dp-mono">{asset.monogram}</span>
          <div class="tf-et-pt">
            <h2>{asset.name}</h2>
            <p class="tf-dim">{asset.type_label}</p>
          </div>
          {#if health.tone !== "muted"}<span class="tf-chip tf-{health.tone}">{health.word}</span>{/if}
        </header>
        <div class="tf-et-pacts">
          <button class="tf-btn tf-sm tf-primary" onclick={() => nav.go(pageOf(asset))}>Open asset</button>
          <button class="tf-btn tf-sm" disabled={types.length === 0} onclick={() => openCreateInside(asset)}>New asset inside</button>
        </div>

        <!--
          What is in force: environment and owner walk up to the nearest asset
          that sets them (stories 8 to 10), so each says where, and the link
          goes there in the tree.
        -->
        {#snippet inForce(label: string, held: Forced)}
          <div>
            <dt>{label}</dt>
            <dd>
              {#if held}
                {@const from = sourceOf(on, held)}
                <span>{held.value}</span>
                {#if from.goTo}
                  {@const there = from.goTo}
                  <button class="tf-linkish tf-et-src" onclick={() => goToSource(there)}>{from.note}</button>
                {:else}
                  <span class="tf-dim tf-et-src">{from.note}</span>
                {/if}
              {:else}
                <span class="tf-dim">Not set anywhere</span>
              {/if}
            </dd>
          </div>
        {/snippet}
        <dl class="tf-as-kv tf-as-force tf-et-kv">
          {@render inForce("Environment", on.effective_environment)}
          {@render inForce("Owner", on.effective_owner)}
          <div>
            <dt>Holds</dt>
            <dd>{on.holds.length === 0 ? "Nothing" : `${on.holds.length} ${on.holds.length === 1 ? "asset" : "assets"}`}</dd>
          </div>
          {#if on.monitoring.length > 0}
            <div>
              <dt>Watched by</dt>
              <dd>{on.monitoring.map((watch) => watch.name).join(", ")}</dd>
            </div>
          {/if}
        </dl>

        <h3 class="tf-et-ph3">
          Properties
          {#if filled.of > 0}<span class="tf-as-n">{filled.set} of {filled.of} set</span>{/if}
        </h3>
        {#if set.length === 0}
          <p class="tf-et-none">Nothing filled in yet. <button class="tf-linkish" onclick={() => nav.go(pageOf(asset))}>Fill it in on its page</button></p>
        {:else}
          <dl class="tf-as-kv tf-et-kv">
            {#each set as property (property.key)}
              <div>
                <dt>{property.label}</dt>
                <dd class:tf-mono={isCode(property)} class="tf-et-v">{shownValue(property)}</dd>
              </div>
            {/each}
          </dl>
        {/if}

        <!--
          Both ends of a route, as two lists: what this asset offers and how it
          is reached. `data-wire` is the end of the wire a row leaves (#433).
        -->
        {#snippet routeRow(key: string, route: import("../ipc/assets").RouteRow, where: import("svelte").Snippet)}
          <li data-wire={key}>
            <span class="tf-et-rname">{route.name}</span>
            {#if route.visibility === "public"}<span class="tf-chip">public</span>{/if}
            <button class="tf-et-url tf-mono" title="Open {route.url}" onclick={() => void openInBrowser(route.url)}>{route.url}</button>
            <span class="tf-et-rto">{@render where()}</span>
          </li>
        {/snippet}
        {#if on.exposes.length > 0}
          <h3 class="tf-et-ph3">Exposes</h3>
          <ul class="tf-et-routes">
            {#each on.exposes as route (route.id)}
              {@render routeRow(wireKey("exposes", route.id), route, lands)}
              {#snippet lands()}
                {#if route.target_id !== null}
                  {@const to = route.target_id}
                  lands on <button class="tf-linkish" onclick={() => goToSource(addressOf({ id: to }))}>{route.target_name}</button>
                {:else}
                  lands on nothing knobas knows
                {/if}
              {/snippet}
            {/each}
          </ul>
        {/if}
        {#if on.reachable_via.length > 0}
          <h3 class="tf-et-ph3">Reachable via</h3>
          <ul class="tf-et-routes">
            {#each on.reachable_via as route (route.id)}
              {@render routeRow(wireKey("via", route.id), route, arrives)}
              {#snippet arrives()}
                {@const landing = landingOf(on, route)}
                {#if landing.goTo}
                  {@const there = landing.goTo}
                  <button class="tf-linkish" onclick={() => goToSource(there)}>{landing.note}</button>
                {:else}
                  {landing.note}
                {/if}, exposed by
                <button class="tf-linkish" onclick={() => goToSource(addressOf({ id: route.asset_id }))}>{route.asset_name}</button>
              {/snippet}
            {/each}
          </ul>
        {/if}
        {#if openFailure}
          <p class="tf-ifail tf-et-none" role="alert">{openFailure}</p>
        {/if}
      {/if}
    </aside>
  </div>
</section>

{#if creating}
  <CreateDialog
    at={creating}
    {types}
    create={io.createAsset}
    edit={io.editAsset}
    onclose={() => (creating = null)}
    oncreated={(row, more) => {
      // The address, not a local insert: the new asset is picked in its own
      // column because the tree read it back, the path a click takes. With
      // *add another* the dialog stays and only the columns read again.
      revision += 1;
      if (more) return;
      creating = null;
      nav.go(addressOf(row));
    }}
  />
{/if}

{#if importing}
  <ImportDialog
    preview={io.previewEstateImport}
    apply={io.applyEstateImport}
    produce={io.produceEstateFile}
    tree={io.assetTree}
    onclose={() => (importing = false)}
    onimported={() => {
      importing = false;
      // An import creates assets and never moves the reader, so the re-read
      // is what draws the new columns.
      revision += 1;
    }}
  />
{/if}

<style>
  /*
    The design system's page: the canonical header, the filter bar as the
    page body's toolbar, then the tree, which fills the rest of the window
    and scrolls inside itself, the columns sideways and each column and the
    preview down.
  */
  .tf-et {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-height: 0;
    min-width: 0;
  }

  .tf-et > * {
    min-width: 0;
  }

  .tf-et-head {
    padding-bottom: 14px;
  }

  .tf-et > .tf-banner {
    margin: 0;
  }

  .tf-et-bar {
    position: relative;
    z-index: 6;
  }

  .tf-et-find {
    position: relative;
  }

  .tf-et-find svg {
    flex: none;
  }

  .tf-et-res {
    position: absolute;
    top: calc(100% + 4px);
    left: -1px;
    z-index: 8;
    display: grid;
    gap: 1px;
    width: 380px;
    max-height: 360px;
    margin: 0;
    padding: 4px;
    overflow-y: auto;
    list-style: none;
    border: 1px solid var(--rule);
    border-radius: 8px;
    background: var(--sheet);
    box-shadow: var(--shadow);
  }

  .tf-et-hit {
    display: grid;
    gap: 1px;
    width: 100%;
    padding: 5px 8px;
    border: 0;
    border-radius: 4px;
    background: none;
    color: var(--ink);
    text-align: left;
    font: inherit;
    cursor: pointer;
  }

  .tf-et-hit.tf-et-on,
  .tf-et-hit:hover {
    background: var(--sheet-2);
  }

  .tf-et-hit .tf-et-nm {
    font-size: var(--t-sm);
    font-weight: 600;
  }

  .tf-et-hit .tf-et-pth {
    font-size: var(--t-xs);
    color: var(--ink-2);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tf-et-none {
    padding: 6px 8px;
    color: var(--ink-2);
    font-size: var(--t-sm);
  }

  /* The open-alert strip (#444): six rows, then it scrolls, so twenty broken monitors leave the columns on screen. */
  .tf-et-alerts {
    padding: 6px 22px;
    background: var(--err-bg);
    border-bottom: 1px solid var(--rule);
  }

  .tf-et-alerts ul {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 0;
    padding: 0;
    list-style: none;
    max-height: 132px;
    overflow-y: auto;
  }

  .tf-et-alerts li {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 22px;
    font-size: var(--t-sm);
  }

  .tf-et-go {
    display: flex;
    align-items: baseline;
    gap: 8px;
    min-width: 0;
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .tf-et-go:hover .tf-et-nm {
    text-decoration: underline;
    text-underline-offset: 2px;
  }

  .tf-et-mn {
    font-weight: 600;
    white-space: nowrap;
  }

  .tf-et-go .tf-et-nm {
    font-family: var(--mono);
    font-variant-ligatures: none;
    font-size: var(--t-xs);
    white-space: nowrap;
  }

  .tf-et-go .tf-et-pth {
    font-size: var(--t-xs);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .tf-et-wh {
    margin-left: auto;
    flex: none;
    font-size: var(--t-xs);
  }

  /* Columns scroll on their own axis; the preview does not move (story 33). `relative` for the wire layer (#433). */
  .tf-et-tree {
    flex: 1;
    position: relative;
    display: grid;
    grid-template-columns: minmax(0, 1fr) 320px;
    min-height: 0;
    overflow: hidden;
    border-top: 1px solid var(--rule);
  }

  .tf-et-cols {
    display: flex;
    align-items: stretch;
    min-width: 0;
    overflow-x: auto;
    overflow-y: hidden;
    background: var(--paper);
  }

  /*
    The column and its header travel together; the rows scroll under it. Its
    220px is `tree.ts`'s `COLUMN_WIDTH`, and the spine's 30px below is
    `SPINE_WIDTH`: `the strip's arithmetic measures the widths this view
    actually draws` in `tree.test.ts` reads both back out of this file.
  */
  .tf-et-colw {
    width: 220px;
    display: flex;
    flex-direction: column;
    flex: none;
    min-height: 0;
    border-right: 1px solid var(--rule-2);
  }

  .tf-et-colh {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 36px;
    flex: none;
    padding: 0 6px 0 12px;
    border-bottom: 1px solid var(--rule-2);
    background: var(--sheet);
  }

  .tf-et-colt {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--t-sm);
    font-weight: 600;
  }

  .tf-et-colc {
    flex: none;
    font-size: var(--t-xs);
    color: var(--ink-2);
    font-variant-numeric: tabular-nums;
  }

  .tf-et-colh svg {
    width: 14px;
    height: 14px;
  }

  .tf-et-col {
    display: flex;
    flex-direction: column;
    flex: 1;
    margin: 0;
    padding: 4px 0;
    list-style: none;
    overflow-y: auto;
  }

  /* A long column's room for the rows it has not drawn (#2210): never shrunk to fit the column. */
  .tf-et-col > li:global(.tf-wgap) {
    flex: none;
  }

  /* A row is 30px tall (`ROW_PX`), its spacing inside its box, as a windowed row's must be. */
  .tf-et-row {
    display: flex;
    align-items: center;
    gap: 7px;
    width: 100%;
    height: 30px;
    padding: 0 8px 0 10px;
    border: 0;
    background: none;
    color: var(--ink);
    font: inherit;
    font-size: var(--t-sm);
    text-align: left;
    cursor: pointer;
  }

  .tf-et-row:hover {
    background: var(--sheet-2);
  }

  /* The path: every column marks the row the reader came through. */
  .tf-et-row.tf-et-on {
    background: var(--sheet-2);
    font-weight: 600;
  }

  /* The pick itself, the deepest of them: an outline, as every selection in the shell is. */
  .tf-et-row.tf-sel {
    box-shadow: inset 0 0 0 2px var(--ink);
    border-radius: 4px;
  }

  .tf-et-row .tf-et-nm {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tf-et-chev {
    flex: none;
    color: var(--ink-2);
    font-size: var(--t-lg);
    line-height: 1;
  }

  .tf-et-badge {
    flex: none;
    min-width: 18px;
    padding: 0 5px;
    border-radius: 3px;
    font-size: var(--t-2xs);
    font-weight: 600;
    line-height: 16px;
    text-align: center;
    font-variant-numeric: tabular-nums;
  }

  .tf-et-badge.tf-et-warn {
    background: var(--warn-bg);
    color: var(--warn);
  }

  .tf-et-badge.tf-et-down {
    background: var(--err-bg);
    color: var(--err);
  }

  .tf-et-badge.tf-et-work {
    background: var(--muted-bg);
    color: var(--muted);
  }

  /* A spine (story 28): 30px, `SPINE_WIDTH`; the name of the reader's own path, turned on its side. */
  .tf-et-spine {
    width: 30px;
    flex: none;
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: 8px 0;
    border: 0;
    border-right: 1px solid var(--rule-2);
    background: var(--sheet);
    color: var(--ink-2);
    cursor: pointer;
  }

  .tf-et-spine:hover {
    background: var(--sheet-2);
    color: var(--ink);
  }

  .tf-et-smg {
    flex: none;
    font: 600 9px/1 var(--mono);
  }

  .tf-et-vn {
    flex: 1;
    min-height: 0;
    margin: 8px 0;
    writing-mode: vertical-rl;
    text-orientation: mixed;
    font-size: var(--t-xs);
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .tf-et-empty {
    margin: 40px auto;
  }

  .tf-et-emptyacts {
    display: flex;
    gap: var(--control-gap);
  }

  .tf-et-blank {
    margin: 0;
    padding: 22px 16px;
    color: var(--ink-2);
    font-size: var(--t-sm);
  }

  /*
    The wires (story 31): one transparent layer across the whole tree, the
    curves measured in `measureWires`. `pointer-events: none` keeps it a
    drawing over rows a reader clicks.
  */
  .tf-et-wires {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    pointer-events: none;
    overflow: visible;
  }

  .tf-et-wires path {
    fill: none;
    stroke-linecap: round;
  }

  .tf-et-halo {
    stroke: var(--paper);
    stroke-width: 3.2;
    opacity: 0.92;
  }

  .tf-et-wire {
    stroke: var(--info);
    stroke-width: 1.2;
    opacity: 0.75;
  }

  /* Reached through something rather than pointed at: a spine standing in for the row, or a route arriving above. */
  .tf-et-wire.tf-et-dashed {
    stroke-dasharray: 3 3;
  }

  .tf-et-land {
    fill: var(--info);
    opacity: 0.85;
  }

  /* The one motion here answers the reader: a wire fades in when a pick brings it. */
  .tf-et-halo,
  .tf-et-wire,
  .tf-et-land {
    animation: tf-et-wire-in 160ms ease-out both;
  }

  @keyframes tf-et-wire-in {
    from {
      opacity: 0;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .tf-et-halo,
    .tf-et-wire,
    .tf-et-land {
      animation: none;
    }
  }

  /* Painted over the wire layer, which would otherwise draw its last pixels across the route rows. */
  .tf-et-pane {
    position: relative;
    z-index: 1;
    min-width: 0;
    overflow-y: auto;
    padding: 0 0 18px;
    border-left: 1px solid var(--rule);
    background: var(--sheet);
  }

  .tf-et-ph {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 14px 14px 8px;
  }

  .tf-et-ph .tf-dp-mono {
    margin-top: 3px;
  }

  .tf-et-pt {
    flex: 1;
    min-width: 0;
  }

  .tf-et-pt h2 {
    font-size: var(--t-xl);
    font-weight: 700;
    font-variation-settings: "wdth" 88;
    line-height: 1.2;
    overflow-wrap: anywhere;
  }

  .tf-et-pt p {
    margin: 2px 0 0;
    font-size: var(--t-sm);
  }

  .tf-et-pacts {
    display: flex;
    gap: var(--control-gap);
    flex-wrap: wrap;
    padding: 0 14px 12px;
  }

  .tf-et-ph3 {
    display: flex;
    align-items: baseline;
    gap: 8px;
    margin: 12px 14px 4px;
    font-size: var(--t-sm);
    font-weight: 600;
  }

  .tf-et-kv > div {
    grid-template-columns: minmax(0, 40%) minmax(0, 1fr);
    padding-left: 14px;
    padding-right: 14px;
  }

  .tf-et-src {
    font-size: var(--t-xs);
    text-align: left;
  }

  .tf-et-v {
    overflow-wrap: anywhere;
    display: -webkit-box;
    -webkit-line-clamp: 3;
    line-clamp: 3;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  .tf-et-v:global(.tf-mono) {
    font-size: var(--t-sm);
  }

  .tf-et-routes {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .tf-et-routes li {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 2px 8px;
    padding: 6px 14px;
    border-top: 1px solid var(--rule-2);
    font-size: var(--t-sm);
  }

  .tf-et-rname {
    font-weight: 600;
  }

  .tf-et-url {
    max-width: 100%;
    padding: 0;
    border: 0;
    background: none;
    color: var(--ink-2);
    font-size: var(--t-xs);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    cursor: pointer;
  }

  .tf-et-url:hover {
    color: var(--ink);
    text-decoration: underline;
  }

  .tf-et-rto {
    flex-basis: 100%;
    color: var(--ink-2);
    font-size: var(--t-xs);
  }

  .tf-et-rto :global(.tf-linkish) {
    font-size: var(--t-xs);
    color: var(--ink);
  }
</style>
