<!--
  The room: a heading strip and the tile board under it
  (`signal-miller.html:2519-2529`).

  One read of its own, and it does two jobs: which kinds are present here — so
  a room only draws tiles it has something to put in (§3a open kinds) — and how
  many items the room holds, which is the heading's count. Every tile then
  fetches its own page.

  It reads with the **room's own filter**, exactly as every tile in it does,
  which is the whole of a project room's narrowing (#209): there is no
  per-tile special case, and the count in the heading cannot disagree with the
  tiles under it about which room this is.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../ipc";
  import { inbox } from "../inbox/inbox.svelte";
  import { contextMembers } from "../ipc/entity";
  import { push } from "./toasts.svelte";
  import Detail from "../detail/Detail.svelte";
  import NoteView from "../notes/NoteView.svelte";
  import {
    createNote,
    listEntities,
    type EntityRow,
    type NoteLinkInput,
  } from "../ipc/entity";
  import { bornWith } from "../detail/relations";
  import AssetsTile from "./AssetsTile.svelte";
  import RoomBar from "./RoomBar.svelte";
  import SuggestionTray from "./SuggestionTray.svelte";
  import Tile from "./Tile.svelte";
  import { ASSETS_TILE, roomDrawsAssets } from "./assets-tile";
  import { contextById, type RoomContext } from "./contexts";
  import { miniBoardOverrides, type MiniBoardOverrides } from "./mini-board-overrides.svelte";
  import { kindRegistry } from "./kind-registry.svelte";
  import { tilesFor } from "./kinds";
  import { hashFor, type Router } from "./router.svelte";
  import { roomForeground } from "./timer";

  let {
    router,
    contexts,
    overrides = miniBoardOverrides,
  }: {
    router: Router;
    contexts: RoomContext[];
    /**
     * The reader's mini board layout overrides, per room, for the session
     * (#245). The window's store by default; a test builds its own, as the
     * other stores' tests do, so nothing leaks between rooms it never drew.
     */
    overrides?: MiniBoardOverrides;
  } = $props();

  /**
   * How deep the room looks to find out which kinds it holds.
   *
   * A window, not a census: a room whose newest 200 items are all tickets
   * draws a Tickets tile, and the Docs tile appears as soon as a page is
   * recent enough to matter. Asking PostgreSQL for `distinct kind` instead
   * would be a second statement in `entity.rs` for a fact this read already
   * carries.
   */
  const SCAN = 200;

  const context = $derived(contextById(router.ctx, contexts));
  /**
   * This room's override of its mini board default, if any (#245).
   *
   * Read here and handed down, never by the tile: the store is keyed by the
   * room, and the room is the one thing that knows which room this is.
   */
  const miniBoardOverride = $derived(overrides.overrideFor(context.id));
  /** The entity the address has open over this room, if any. */
  const detail = $derived(router.route.view === "room" ? router.route.detail : null);

  let kinds = $state<string[] | null>(null);
  let total = $state<number | null>(null);
  let error = $state<string | null>(null);
  let token = 0;

  $effect(() => {
    const { sources, context: ctx, project } = context.filter;
    const mine = ++token;
    kinds = null;
    total = null;
    error = null;
    void listEntities(
      {
        sources,
        kinds: [],
        updated_within_days: null,
        context: ctx,
        project,
        order: "updated_desc",
        include_deleted: false,
      },
      SCAN,
      0,
    )
      .then((page) => {
        if (mine !== token) return;
        kinds = [...new Set(page.rows.map((row) => row.kind))];
        total = page.total;
      })
      .catch((rejection) => {
        if (mine !== token) return;
        error = ipcErrorMessage(rejection);
      });
  });

  /**
   * The room's membership, for the per-context inbox filter ("3 here").
   *
   * Fetched only for a stored context — a derived room's inbox is the global
   * one — and intersected with the inbox *stream* the strip already holds, so
   * the badge and the rows it stands for come from the same read by
   * construction (the rule `inbox.svelte.ts` states for the global count does
   * not apply: this is a filter over the visible stream, not a second
   * definition of "needs me now").
   */
  let memberIds = $state<Set<string> | null>(null);
  $effect(() => {
    const ctx = context.filter.context;
    memberIds = null;
    if (!ctx) return;
    const mine = ++membersToken;
    void contextMembers(ctx)
      .then((ids) => {
        if (mine !== membersToken) return;
        memberIds = new Set(ids);
      })
      .catch(() => {
        // No members reading is no chip — the room still renders whole.
      });
  });
  let membersToken = 0;

  const inboxHere = $derived.by(() => {
    if (memberIds === null) return null;
    const members = memberIds;
    return inbox.stream.filter(
      (entry) => entry.item.entity_id !== null && members.has(entry.item.entity_id),
    ).length;
  });

  // The registry is read, not merely consulted: it is a rune, so a tile drawn
  // before `list_adapters` answered redraws with the adapter's own word rather
  // than keeping the humanised one for the session.
  const tiles = $derived(
    kinds === null ? [] : tilesFor(kinds, (kind) => kindRegistry.info(kind)),
  );

  /**
   * Whether this room draws an Assets tile (#434).
   *
   * Not a kind tile, and it could not be one: `tilesFor` is driven by the
   * kinds the room's *mirror* scan reported, and an asset is knobas' own — no
   * source ever syncs one, so no scan can ever report the kind. The rule is
   * the room's filter's, in `assets-tile.ts`, which is also what the tile
   * reads through.
   *
   * It is read here rather than inside the tile so the grid can be sized and
   * the empty state decided before anything mounts: a stored room with assets
   * and nothing synced has one tile, not the "nothing synced yet" page.
   */
  const drawsAssets = $derived(roomDrawsAssets(context.filter));
  /**
   * Two columns, so the row count is half the tiles — capped at the four the
   * stylesheet declares (`.tiles.rows-*`), past which the board scrolls rather
   * than growing rows nobody can see.
   */
  const tileCount = $derived(tiles.length + (drawsAssets ? 1 : 0));
  const rows = $derived(Math.min(4, Math.max(1, Math.ceil(tileCount / 2))));

  /**
   * The tile the reader maximised, or null for the grid (#250).
   *
   * Per visit: a new room is a fresh grid, whatever the last room had, and
   * walking *back* finds the grid too -- nothing waits under a room's id, which
   * is why this is a reset on the room and not a choice remembered per room.
   * Opening a detail over the room changes the address and not `context.id`,
   * so the tile stays maximised under the slide-over (decision 1). One tile
   * at a time is the shape of the field: a single id, not a set.
   */
  let maximised = $state<string | null>(null);
  /**
   * The room's id as its own signal, so the reset below fires on a change of
   * *room* and not on a fresh object for the same one. `App.svelte` derives
   * the switcher's list afresh after every census (a sync run ending, a
   * source's health moving), and `context` is a new object each time; a
   * derived string is equal to itself and propagates nothing. The reads
   * above still re-run on that fresh object, on purpose: a census may have
   * changed what the room holds, and a re-read costs nothing the reader can
   * see, where a reset would.
   */
  const roomId = $derived(context.id);
  $effect(() => {
    // Reads the room id and nothing else, so the effect runs when the room
    // changes and not when the choice does.
    void roomId;
    maximised = null;
  });

  /** The header's button: *Maximise* on a grid tile, *Restore* on the maximised one. */
  function toggleMaximise(tileId: string) {
    maximised = maximised === tileId ? null : tileId;
  }

  /**
   * Escape's rung 4 (`keys.ts`), reached through `bind:this` in `App.svelte`.
   *
   * Says whether it did anything, so the ladder can fall through to "nothing"
   * when the grid is already drawn.
   */
  export function restoreTile(): boolean {
    if (maximised === null) return false;
    maximised = null;
    return true;
  }

  /**
   * Whether this address is a note.
   *
   * The **id** decides, and the kind word is only a fallback for the moment
   * before one is available: `note:` is a namespace knobas keeps for itself
   * (`RESERVED_NAMESPACES`), so no source can ever write an id that looks like
   * one. `#/entity/<id>` carries no kind at all, and it still has to open the
   * right view.
   */
  function isNote(entityId: string, kind: string | null): boolean {
    return entityId.toLowerCase().startsWith("note:") || kind === "note";
  }

  /**
   * The links a note started here is **born with** (#502, spec #491 stories
   * 40-43; `CONTEXT.md`, **Capture**).
   *
   * Two of them at most, and each is present only when it has something true
   * to say:
   *
   * * `captured-in` names this room's **context**, which is `null` for every
   *   derived room — *All work*, a source, a project — because a derived room
   *   has no context (`CONTEXT.md`, **Room**). Not the room's `id`: a derived
   *   room's id (`src:gitea`) addresses nothing, and a stored room's id and its
   *   context are the same `ctx:` entity, so reading the filter is the reading
   *   that cannot be wrong for one of them. The link is what makes the note a
   *   member of the context (ADR-0008), which is why there is no membership
   *   write beside it.
   * * `captured-from` names the **foreground**, as `CONTEXT.md`'s **Passive
   *   attribution** defines that word and as the heartbeat computes it: the
   *   open detail, else the room's anchor, else nothing. It is *exactly* the
   *   timer's foreground rule (#278) and shares its one spelling,
   *   `timer.ts`'s `roomForeground` — that is the point, not an accident. The
   *   ticket's *"when a detail is open"* and story 42's *"when a detail was
   *   open"* name the first rung, which is the common case, and the word they
   *   use has a definition; the deputy's ruling of 2026-09-08 on #502 settles
   *   it that way.
   *
   *   A promoted room with nothing open therefore draws two links that share a
   *   name in the panel, and they are two facts: `captured-in` is which
   *   working set the note belongs to (ADR-0008), `captured-from` is what the
   *   note was about — the same split the timer in that room already makes
   *   when it runs on the anchor and not on the context (`contexts.ts`,
   *   *"Never the context's own id"*). What it buys is that the note and the
   *   day review's passive block for that minute name the same entity.
   *
   * **The two pushes live in `detail/relations.ts` since #503**, beside the two
   * constants and shared with the capture window, which makes the same pair
   * from a room and a foreground it remembered rather than ones it is drawing.
   * What is left here is this room's two answers, which is the part only a room
   * can give: the **filter's** context, and `roomForeground` over the open
   * detail and the anchor.
   */
  function bornHere(): NoteLinkInput[] {
    return bornWith({
      context: context.filter.context,
      foreground: roomForeground(detail?.entityId, context.anchorId),
    });
  }

  /**
   * Write a new note and open it.
   *
   * The row exists before the editor does, and that is the whole of story 2:
   * *New note* needs nothing typed, so there is nothing to lose between the
   * button and the first keystroke. What the reader then edits is a note that
   * is already saved — and already linked to where it was written, in the same
   * transaction, so it is never briefly a thought belonging to nothing.
   */
  async function startNote() {
    try {
      const written = await createNote(undefined, undefined, bornHere());
      router.go(
        hashFor({
          view: "room",
          ctx: context.id,
          detail: { kind: "note", entityId: written.note.id },
        }),
      );
    } catch (rejection) {
      push({ text: `Could not start a note: ${ipcErrorMessage(rejection)}`, tone: "err" });
    }
  }

  /**
   * Opening an item from a tile.
   *
   * Takes only the two fields the address is built from: a mini-board card is
   * not a mirror row and carries no timestamps, and a wider parameter would
   * make the tile invent them (#178).
   */
  function open(row: Pick<EntityRow, "kind" | "entity_id">) {
    router.go(hashFor({ view: "room", ctx: context.id, detail: { kind: row.kind, entityId: row.entity_id } }));
  }
