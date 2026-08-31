<!--
  The room: a heading strip and the tile board under it
  (`signal-miller.html:2519-2529`).

  One read of its own, and it does two jobs: which kinds are present here — so
  a room only draws tiles it has something to put in (§3a open kinds) — and how
  many items the room holds, which is the heading's count. Every tile then
  fetches its own page.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../ipc";
  import { inbox } from "../inbox/inbox.svelte";
  import { contextMembers } from "../ipc/entity";
  import { push } from "./toasts.svelte";
  import Detail from "../detail/Detail.svelte";
  import NoteView from "../notes/NoteView.svelte";
  import { createNote, listEntities, type EntityRow } from "../ipc/entity";
  import RoomBar from "./RoomBar.svelte";
  import SuggestionTray from "./SuggestionTray.svelte";
  import Tile from "./Tile.svelte";
  import { contextById, type RoomContext } from "./contexts";
  import { kindRegistry } from "./kind-registry.svelte";
  import { tilesFor } from "./kinds";
  import { hashFor, type Router } from "./router.svelte";

  let { router, contexts }: { router: Router; contexts: RoomContext[] } = $props();

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
  /** The entity the address has open over this room, if any. */
  const detail = $derived(router.route.view === "room" ? router.route.detail : null);

  let kinds = $state<string[] | null>(null);
  let total = $state<number | null>(null);
  let error = $state<string | null>(null);
  let token = 0;

  $effect(() => {
    const { sources, context: ctx } = context.filter;
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
        // Unscoped, for the reason `Tile.svelte` states: no room narrows by a
        // project until #209 adds the rooms that do.
        project: null,
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
   * Two columns, so the row count is half the tiles — capped at the four the
   * stylesheet declares (`.tiles.rows-*`), past which the board scrolls rather
   * than growing rows nobody can see.
   */
  const rows = $derived(Math.min(4, Math.max(1, Math.ceil(tiles.length / 2))));

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
   * Write a new note and open it.
   *
   * The row exists before the editor does, and that is the whole of story 2:
   * `create_note` takes no arguments, so there is nothing to lose between
   * *New note* and the first keystroke. What the reader then edits is a note
   * that is already saved.
   */
  async function startNote() {
    try {
      const written = await createNote();
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
  {:else if tiles.length === 0}
    <div class="empty">
      <p>Nothing synced into this room yet.</p>
      <p class="muted">
        Add a source and run a sync, and what it mirrors appears here as tiles.
      </p>
    </div>
  {:else}
    <div class="tiles rows-{rows} {tiles.length === 1 ? 'one' : ''}">
      {#each tiles as spec (spec.id)}
        <Tile {spec} sources={context.filter.sources} ctx={context.filter.context} onopen={open} />
      {/each}
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
