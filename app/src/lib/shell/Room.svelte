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
  import Detail from "../detail/Detail.svelte";
  import { listEntities, type EntityRow } from "../ipc/entity";
  import RoomBar from "./RoomBar.svelte";
  import Tile from "./Tile.svelte";
  import { contextById, type RoomContext } from "./contexts";
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
    const sources = context.filter.sources;
    const mine = ++token;
    kinds = null;
    total = null;
    error = null;
    void listEntities(
      {
        sources,
        kinds: [],
        updated_within_days: null,
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

  const tiles = $derived(kinds === null ? [] : tilesFor(kinds));
  /**
   * Two columns, so the row count is half the tiles — capped at the four the
   * stylesheet declares (`.tiles.rows-*`), past which the board scrolls rather
   * than growing rows nobody can see.
   */
  const rows = $derived(Math.min(4, Math.max(1, Math.ceil(tiles.length / 2))));

  function open(row: EntityRow) {
    router.go(hashFor({ view: "room", ctx: context.id, detail: { kind: row.kind, entityId: row.entity_id } }));
  }
</script>

<div class="room">
  <RoomBar {context} count={total} />

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
        <Tile {spec} sources={context.filter.sources} onopen={open} />
      {/each}
    </div>
  {/if}

  <!--
    The slide-over is drawn *inside* the room, over its right-hand half
    (`.detail` is absolutely positioned against `.main`). Keyed on the entity
    id so opening a second item from behind the panel rebuilds it rather than
    leaving the previous one's focus capture in place.
  -->
  {#if detail}
    {#key detail.entityId}
      <Detail
        entityId={detail.entityId}
        kind={detail.kind}
        contextLabel={context.label}
        onclose={() => router.back()}
      />
    {/key}
  {/if}
</div>