</script>

<div class="room">
  <RoomBar {context} count={total} {inboxHere} onnewnote={() => void startNote()} />

  {#if error}
    <div class="empty">
      <p class="fail">{error}</p>
    </div>
  {:else if kinds === null}
    <div class="empty"><p class="muted">Reading the room…</p></div>
  {:else if tileCount === 0}
    <div class="empty">
      <p>Nothing synced into this room yet.</p>
      <p class="muted">
        Add a source and run a sync, and what it mirrors appears here as tiles.
      </p>
    </div>
  {:else}
    <!--
      A maximised tile (#250) is the only one drawn, and `.max` gives it the
      whole grid. Not drawn rather than hidden: a tile that is not on screen
      is not in this room's view, the way a kind the corpus lacks is not.
      Restoring mounts the others again and they read again, as they do on
      any room switch.
    -->
    <div class="tiles rows-{rows}" class:one={tileCount === 1} class:max={maximised !== null}>
      {#each tiles as spec (spec.id)}
        {#if maximised === null || maximised === spec.id}
          <Tile
            {spec}
            sources={context.filter.sources}
            ctx={context.filter.context}
            project={context.filter.project}
            miniBoardLayout={context.miniBoardLayout}
            {miniBoardOverride}
            maximised={maximised === spec.id}
            onopen={open}
            onlayout={(layout) => overrides.choose(context, layout)}
            onmaximise={() => toggleMaximise(spec.id)}
          />
        {/if}
      {/each}
      <!--
        Last in the grid, after the kind tiles: the estate is what the work in
        this room runs on, and a reader scanning a room reads the work first.
        Drawn under the same maximise gesture as any other tile (#250) — one
        id, the room's, so maximising the Assets tile restores like the rest.
      -->
      {#if drawsAssets && (maximised === null || maximised === ASSETS_TILE)}
        <AssetsTile
          filter={context.filter}
          maximised={maximised === ASSETS_TILE}
          onopen={(row) =>
            router.go(hashFor({ view: "assets", tab: "tree", assetId: row.asset.id }))}
          onmaximise={() => toggleMaximise(ASSETS_TILE)}
        />
      {/if}
    </div>
  {/if}

  <!--
    Under the board and above the slide-over: suggestions arrive where the work
    is rather than as a chore of their own (#41). The strip is `flex: none`, so
    the tiles keep the space they had and the tray takes only what it needs.
  -->
  <SuggestionTray
    sources={context.filter.sources}
    ctx={context.filter.context}
    onopen={(hash) => router.go(hash)}
  />

  <!--
    The slide-over is drawn *inside* the room, over its right-hand half
    (`.detail` is absolutely positioned against `.main`). Keyed on the entity
    id so opening a second item from behind the panel rebuilds it rather than
    leaving the previous one's focus capture in place.
  -->
  {#if detail}
    {#key detail.entityId}
      {#if isNote(detail.entityId, detail.kind)}
        <!--
          A note is an entity and opens where every entity opens; what it is
          not is a mirror row, so `Detail`'s source/payload/synced frame has
          nothing to fill (see `NoteView.svelte`).

          Decided on the **id**, not on the address's kind word, so the
          kind-agnostic `#/entity/<id>` alias lands in the right view too --
          there the kind is `null` until a read answers, and a note's read is
          not `get_entity`.
        -->
        <NoteView
          entityId={detail.entityId}
          contextLabel={context.label}
          onclose={() => router.back()}
          onnavigate={(hash) => router.go(hash)}
        />
      {:else}
        <Detail
          entityId={detail.entityId}
          kind={detail.kind}
          contextLabel={context.label}
          onclose={() => router.back()}
          onnavigate={(hash) => router.go(hash)}
        />
      {/if}
    {/key}
  {/if}
</div>
