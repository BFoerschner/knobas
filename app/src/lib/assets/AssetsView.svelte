<!--
  The Assets view, on its **Tree** tab — the estate as Miller columns with a
  fixed right pane (issue #428, spec #427 stories 27 and 33).

  **Never "board"** (ADR-0009, `CONTEXT.md` **Tree**). The word does not appear
  on this surface, in its classes, or in its address.

  **One column per level, and each column is one read.** `assetTree(parentId)`
  answers with what one asset holds; the columns are those answers side by
  side. A recursive read of the whole estate would be one round trip and an
  unbounded answer, which is the thing Miller columns exist to avoid — and the
  path to a deep asset comes back with the asset itself
  (`AssetDetail.held_by`), so opening `#/asset/<id>` cold needs no walk.

  **The pane is fixed and the columns scroll under it** (story 33): the whole
  point of the layout is that the path stays visible while the reader reads.

  **What is in force, and what is wrong inside** (#431). The pane's *In force*
  group is the value the asset actually has — environment, owner, health —
  against the *Properties* group, which is what is stored on it. Where a value
  comes from an ancestor the note says which, and it is a link: story 10's
  point is that a reader who sees `prod` needs one click to the place `prod`
  can be changed. The column badge counts what is **inside** a row and is
  coloured by the worst of it, which is why a row can be red with an amber
  badge — the row's colour is about the row, the badge is about its contents.

  **The keyboard walks the same rails a click does** (#430, stories 28-30).
  Every arrow, `Enter` and `Esc` answers with an *address* (`tree.ts`'s
  `walk`), so a walked path and a clicked one are the same path read back the
  same way — and `Esc` that the Tree has no step for is left for the shell's
  ladder, which takes it back to the room. The columns that no longer fit
  beside the fixed pane collapse to spines rather than pushing the pane off
  the screen, and the search box is the launcher's own engine narrowed to
  `corpus::ASSET`: it hands the first match's id to the router and the
  columns follow, so "search reveals the path" is the same derivation
  everything else here uses.

  **The Tree is writable, and every write goes back through the address**
  (#429). A plus on every column header creates at that level; the pane renames,
  edits typed and custom properties, moves and deletes. What none of them do is
  patch what is on screen: a write is followed by a **re-read**, so the columns,
  the pane and the history are one answer from the backend rather than an
  optimistic guess that a failed second write would leave standing.

  **What this ticket does not draw, and why the gaps are gaps rather than
  stubs.** The *open URL* / *copy SSH* actions are #431's neighbours in spec §2
  and arrive with the routes that carry the URLs (#432). Wires, *Link to…*, the
  linked-work badge and monitoring are #433, #435 and M4.1; monitors are also
  the half of story 37's *own* health that is not here yet. The Monitors tab is
  M4.1's, so the tab strip has one tab in it: a disabled sibling would teach the
  reader only that the app is unfinished. **Environment, owner and status are
  editable fields on `AssetEdit` and this pane does not set them**: #431 owns
  their *in force* half and draws it, and a control that wrote the stored value
  beside a line reading "or inherited from vm-db-01" is a second ticket's
  design, not this one's omission. **Dragging a row between columns** is the
  create/move gesture this ticket's parenthetical allows and does not require;
  the parent picker is the one that works with a keyboard, and it is what
  landed.
-->
<script lang="ts">
  import { ipcErrorMessage, type SearchResponse } from "../ipc";
  import {
    assetTree as realAssetTree,
    assetTypes as realAssetTypes,
    createAsset as realCreateAsset,
    createRoute as realCreateRoute,
    deleteAsset as realDeleteAsset,
    deleteRoute as realDeleteRoute,
    editAsset as realEditAsset,
    editRoute as realEditRoute,
    getAsset as realGetAsset,
    getRoute as realGetRoute,
    moveAsset as realMoveAsset,
    type AssetDetail,
    type AssetRow,
    type AssetProperty,
    type Environment,
    type Inherited,
    type AssetType,
    type PropertyKind,
    type RouteDetail,
    type RouteRow,
  } from "../ipc/assets";
  import { search as realSearch } from "../ipc/search";
  // The launcher's own debounce, imported rather than copied: two search
  // boxes in one app that wait different amounts of time feel like two apps.
  import { DEBOUNCE_MS } from "../launcher";
  import { latestRead } from "../shell/latest-read";
  import Modal from "../shell/Modal.svelte";
  import { ago } from "../shell/time";
  import { hashFor, type Router } from "../shell/router.svelte";
  import { openExternal as realOpenExternal } from "../shell/open-external";
  import CreateDialog from "./CreateDialog.svelte";
  import MoveDialog from "./MoveDialog.svelte";
  import RouteDialog from "./RouteDialog.svelte";
  import {
    draftOf,
    inputTypeFor,
    kindFor,
    parseProperty,
    PROPERTY_KINDS,
    propertyEdit,
  } from "./editing";
  import {
    addressOf,
    columnPathFor,
    emptyPath,
    estateQuery,
    heldByPath,
    landingOf,
    matchesIn,
    problemBadge,
    routeAddressOf,
    selectionIn,
    sourceOf,
    stripFor,
    walk,
    type ColumnPath,
    type Match,
  } from "./tree";

  /**
   * The bridge this view needs, injectable so a test needs no Tauri — the
   * shape `StandupView`'s `StandupPorts` and `DayReview`'s `DayPorts` have.
   */
  interface AssetPorts {
    assetTree: typeof realAssetTree;
    getAsset: typeof realGetAsset;
    /** The launcher's engine. The Tree narrows it to assets (`estateQuery`). */
    search: typeof realSearch;
    assetTypes: typeof realAssetTypes;
    createAsset: typeof realCreateAsset;
    editAsset: typeof realEditAsset;
    moveAsset: typeof realMoveAsset;
    deleteAsset: typeof realDeleteAsset;
    /** The routes an asset exposes and is reached by (#432). */
    getRoute: typeof realGetRoute;
    createRoute: typeof realCreateRoute;
    editRoute: typeof realEditRoute;
    deleteRoute: typeof realDeleteRoute;
    /**
     * The OS browser, for the *open URL* action a route finally gives this
     * view something to point at (spec §12.2's actions list).
     *
     * A port like the rest, and for the rest's reason: a test must be able to
     * press the button without a Tauri backend behind it.
     */
    openExternal: typeof realOpenExternal;
  }

  let {
    router,
    ports,
    now = () => new Date(),
    stripWidth,
  }: {
    router: Router;
    ports?: Partial<AssetPorts>;
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
  // re-read through another.
  const io: AssetPorts = {
    assetTree: realAssetTree,
    getAsset: realGetAsset,
    search: realSearch,
    assetTypes: realAssetTypes,
    createAsset: realCreateAsset,
    editAsset: realEditAsset,
    moveAsset: realMoveAsset,
    deleteAsset: realDeleteAsset,
    getRoute: realGetRoute,
    createRoute: realCreateRoute,
    editRoute: realEditRoute,
    deleteRoute: realDeleteRoute,
    openExternal: realOpenExternal,
    ...ports,
  };

  /**
   * The route the address names (#432), or `null` for every other address.
   *
   * `#/route/<id>` is the Tree at the asset that exposes the route, so this is
   * read *before* the selected asset: the address does not carry an asset id
   * and the route's own read is what supplies one.
   */
  const routeId = $derived(
    router.route.view === "assets" ? (router.route.routeId ?? null) : null,
  );

  /** The route `routeId` names, once it has been read. */
  let routeDetail = $state<RouteDetail | null>(null);

  /**
   * The asset the address names, or `null` for the bare `#/assets/tree`.
   *
   * Two ways an address names one, and the second is the route's: story 14's
   * *a ticket about a certificate links to the route whose certificate it is*
   * has to open somewhere a reader can see what the route belongs to, which is
   * the pane of the asset exposing it. The `routeDetail.route.id === routeId`
   * guard is what keeps a stale read from selecting the previous route's asset
   * for the frame between two route addresses.
   */
  const selectedId = $derived.by(() => {
    if (router.route.view !== "assets") return null;
    if (router.route.assetId !== null) return router.route.assetId;
    if (routeId === null || routeDetail === null) return null;
    return routeDetail.route.id === routeId ? routeDetail.route.asset_id : null;
  });

  /** The pane's read. `null` until something is selected and read. */
  let detail = $state<AssetDetail | null>(null);
  /** One list per column, aligned with `path.parents`. */
  let columns = $state<AssetRow[][]>([]);
  let failure = $state<string | null>(null);
  /** Nothing has come back yet — told apart from "an estate with nothing in it". */
  let loaded = $state(false);

  /**
   * The built-in type table, read once.
   *
   * A constant on the backend, so once is enough and a second read would ask
   * the same question twice. Empty until it lands, which is what disables the
   * pluses: a create dialog with no type to offer is a dialog that cannot
   * create.
   */
  let types = $state<AssetType[]>([]);

  /**
   * Why the type table is not there, when it is not.
   *
   * Its **own** state and not `writeFailure`: that one is drawn inside the
   * pane and is cleared whenever the selection moves, so a failed type read on
   * the bare `#/assets/tree` — where there is no pane at all — would have left
   * the reader with a disabled plus whose only explanation said the read had
   * *not happened yet*, which is a different and untrue thing.
   */
  let typesFailure = $state<string | null>(null);

  /**
   * Bumped by every write, and read by the pane's effect.
   *
   * The pane re-reads on **the address changing or this changing**, and a
   * write that leaves the address alone — an edit, a move — changes only this.
   * A view that patched its own state instead would be showing an answer
   * nobody gave it, and story 11's *every mutation is a line in the history*
   * is precisely the part that cannot be patched: the line is written by the
   * backend and arrives only by reading.
   */
  let revision = $state(0);

  /**
   * The level a plus was pressed on.
   *
   * A type rather than three loose fields, because the id, the name and the
   * type conventions are one fact — *which asset the new one goes inside* —
   * and they were travelling together from `openCreate` through three props on
   * the dialog. `parent` is `null` at the top of the estate, where `id` is
   * `null` too and there is no name and no convention.
   */
  let creating = $state<CreateAt | null>(null);
  let moving = $state(false);
  let deleting = $state(false);
  /** The route editor: open on a new route, or on one of the pane's own. */
  let exposing = $state(false);
  let editingRoute = $state<RouteRow | null>(null);
  /** The property row being edited, by key, and the text in its field. */
  let editingKey = $state<string | null>(null);
  let draft = $state("");
  let renaming = $state(false);
  let nameDraft = $state("");
  /** The *add a property* form, open with a key, a kind and a value. */
  let adding = $state(false);
  let newKey = $state("");
  let newKind = $state<PropertyKind>("text");
  let newValue = $state("");
  /**
   * What the last write refused with — shown in the pane, beside what it
   * refused, rather than as a toast over a surface the reader is working in.
   */
  let writeFailure = $state<string | null>(null);
  let writing = $state(false);

  /**
   * The layout, derived from the pane's read and from nothing else.
   *
   * Not from a click: a click sets the address, the address is read back, and
   * the columns follow the answer. That is what makes an address opened cold
   * draw the same thing as an address clicked into.
   */
  const path = $derived<ColumnPath>(
    detail === null || detail.asset.id !== selectedId ? emptyPath() : columnPathFor(detail),
  );

  const detailRead = latestRead<AssetDetail>();
  const columnsRead = latestRead<AssetRow[][]>();
  const routeRead = latestRead<RouteDetail>();

  /**
   * Read the route the address names, and nothing else.
   *
   * A read of its own rather than a search through the pane's `exposes`: the
   * pane has no asset to read until this answers, which is the whole point of
   * the address — a link into the estate from a ticket knows the route and
   * nothing else. It re-runs on a write (`revision`) so that editing the
   * selected route redraws it.
   */
  $effect(() => {
    const id = routeId;
    void revision;
    if (id === null) {
      routeDetail = null;
      return;
    }
    void routeRead(() => io.getRoute(id), {
      ok: (answer) => {
        // `failure` is left alone: it belongs to the pane's own read, and a
        // route landing is no evidence that the asset read succeeded.
        routeDetail = answer;
      },
      fail: (cause) => {
        routeDetail = null;
        failure = ipcErrorMessage(cause);
      },
    });
  });

  /**
   * The type table, once.
   *
   * No dependency is read, so this runs on mount and never again — which is
   * the whole claim: `asset_types` answers a constant, and it answers before
   * the database is up, so the pluses work on a cold start.
   */
  $effect(() => {
    void io
      .assetTypes()
      .then((answer) => {
        types = answer;
      })
      .catch((cause: unknown) => {
        // Not `failure`: the columns and the pane still draw. What stops
        // working is creating, and the line below the tab strip plus the
        // plus's own title are where that is said.
        typesFailure = ipcErrorMessage(cause);
      });
  });

  /**
   * Which asset the editors on screen belong to.
   *
   * Not `$state`: it is a note to the effect below and nothing draws from it.
   * A half-typed IP has to go when the reader clicks another asset — a field
   * left open over a different asset would save the first one's value onto the
   * second — and it has to **stay** across a re-read of the same asset, which
   * is why this is a comparison rather than a reset on every run.
   */
  let editorsFor: string | null = null;

  /** The address names an asset: read it, or clear the pane when it names none. */
  $effect(() => {
    const id = selectedId;
    // A write bumps this; the read below is what puts its result on screen.
    void revision;
    if (id !== editorsFor) {
      editorsFor = id;
      closeEditors();
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
        // The selection is dropped rather than kept: an asset that is not
        // there has no path, and a pane still showing the last one would be
        // the view lying about what the address names.
        detail = null;
        failure = ipcErrorMessage(cause);
      },
    });
  });

  /**
   * Fetch every column the layout calls for and swap them in **together**.
   *
   * One assignment rather than one per column: a half-drawn Miller layout is a
   * path with a hole in it. Driven off {@link path} rather than off a click,
   * which is what makes an address opened cold draw what an address clicked
   * into draws.
   */
  $effect(() => {
    const parents = path.parents;
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
    router.go(addressOf(row));
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
    const row = tree?.querySelector('.cols button.row[aria-current="true"]') ?? null;
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
   * * the arrows and `Enter` are answered everywhere **except** the three
   *   places with keys of their own: the pane, which is full of buttons
   *   (*Held by*, *Holds*, a history) whose activation `Enter` would
   *   otherwise swallow; the search box, whose arrows walk its offers; and
   *   the tab strip. Everywhere else in the Tree — a column row, a spine, the
   *   strip itself — the arrows are the walk's.
   */
  const KEEPS_ITS_OWN_KEYS = ".pane, .find, .tabs";

  function onkeydown(event: KeyboardEvent): void {
    if (event.key !== "Escape" && !WALK_KEYS.includes(event.key)) return;
    const from = event.target;
    const elsewhere = from instanceof Element && from.closest(KEEPS_ITS_OWN_KEYS) !== null;
    if (event.key !== "Escape" && elsewhere) return;

    const step = walk(event.key, path, columns);
    if (step.go === "nowhere" && event.key === "Escape") return;
    event.preventDefault();
    event.stopPropagation();
    if (step.go === "asset") {
      wantFocus = true;
      router.go(addressOf({ id: step.id }));
    } else if (step.go === "top") {
      // No focus to want: nothing is selected, and the flag would be spent on
      // whatever the reader selected next.
      router.go(hashFor({ view: "assets", tab: "tree", assetId: null }));
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
    router.go(addressOf({ id: match.id }));
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
  function closeEditors() {
    editingKey = null;
    renaming = false;
    adding = false;
    creating = null;
    moving = false;
    deleting = false;
    exposing = false;
    editingRoute = null;
    writeFailure = null;
  }

  /**
   * Run one write and re-read what it changed.
   *
   * Every mutation in this view goes through here, for two reasons that are
   * really one: a refusal is shown **in the pane, in the backend's own
   * words** — the cycle refusal names the asset that closes the loop, the
   * delete refusal names what the branch still holds, and neither is a
   * sentence this view could write for itself — and a success is followed by a
   * read rather than by a patch, so the history line the mutation wrote is on
   * screen the moment the write lands.
   *
   * Answers `null` when the write was refused, which is how a caller knows
   * whether to close the editor it was called from — and also, without a
   * message, when one is **already in flight**. Both leave the editor open,
   * which is the right outcome for each: a refusal has a sentence beside it,
   * and a second `Enter` pressed during a save is answered by the save that is
   * already running. Every control that calls this is `disabled={writing}`, so
   * the second case is only reachable from the keyboard.
   */
  async function write<T>(action: () => Promise<T>, reread = true): Promise<T | null> {
    if (writing) return null;
    writing = true;
    writeFailure = null;
    try {
      const answer = await action();
      if (reread) revision += 1;
      return answer;
    } catch (cause) {
      writeFailure = ipcErrorMessage(cause);
      return null;
    } finally {
      writing = false;
    }
  }

  /** The level a plus creates inside: an asset, or the top of the estate. */
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
  function schemaOf(typeId: string): AssetType | undefined {
    return types.find((type) => type.id === typeId);
  }

  /** Which kind of input a property's editor is. */
  function kindOf(property: AssetProperty): PropertyKind {
    return kindFor(property, detail === null ? undefined : schemaOf(detail.asset.type_id));
  }

  /**
   * `Esc` in an inline editor: put it away, and **consume the press**.
   *
   * Rung 1 of the `Esc` ladder, the rung `Modal`'s own handler occupies for a
   * dialog. The view answers `Escape` from anywhere inside itself -- that is
   * #430's walk, and it is deliberate, because a reader in the pane means the
   * same thing by the key as one standing in a column. So an editor that
   * closed and let the press through would cancel the edit *and* walk the
   * selection out of the asset being edited, which is two unwinds for one
   * keypress.
   */
  function cancelEditor(event: KeyboardEvent, close: () => void) {
    event.preventDefault();
    event.stopPropagation();
    close();
  }

  function beginEdit(property: AssetProperty) {
    editingKey = property.key;
    draft = draftOf(property);
    writeFailure = null;
  }

  /** Save one property, or clear it when the field was emptied. */
  async function saveProperty(property: AssetProperty) {
    const current = detail;
    if (current === null) return;
    const parsed = parseProperty(kindOf(property), draft);
    if ("refused" in parsed) {
      writeFailure = parsed.refused;
      return;
    }
    const done = await write(() =>
      io.editAsset(current.asset.id, [propertyEdit(property.key, parsed.value)]),
    );
    if (done !== null) editingKey = null;
  }

  /** Story 6: a key of the reader's own, of one of the four kinds. */
  async function addProperty() {
    const current = detail;
    if (current === null) return;
    const parsed = parseProperty(newKind, newValue);
    if ("refused" in parsed) {
      writeFailure = parsed.refused;
      return;
    }
    // A key with nothing in it is not a property that reads as unset — it is a
    // key `assets::edit` would remove in the same breath it was added, writing
    // no line and changing nothing. That is a rule about *this form's* meaning
    // rather than about a value, so it is the **Add** button's `disabled` and
    // not a sentence composed here: what a value may be stays the backend's.
    if (parsed.value === null) return;
    const value = parsed.value;
    const done = await write(() =>
      io.editAsset(current.asset.id, [propertyEdit(newKey, value)]),
    );
    if (done !== null) {
      adding = false;
      newKey = "";
      newValue = "";
      // The kind resets with the other two: a form that reopened on the last
      // kind would offer a date box to a reader who came back for a port.
      newKind = "text";
    }
  }

  function beginRename() {
    if (detail === null) return;
    renaming = true;
    nameDraft = detail.asset.name;
    writeFailure = null;
  }

  async function saveName() {
    const current = detail;
    if (current === null) return;
    const done = await write(() =>
      io.editAsset(current.asset.id, [{ field: "name", value: nameDraft }]),
    );
    if (done !== null) renaming = false;
  }

  /**
   * Delete, and go where the asset was.
   *
   * No re-read of the id that has just gone: it would answer `not_found` and
   * draw the deep-link failure over a deletion that worked. The parent's
   * address is the honest place to land, and the top of the estate is where a
   * root asset leaves from.
   */
  async function confirmDelete() {
    const current = detail;
    if (current === null) return;
    const parent = current.held_by.at(-1) ?? null;
    const done = await write(async () => {
      await io.deleteAsset(current.asset.id);
      return true;
    }, false);
    if (done === null) return;
    deleting = false;
    router.go(
      parent === null ? hashFor({ view: "assets", tab: "tree", assetId: null }) : addressOf(parent),
    );
  }

  /** What a history line says, in one sentence. */
  function line(verb: string, raw: unknown): string {
    const said = (value: unknown): string => {
      if (value === null || value === undefined) return "nothing";
      if (typeof value === "object" && value !== null && "value" in value) {
        return String((value as { value: unknown }).value);
      }
      return String(value);
    };
    // `fields`, not `detail`: the component already has a `detail`, and two
    // different things under one name in one file is a reader's trap.
    const fields = (raw ?? {}) as Record<string, unknown>;
    if (verb === "created") return "Created";
    if (verb === "deleted") return "Deleted";
    if (verb === "moved") return `Moved to ${nameOf(fields.to) ?? said(fields.to)}`;
    const field = fields.field === "property" ? said(fields.key) : said(fields.field);
    // A route's `target` edit carries asset **ids** on both sides, for the
    // reason a move's line does: the name at the moment of the edit is not
    // the name now. So it gets a move's treatment too, rather than putting
    // `target: asset:postgres → nothing` in front of a reader who has the
    // word *postgres* on the same pane.
    const shown = (value: unknown): string =>
      fields.field === "target" ? (nameOf(value) ?? said(value)) : said(value);
    return `${field}: ${shown(fields.from)} → ${shown(fields.to)}`;
  }

  /**
   * What an asset id is *called*, out of the rows the pane already has.
   *
   * A move's history line carries the new parent's **id**, which is the only
   * thing the backend can honestly write down — the name at the moment of the
   * move is not the name now. After the move that parent is on the held-by
   * path, so the pane can say `Moved to knobas-jira` instead of `Moved to
   * asset:knobas-jira`. `null` when it is not, and the id is then drawn as it
   * is rather than guessed at.
   */
  function nameOf(value: unknown): string | null {
    if (detail === null || typeof value !== "string") return null;
    const known = [detail.asset, ...detail.held_by, ...detail.holds];
    return known.find((row) => row.id === value)?.name ?? null;
  }

  /** A property's value as text. `null` is drawn as an em dash, not as "null". */
  function reading(property: AssetProperty): string {
    if (property.value === null) return "—";
    return String(property.value.value);
  }

  /** Open a route's own address — the Tree, at the asset exposing it. */
  function openRoute(route: RouteRow) {
    router.go(routeAddressOf(route));
  }

  /**
   * Hand a route's URL to the OS browser — spec §12.2's *open URL*, which
   * this ticket is the one that finally has a URL for.
   *
   * `openExternal` refuses anything that is not `http:`/`https:`, and the
   * refusal is **shown rather than swallowed**: a route is a URL *or an
   * endpoint*, so `postgres://10.0.0.20:5432` is a perfectly good route that
   * no browser can open, and the reader is owed that sentence rather than a
   * button that does nothing.
   */
  async function openInBrowser(url: string) {
    try {
      await io.openExternal(url);
    } catch (rejection) {
      writeFailure = `Could not open the link: ${ipcErrorMessage(rejection)}`;
    }
  }

  /**
   * Follow a `ValueSource`'s link, where it has one.
   *
   * A function rather than a `!` in the template: `goTo` is `null` exactly
   * when the value is set on this asset, and the branch that must never
   * happen is a click leading back to the asset the reader is looking at.
   */
  function goToSource(hash: string | null) {
    if (hash !== null) router.go(hash);
  }
</script>

<!--
  `Esc` and the arrows are read here, at the top of the view, and not on
  `window`: the shell's handler is on `window` (`shell/keys.ts`), so a press
  the Tree answers is stopped before it gets there, and a press the Tree has
  no answer for arrives there on its own. Binding both to `window` would make
  the order they were installed in decide what `Esc` means.
-->
<!--
  The view is not a control and takes no focus of its own: what the reader
  focuses is a row, a spine or the box, and this handler reads the presses
  that bubble up from them. So the rule the ignore silences — an element with
  a key handler needs a role — is asking for a role no assistive technology
  would want here.
-->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<section class="view" onkeydown={onkeydown}>
  <div class="room-bar">
    <h1>Assets</h1>
    <!--
      A tab strip with one tab: *Tree* is a destination with an address
      (`#/assets/tree`) and the name is what keeps "board" off this surface
      (ADR-0009). Its sibling *Monitors* arrives with M4.1.
    -->
    <nav class="tabs" aria-label="Assets views">
      <!--
        `hashFor` and not the literal `"#/assets/tree"`: the tab rides in the
        address, so when *Monitors* arrives with M4.1 a literal would keep
        type-checking while pointing at the wrong tab. Bare literals stay right
        for the addresses that carry nothing (`"#/inbox"`, `"#/sources"`).
      -->
      <button
        class="tab on"
        aria-current="page"
        onclick={() => router.go(hashFor({ view: "assets", tab: "tree", assetId: null }))}
      >
        Tree
      </button>
    </nav>

  </div>

  {#if typesFailure}
    <!--
      Outside the pane on purpose: creating is a whole-view capability, and the
      address that most needs this message (`#/assets/tree`, nothing selected)
      has no pane to draw it in.
    -->
    <p class="fail tf" role="alert">Creating is unavailable: {typesFailure}</p>
  {/if}

  <!--
    The estate's search (story 30), on a strip of its own between the room bar
    and the columns.

    Not *in* the room bar: `app.css` gives that 40px and `overflow: hidden`,
    and the offers hang below the box — in there they were clipped to a
    sliver. A row of its own also keeps the box clear of the path it is about
    to open, since the offers overlay the columns and nothing else.
  -->
  <div class="find">
    <input
      class="q"
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
      <ol class="res">
        {#each matches as match, index (match.id)}
          <li>
            <button
              class="hit {index === active ? 'on' : ''}"
              aria-current={index === active ? "true" : undefined}
              onclick={() => reveal(match)}
            >
              <span class="nm">{match.name}</span>
              <!--
                Where it lives, before the reader takes it: two containers
                called `postgres` are told apart by their path and by
                nothing else on this line.
              -->
              <span class="pth faint">{match.path ?? "top level"}</span>
            </button>
          </li>
        {/each}
        {#if searchFailure}
          <li class="none fail">{searchFailure}</li>
        {:else if answered && matches.length === 0}
          <li class="none">Nothing in the estate matches “{query.trim()}”.</li>
        {/if}
      </ol>
    {/if}
  </div>

  <div class="tree" bind:this={tree}>
    <div class="cols" bind:this={stripEl}>
      {#if failure}
        <p class="empty fail">{failure}</p>
      {:else if loaded && columns.length > 0 && (columns[0]?.length ?? 0) === 0}
        <!--
          The estate with nothing in it, which is what a fresh install has
          until the import (#439) or a hand-created asset. It says so in its
          own words rather than drawing an empty column the reader would take
          for a failed read — and it carries the one control that gets out of
          this state, because a column header with no rows under it is not
          somewhere a reader thinks to look.
        -->
        <div class="none">
          <p class="empty">Nothing in the estate yet.</p>
          <button class="btn" disabled={types.length === 0} onclick={() => openCreate(0)}>
            New asset
          </button>
        </div>
      {:else}
        {#each columns as column, index (index)}
          {@const chosen = selectionIn(path, index)}
          {#if collapsed(index)}
            <!--
              A spine (story 28, `CONTEXT.md`). A button, because clicking it
              re-expands the column — and the only thing it says is which
              asset of the reader's own path runs through it, turned on its
              side.
            -->
            {@const step = spineRow(index)}
            <button
              class="col spine"
              onclick={() => (anchored = { at: index, of: selectedId })}
              title="Show what {step?.name ?? 'the estate'} holds"
            >
              <span class="smg" aria-hidden="true">{step?.monogram ?? "··"}</span>
              <span class="vn">{step?.name ?? "top level"}</span>
            </button>
          {:else}
            <div class="colw">
              <!--
                Story 34: a plus on **every** column header, so creating at that
                level is one click and not a click plus a selection. The header
                also names the level, which is what makes the plus unambiguous —
                a bare `+` over the third column says nothing about what it
                creates inside.
              -->
              <header class="col-h">
                <span class="col-t">{headingFor(index)}</span>
                <button
                  class="plus"
                  disabled={types.length === 0}
                  aria-label="New asset in {headingFor(index)}"
                  title={plusTitle(index)}
                  onclick={() => openCreate(index)}
                >
                  +
                </button>
              </header>
              <ol class="col">
                {#each column as row (row.id)}
                  {@const badge = problemBadge(row)}
                  <li>
                    <button
                      class="row {chosen === row.id ? 'on' : ''}"
                      aria-current={chosen === row.id ? "true" : undefined}
                      title={addressOf(row)}
                      onclick={() => select(row)}
                    >
                      <span class="mg" title={row.type_label}>{row.monogram}</span>
                      <span class="nm">{row.name}</span>
                      {#if badge}
                        <!--
                          Story 32: what a *closed* branch is hiding. The number is
                          the count of descendants carrying warn or down, and the
                          colour is the worst of them — so a row that is itself
                          down while holding one warning container is a red row
                          with an amber badge.
                        -->
                        <span
                          class="badge {badge.tone}"
                          title="{badge.count} {badge.count === 1 ? 'problem' : 'problems'} inside"
                        >{badge.count}</span>
                      {/if}
                      {#if row.has_children}
                        <!--
                          The chevron is the only thing that says there is a next
                          column, which is why an empty trailing column is never
                          drawn: the absence here has already said it.
                        -->
                        <span class="chev" aria-hidden="true">›</span>
                      {/if}
                    </button>
                  </li>
                {/each}
              </ol>
            </div>
          {/if}
        {/each}
      {/if}
    </div>

    <aside class="pane">
      {#if detail === null}
        <p class="empty">
          {loaded ? "Select an asset to see what it holds." : "Reading…"}
        </p>
      {:else}
        <header class="pane-h">
          <span class="mg">{detail.asset.monogram}</span>
          {#if renaming}
            <input
              class="inp"
              type="text"
              aria-label="Name"
              autocomplete="off"
              value={nameDraft}
              oninput={(event) => (nameDraft = event.currentTarget.value)}
              onkeydown={(event) => {
                if (event.key === "Enter") void saveName();
                if (event.key === "Escape") cancelEditor(event, () => (renaming = false));
              }}
            />
            <button class="btn sm pri" disabled={writing} onclick={() => void saveName()}>
              Save
            </button>
          {:else}
            <h2>{detail.asset.name}</h2>
            <span class="kind">{detail.asset.type_label}</span>
          {/if}
        </header>

        <p class="crumb mono" title="Where this asset sits in the estate">
          {heldByPath(detail).join(" / ")}
        </p>

        <!--
          What the asset *has*, as against what is stored on it. Environment
          and owner walk up to the nearest asset that sets them (stories 8 and
          9) and health is the worst of this asset and everything under it
          (story 37) — three values that can each come from somewhere else, so
          each one says where.
        -->
        <!--
          Environment and owner draw the same three lines from the same shape,
          so they are one snippet: two copies differing only in which field
          they read is the pane's own version of the duplication the backend
          avoids with one `inherited` walk.
        -->
        {#snippet inForce(label: string, held: Inherited<string | Environment> | null)}
          <div class="prop force">
            <dt>{label}</dt>
            <dd>
              {#if held}
                {@const from = sourceOf(detail!, held)}
                <span>{held.value}</span>
                {#if from.goTo}
                  <button class="link src" onclick={() => goToSource(from.goTo)}>{from.note}</button>
                {:else}
                  <span class="src faint">{from.note}</span>
                {/if}
              {:else}
                <span class="faint">Not set anywhere</span>
              {/if}
            </dd>
          </div>
        {/snippet}

        <section class="grp">
          <h3 class="lab">In force</h3>
          <dl class="props">
            {@render inForce("Environment", detail.effective_environment)}
            {@render inForce("Owner", detail.effective_owner)}
            <div class="prop force">
              <dt>Health</dt>
              <dd>
                <span class="hl {detail.asset.health}">{detail.asset.health}</span>
                {#if detail.asset.health !== detail.asset.status}
                  <!--
                    Only when the two differ: repeating "own status: down"
                    under a health of "down" is a line that never says
                    anything.
                  -->
                  <span class="src faint">own status {detail.asset.status}</span>
                {/if}
              </dd>
            </div>
          </dl>
        </section>

        <!--
          The three writes that are about the asset rather than about one of
          its fields. Environment, owner and status are not among them: the
          *In force* group above is #431's and draws where each value comes
          from, and setting the stored half beside that is a design of its own.
        -->
        <div class="acts">
          <button class="btn sm" onclick={beginRename}>Rename</button>
          <button class="btn sm" onclick={() => (moving = true)}>Move…</button>
          <button class="btn sm danger" onclick={() => (deleting = true)}>Delete</button>
        </div>

        {#if writeFailure}
          <p class="fail wf" role="alert">{writeFailure}</p>
        {/if}

        <section class="grp">
          <h3 class="lab">Properties</h3>
          <dl class="props">
            {#each detail.properties as property (property.key)}
              <div class="prop {property.custom ? 'own' : ''}">
                <dt>{property.label}</dt>
                <dd>
                  {#if editingKey === property.key}
                    <!--
                      The kind decides the control, and the kind of a
                      *declared but unfilled* property can only come from the
                      type table — which is why it is on the wire. `value` and
                      `oninput` rather than `bind:value`: Svelte refuses a
                      two-way binding on an input whose `type` is dynamic.
                    -->
                    <input
                      class="inp"
                      type={inputTypeFor(kindOf(property))}
                      aria-label={property.label}
                      autocomplete="off"
                      value={draft}
                      oninput={(event) => (draft = event.currentTarget.value)}
                      onkeydown={(event) => {
                        if (event.key === "Enter") void saveProperty(property);
                        if (event.key === "Escape") cancelEditor(event, () => (editingKey = null));
                      }}
                    />
                    <button
                      class="btn sm pri"
                      disabled={writing}
                      onclick={() => void saveProperty(property)}
                    >
                      Save
                    </button>
                  {:else}
                    <!--
                      The value is the control. A declared property nobody has
                      filled in draws an em dash and is still a button, because
                      the empty one is exactly the property a reader has come
                      to fill in.
                    -->
                    <button
                      class="val {property.value === null ? 'faint' : ''}"
                      title="Edit {property.label}"
                      onclick={() => beginEdit(property)}
                    >
                      {reading(property)}
                    </button>
                  {/if}
                </dd>
              </div>
            {/each}
          </dl>
          {#if detail.properties.length === 0}
            <p class="empty">This type declares no properties, and none were added.</p>
          {/if}

          {#if adding}
            <!--
              Story 6: a key of the reader's own, of one of the four kinds, on
              any asset. The kind is asked for rather than guessed from the
              text, because `"8080"` and `8080` are different properties and a
              date read as a string cannot be ordered against another one.
            -->
            <!--
              `Esc` is read on the form and not on each field: the key box and
              the kind picker are as much a way out of this form as the value
              box is, and a press in either of them would otherwise close the
              form *and* walk the selection (see `cancelEditor`).
              `svelte-ignore` because the wrapper is a form, not a control: the
              role a key handler usually asks for is one the three fields
              inside already carry.
            -->
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div
              class="add"
              onkeydown={(event) => {
                if (event.key === "Escape") cancelEditor(event, () => (adding = false));
              }}
            >
              <input
                class="inp k"
                type="text"
                aria-label="New property key"
                autocomplete="off"
                placeholder="backup window"
                bind:value={newKey}
              />
              <!-- The kinds are enumerated once, in `editing.ts`. -->
              <select class="inp" aria-label="New property kind" bind:value={newKind}>
                {#each PROPERTY_KINDS as offered (offered.kind)}
                  <option value={offered.kind}>{offered.label}</option>
                {/each}
              </select>
              <input
                class="inp"
                type={inputTypeFor(newKind)}
                aria-label="New property value"
                autocomplete="off"
                value={newValue}
                oninput={(event) => (newValue = event.currentTarget.value)}
                onkeydown={(event) => {
                  if (event.key === "Enter") void addProperty();
                }}
              />
              <div class="add-f">
                <button class="btn sm" onclick={() => (adding = false)}>Cancel</button>
                <button
                  class="btn sm pri"
                  disabled={writing || newKey.trim() === "" || newValue.trim() === ""}
                  onclick={() => void addProperty()}
                >
                  Add
                </button>
              </div>
            </div>
          {:else}
            <button
              class="btn sm"
              onclick={() => {
                adding = true;
                writeFailure = null;
              }}
            >
              Add a property
            </button>
          {/if}
        </section>

        <section class="grp">
          <h3 class="lab">Held by</h3>
          {#if detail.held_by.length === 0}
            <p class="empty">At the top of the estate.</p>
          {:else}
            <ul class="lst">
              {#each detail.held_by as held (held.id)}
                <li>
                  <button class="link" onclick={() => select(held)}>{held.name}</button>
                </li>
              {/each}
            </ul>
          {/if}
        </section>

        <section class="grp">
          <h3 class="lab">Holds</h3>
          {#if detail.holds.length === 0}
            <p class="empty">Nothing.</p>
          {:else}
            <ul class="lst">
              {#each detail.holds as held (held.id)}
                <li>
                  <button class="link" onclick={() => select(held)}>{held.name}</button>
                  <span class="faint">{held.type_label}</span>
                </li>
              {/each}
            </ul>
          {/if}
        </section>

        <!--
          Both ends of a route, and they are two lists rather than one with a
          direction column: *what this asset offers* and *how this asset is
          reached* are different questions, and the mockup's pane asks them
          under two headings ("N out · M in").
        -->
        <!--
          What every route row draws whichever list it is in: its name, whether
          it is public, its URL as the *open URL* action, and the reader's own
          properties. One snippet rather than two copies, the way `inForce`
          above is one — the halves that differ are the line under it, and they
          are what each list passes in.
        -->
        {#snippet routeRow(route: RouteRow, where: import("svelte").Snippet)}
          <li class:on={route.id === routeId} aria-current={route.id === routeId ? "true" : undefined}>
            <span class="rname">
              <button class="link" onclick={() => openRoute(route)}>{route.name}</button>
              {#if route.visibility === "public"}
                <span class="vis">public</span>
              {/if}
            </span>
            <button
              class="url mono"
              title="Open {route.url}"
              onclick={() => void openInBrowser(route.url)}
            >
              {route.url}
            </button>
            <span class="rto">{@render where()}</span>
            {#each route.properties as property (property.key)}
              <span class="rprop faint">{property.label} {reading(property)}</span>
            {/each}
          </li>
        {/snippet}

        <section class="grp">
          <h3 class="lab">Exposes</h3>
          {#if detail.exposes.length === 0}
            <p class="empty">No routes.</p>
          {:else}
            <ul class="lst routes">
              {#each detail.exposes as route (route.id)}
                {@render routeRow(route, lands)}
                {#snippet lands()}
                  {#if route.target_id !== null}
                    {@const to = route.target_id}
                    <span class="faint" aria-hidden="true">→</span>
                    <button class="link" onclick={() => goToSource(addressOf({ id: to }))}>
                      {route.target_name}
                    </button>
                  {:else}
                    <span class="faint">lands on nothing knobas knows</span>
                  {/if}
                  <button
                    class="btn sm"
                    onclick={() => {
                      editingRoute = route;
                      // The same clean slate `Add a route` opens on: a failure
                      // from the last write, or from a link that would not
                      // open, is drawn above *Properties* and far from here,
                      // and one left behind the dialog reads as this edit's.
                      writeFailure = null;
                    }}
                  >
                    Edit…
                  </button>
                {/snippet}
              {/each}
            </ul>
          {/if}
          <button
            class="btn sm"
            onclick={() => {
              exposing = true;
              writeFailure = null;
            }}
          >
            Add a route
          </button>
        </section>

        <section class="grp">
          <h3 class="lab">Reachable via</h3>
          {#if detail.reachable_via.length === 0}
            <p class="empty">Nothing lands here.</p>
          {:else}
            <ul class="lst routes">
              {#each detail.reachable_via as route (route.id)}
                {@render routeRow(route, arrives)}
                <!--
                  Where it lands and who offers it — the two facts that make
                  this list readable from the arriving end. `landing` is
                  `tree.ts`' one answer to "here, through something above, or
                  on something inside".
                -->
                {#snippet arrives()}
                  {@const landing = landingOf(detail!, route)}
                  {#if landing.goTo}
                    <button class="link" onclick={() => goToSource(landing.goTo)}>
                      {landing.note}
                    </button>
                  {:else}
                    <span class="faint">{landing.note}</span>
                  {/if}
                  <span class="faint">· exposed by</span>
                  <button class="link" onclick={() => goToSource(addressOf({ id: route.asset_id }))}>
                    {route.asset_name}
                  </button>
                {/snippet}
              {/each}
            </ul>
          {/if}
        </section>

        <!--
          The selected route's **own** history (spec #427: routes *"have their
          own history"*), drawn only when the address names one. Beside the
          asset's rather than inside the route rows: a line per route in the
          list would be a wall of them, and what a reader who followed
          `#/route/<id>` came for is the story of that one route.
        -->
        {#if routeDetail !== null && routeDetail.route.id === routeId}
          <section class="grp">
            <h3 class="lab">{routeDetail.route.name} — history</h3>
            {#if routeDetail.history.length === 0}
              <p class="empty">Nothing recorded.</p>
            {:else}
              <ol class="hist">
                {#each routeDetail.history as entry (entry.id)}
                  <li>
                    <span class="hw">{line(entry.verb, entry.detail)}</span>
                    <span class="ha faint">{ago(entry.at, now())}</span>
                  </li>
                {/each}
              </ol>
            {/if}
          </section>
        {/if}

        <section class="grp">
          <h3 class="lab">History</h3>
          {#if detail.history.length === 0}
            <p class="empty">Nothing recorded.</p>
          {:else}
            <ol class="hist">
              {#each detail.history as entry (entry.id)}
                <li>
                  <span class="hw">{line(entry.verb, entry.detail)}</span>
                  <span class="ha faint">{ago(entry.at, now())}</span>
                </li>
              {/each}
            </ol>
          {/if}
        </section>
      {/if}
    </aside>
  </div>
</section>

{#if creating}
  <CreateDialog
    at={creating}
    {types}
    create={io.createAsset}
    onclose={() => (creating = null)}
    oncreated={(row) => {
      creating = null;
      // The address, not a local insert: the new asset is selected in its own
      // column because the Tree read it back, which is the same path a click
      // takes and the reason a cold link lands in the same place.
      router.go(addressOf(row));
    }}
  />
{/if}

{#if moving && detail}
  <MoveDialog
    asset={detail.asset}
    heldBy={detail.held_by}
    tree={io.assetTree}
    move={io.moveAsset}
    onclose={() => (moving = false)}
    onmoved={() => {
      moving = false;
      // The address is unchanged — the asset kept its id — so the re-read is
      // what redraws the columns at the new path.
      revision += 1;
    }}
  />
{/if}

{#if (exposing || editingRoute !== null) && detail}
  <!--
    One dialog for both, opened on a route or on nothing. `route` decides
    which, and `closeEditors` puts both away when the selection moves — a
    route editor left standing over another asset would save this asset's URL
    onto that one.
  -->
  <!-- Bound here: a callback prop is its own scope and the narrowing above
       does not reach into it — `deleting`'s dialog below binds for the same
       reason. -->
  {@const on = detail.asset}
  <RouteDialog
    asset={on}
    heldBy={detail.held_by}
    route={editingRoute}
    tree={io.assetTree}
    create={io.createRoute}
    edit={io.editRoute}
    remove={io.deleteRoute}
    onclose={() => {
      exposing = false;
      editingRoute = null;
    }}
    onsaved={(row) => {
      exposing = false;
      editingRoute = null;
      // The address, not a local insert: the route is selected in the pane
      // because the Tree read it back, which is the path a link from a ticket
      // takes as well.
      router.go(routeAddressOf(row));
      revision += 1;
    }}
    ondeleted={() => {
      exposing = false;
      editingRoute = null;
      // Back to the asset that exposed it: the route's own address would
      // answer `not_found` and draw the deep-link failure over a deletion
      // that worked — `confirmDelete`'s rule.
      router.go(addressOf(on));
      revision += 1;
    }}
  />
{/if}

{#if deleting && detail}
  <!-- Bound here: a `{#snippet}` is its own scope and the narrowing above does
       not reach into it. -->
  {@const doomed = detail.asset}
  <Modal title="Delete asset" subtitle={doomed.name} onclose={() => (deleting = false)}>
    {#snippet body()}
      <p class="ask">
        Delete <strong>{doomed.name}</strong>? Its history goes with it, and links drawn to it
        stay visible and marked.
      </p>
      <!--
        Nothing here counts what the asset holds. `assets::delete` refuses a
        branch by name, with the count in the sentence, and a second copy of
        that rule in the frontend is the copy that goes stale — so the refusal
        below is the backend's own.
      -->
      {#if writeFailure}
        <p class="fail" role="alert">{writeFailure}</p>
      {/if}
    {/snippet}
    {#snippet footer()}
      <button class="btn" onclick={() => (deleting = false)}>Cancel</button>
      <button class="btn danger" disabled={writing} onclick={() => void confirmDelete()}>
        {writing ? "Deleting…" : "Delete"}
      </button>
    {/snippet}
  </Modal>
{/if}

<style>
  /* Three rows now: the room bar, the search strip, and the tree under both.
     `app.css` gives `.view` two, and the strip is this view's own. */
  .view {
    grid-template-rows: auto auto 1fr;
  }

  /* Columns scroll on their own axis; the pane does not move. Story 33. */
  .tree {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 320px;
    min-height: 0;
    overflow: hidden;
  }

  .cols {
    display: flex;
    align-items: stretch;
    min-width: 0;
    overflow-x: auto;
    overflow-y: hidden;
  }

  /*
    The column and its header travel together; the rows scroll under it.

    Its 220px is `tree.ts`'s `COLUMN_WIDTH` -- see the spine's rule below.
    The width moved here from `.col` when the header arrived (#429): what the
    strip lays out is this wrapper, so this is the number the arithmetic has
    to be told.
  */
  .colw {
    display: flex;
    flex-direction: column;
    width: 220px;
    flex: none;
    min-height: 0;
    border-right: 1px solid var(--hair);
  }

  .col-h {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 24px;
    flex: none;
    padding: 0 4px 0 10px;
    border-bottom: 1px solid var(--hair);
  }

  .col-h .col-t {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font: 600 10px/1 var(--disp);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--muted);
  }

  .col-h .plus {
    flex: none;
    width: 18px;
    height: 18px;
    border-radius: 2px;
    color: var(--muted);
    font: 400 14px/1 var(--sans);
  }

  .col-h .plus:hover:not(:disabled) {
    background: var(--raised);
    color: var(--text);
  }

  .col-h .plus:disabled {
    opacity: 0.4;
    cursor: default;
  }

  /*
    The search strip: one row, the box on the right, and its offers hanging
    over the columns rather than pushing them down while a reader types.
  */
  .find {
    position: relative;
    display: flex;
    justify-content: flex-end;
    padding: 5px 14px;
    border-bottom: 1px solid var(--hair);
  }

  .find .q {
    width: 220px;
    height: 24px;
    padding: 0 8px;
    border: 1px solid var(--hair);
    border-radius: 2px;
    background: var(--panel);
    font-size: 12px;
  }

  .find .res {
    position: absolute;
    top: 100%;
    right: 14px;
    z-index: 5;
    display: grid;
    gap: 1px;
    width: 320px;
    max-height: 320px;
    margin: 4px 0 0;
    padding: 4px;
    overflow-y: auto;
    list-style: none;
    border: 1px solid var(--hair);
    background: var(--panel);
    box-shadow: 0 8px 24px rgb(0 0 0 / 35%);
  }

  .find .hit {
    display: grid;
    width: 100%;
    padding: 4px 6px;
    text-align: left;
    font-size: 12px;
  }

  .find .hit.on,
  .find .hit:hover {
    background: var(--raised);
  }

  .find .hit .pth {
    font-family: var(--mono);
    font-size: 10.5px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .find .none {
    padding: 6px;
    color: var(--muted);
    font-size: 12px;
  }

  .col {
    display: flex;
    flex-direction: column;
    gap: 1px;
    flex: 1;
    margin: 0;
    padding: 6px 0;
    list-style: none;
    overflow-y: auto;
  }

  /*
    A spine (story 28). Its 30px, and `.colw`'s 220px above, are `tree.ts`'s
    `SPINE_WIDTH` and `COLUMN_WIDTH`: the arithmetic that decides *which*
    columns collapse is pure, so it has to be told how wide the things it
    arranges are drawn. `the strip's arithmetic measures the widths this view
    actually draws` in `tree.test.ts` reads both numbers back out of this file
    — changing one without the other makes the strip overflow the pane the
    collapse exists to protect, and that test is what says so.
  */
  .col.spine {
    width: 30px;
    flex: none;
    align-items: center;
    padding: 6px 0;
    /* Its own, since #429 moved the full column's to the `.colw` wrapper the
       spine does not have: without it a run of spines is one grey block. */
    border-right: 1px solid var(--hair);
    background: var(--panel);
    cursor: pointer;
  }

  .col.spine:hover {
    background: var(--raised);
  }

  .col.spine .smg {
    flex: none;
    font: 500 9.5px/1 var(--mono);
    color: var(--muted);
  }

  /* The name, turned on its side: a 30px column has no other way to carry
     one, and the path is the thing a reader needs back. */
  .col.spine .vn {
    flex: 1;
    min-height: 0;
    margin: 7px 0;
    writing-mode: vertical-rl;
    text-orientation: mixed;
    font: 600 11px/1 var(--disp);
    letter-spacing: 0.1em;
    color: var(--faint);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .col.spine:hover .vn {
    color: var(--muted);
  }

  .none {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 8px;
    padding: 10px 12px;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    height: var(--row);
    padding: 0 8px 0 10px;
    text-align: left;
    color: var(--text);
    font-size: 12px;
  }

  .row:hover {
    background: var(--raised);
  }

  .row.on {
    background: var(--raised);
    box-shadow: inset 2px 0 0 var(--text);
  }

  .row .nm {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .row .chev {
    color: var(--faint);
    flex: none;
  }

  /* Story 32's badge. Amber and red are the app's own two alarm colours
     (`--amber`, `--fail`), and the number is monospaced so a column of them
     lines up at one glance rather than being read one row at a time. */
  .row .badge {
    flex: none;
    min-width: 16px;
    padding: 0 4px;
    border: 1px solid currentColor;
    border-radius: 8px;
    font-family: var(--mono);
    font-size: 10px;
    line-height: 14px;
    text-align: center;
  }

  .row .badge.warn {
    color: var(--amber);
  }

  .row .badge.down {
    color: var(--fail);
  }

  .pane {
    border-left: 1px solid var(--hair);
    background: var(--panel);
    padding: 10px 12px 18px;
    overflow-y: auto;
    min-width: 0;
  }

  .pane-h {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 4px;
  }

  .pane-h h2 {
    font: 600 16px/1.2 var(--disp);
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .crumb {
    font-size: 11px;
    color: var(--muted);
    margin: 0 0 14px;
    overflow-wrap: anywhere;
  }

  .grp {
    margin: 0 0 14px;
  }

  .grp h3 {
    margin: 0 0 4px;
  }

  .props {
    display: grid;
    gap: 2px;
    margin: 0;
  }

  .prop {
    display: grid;
    grid-template-columns: 108px minmax(0, 1fr);
    column-gap: 8px;
    align-items: baseline;
    font-size: 12px;
  }

  /* A custom key is marked, so the reader can tell the schema from their own
     additions without the pane keeping a copy of the type table. */
  .prop.own dt::after {
    content: "*";
    color: var(--faint);
  }

  .prop dt {
    color: var(--muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .prop dd {
    display: flex;
    align-items: center;
    gap: 4px;
    margin: 0;
    font-family: var(--mono);
    overflow-wrap: anywhere;
  }

  /*
     The *In force* rows stack their value and the note about where it came
     from, rather than sharing one line: the note is a sentence, and `.prop
     dd`'s `overflow-wrap: anywhere` -- right for an IP or an image tag --
     breaks a sentence one character at a time in a 200px pane.
  */
  .prop.force dd {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 1px;
  }

  /*
     Where a value came from, under the value and never instead of it. One
     line, elided if the ancestor's name is long -- the treatment `.row .nm`
     gets one section up, and the reason is the same: this is a sentence, and
     a sentence broken across three lines in a 200px column reads as three.
  */
  .prop .src {
    max-width: 100%;
    font-family: var(--sans);
    font-size: 11px;
    text-align: left;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .prop button.src {
    text-decoration: underline;
  }

  .prop .hl.warn {
    color: var(--amber);
  }

  .prop .hl.down {
    color: var(--fail);
  }

  .prop .hl.up {
    color: var(--ok);
  }

  .prop .hl.none {
    color: var(--faint);
  }

  /* The value reads as the value it is until it is hovered: a pane of boxes
     would be a form, and this is a record that can be edited. */
  .prop dd .val {
    flex: 1;
    min-width: 0;
    padding: 0 3px;
    text-align: left;
    color: inherit;
    font: inherit;
    border-radius: 2px;
    overflow-wrap: anywhere;
  }

  .prop dd .val:hover {
    background: var(--raised);
  }

  .prop dd .inp {
    height: 22px;
    font: inherit;
  }

  .acts {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin: 0 0 12px;
  }

  .wf {
    margin: -6px 0 12px;
    font-size: 12px;
  }

  .tf {
    margin: 0;
    padding: 6px 12px;
    font-size: 12px;
    border-bottom: 1px solid var(--hair);
  }

  .add {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 88px;
    gap: 6px;
    margin-top: 8px;
  }

  .add .add-f {
    grid-column: 1 / -1;
    display: flex;
    justify-content: flex-end;
    gap: 6px;
  }

  .ask {
    margin: 0;
    font-size: 13px;
  }

  /* One route per row: what it is called, where it answers, and where it
     lands — three lines at the width the pane has, not a table. */
  .routes li {
    display: grid;
    gap: 2px;
    padding: 4px 0;
    border-bottom: 1px solid var(--hair);
  }

  .routes li.on {
    box-shadow: inset 2px 0 0 var(--link);
    padding-left: 6px;
  }

  .routes .rname {
    display: flex;
    align-items: baseline;
    gap: 6px;
  }

  .routes .vis {
    font: 500 9px var(--mono);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--amber);
  }

  .routes .url {
    justify-self: start;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 11px;
    color: var(--muted);
    text-align: left;
  }

  .routes .url:hover {
    color: var(--link);
  }

  .routes .rto,
  .routes .rprop {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 4px;
    font-size: 11px;
  }

  .lst,
  .hist {
    display: grid;
    gap: 2px;
    margin: 0;
    padding: 0;
    list-style: none;
    font-size: 12px;
  }

  .lst li,
  .hist li {
    display: flex;
    align-items: baseline;
    gap: 8px;
  }

  .hist .hw {
    flex: 1;
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .hist .ha {
    flex: none;
    font-family: var(--mono);
    font-size: 11px;
  }
</style>
