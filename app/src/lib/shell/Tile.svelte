<!--
  One room tile, which owns its own read.

  The mockup swept every tile out of one `render()` over one blob of state
  (roadmap §5). That does not carry over: a tile is the thing that knows which
  kinds it draws, so it is the thing that asks for them. Six tiles are six
  round trips against a local PostgreSQL, and the alternative — one query the
  room slices up — puts the tile layout back into the room and makes adding a
  tile a change in two places.

  Two bodies since #178. Four tiles are lists of rows; Tickets is the **mini
  board** (ADR-0009), whose columns come from a read of its own. Both live here
  rather than in two components because everything around the body is the same
  fact in both cases — the header, the three states of a read, and the
  stale-answer guard that keeps a slow room switch from painting the room you
  just left.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../ipc";
  import {
    listEntities,
    miniBoard,
    type EntityPage,
    type EntityRow,
    type MiniBoard,
  } from "../ipc/entity";
  import EntityLine from "./EntityLine.svelte";
  import MiniBoardBody from "./MiniBoard.svelte";
  import type { TileSpec } from "./kinds";

  let {
    spec,
    sources,
    ctx = null,
    project = null,
    onopen,
  }: {
    spec: TileSpec;
    /** The room's source filter; `[]` is every source. */
    sources: string[];
    /** The stored context whose membership scopes this tile, or `null` (#47). */
    ctx?: string | null;
    /**
     * The project this room narrows to, or `null` for no scoping (#209).
     *
     * Narrows **within** `sources` and never instead of them: a project key is
     * unique only inside its own source, so a project room supplies both and a
     * tile that took only this one would draw another source's work under the
     * same key.
     */
    project?: string | null;
    /**
     * Opening an item. Narrowed to the two fields the room's router needs: a
     * board card is not a mirror row and has no timestamps, and inventing them
     * to satisfy a wider type would be worse than asking for less.
     */
    onopen: (row: Pick<EntityRow, "kind" | "entity_id">) => void;
  } = $props();

  /**
   * Which tile draws a mini board.
   *
   * Keyed on the bucket's stable id, and not on a `TileSpec` flag: the mini
   * board is *the Tickets tile's* rendering (spec #175, ADR-0009), not a mode
   * any tile could be put into, and a flag would advertise a generality
   * nothing has asked for. `kinds.ts` owns that id.
   */
  const MINI_BOARD_TILE = "tickets";
  const isMiniBoard = $derived(spec.id === MINI_BOARD_TILE);

  /**
   * How many rows a tile holds.
   *
   * The tile scrolls (`.tile-b { overflow: auto }`), and `total` in the header
   * says how many there are in all, so this is a window rather than a cap on
   * the truth.
   */
  const PAGE = 50;

  let page = $state<EntityPage | null>(null);
  let board = $state<MiniBoard | null>(null);
  let error = $state<string | null>(null);

  /**
   * The generation of the newest request.
   *
   * A slow tile must not overwrite a newer context's answer: switching rooms
   * twice quickly leaves two reads in flight, and without this the one that
   * started first can land last. Plain, not a rune — writing it is not a
   * render.
   */
  let token = 0;

  $effect(() => {
    const mine = ++token;
    const filter = {
      sources,
      kinds: spec.kinds,
      updated_within_days: null,
      context: ctx,
      project,
      order: "updated_desc" as const,
      include_deleted: false,
    };
    // Cleared before the request, not after it: a tile showing the previous
    // room's rows while the new ones load is showing rows that are not in this
    // room at all.
    page = null;
    board = null;
    error = null;
    // The board asks for the room's narrowing dimensions and nothing else: it
    // is tickets by construction, it is never paged, and its order is the
    // command's (#177).
    const read = isMiniBoard
      ? miniBoard({ sources, context: ctx, project }).then((answer) => {
          if (mine === token) board = answer;
        })
      : listEntities(filter, PAGE, 0).then((answer) => {
          if (mine === token) page = answer;
        });
    void read.catch((rejection) => {
      if (mine !== token) return;
      // Shown in the tile, not swallowed: a tile that silently renders empty
      // on a failed read is indistinguishable from a tile with nothing in it.
      error = ipcErrorMessage(rejection);
    });
  });

  /**
   * The header's count.
   *
   * For a list it is the whole filtered set, which is wider than the page on
   * screen. For the board it is every card drawn — the board is not paged, so
   * the two are the same number, and summing the columns keeps the header and
   * the columns under it one fact rather than two reads that could disagree.
   */
  const count = $derived.by(() => {
    if (isMiniBoard) {
      return board ? board.columns.reduce((all, column) => all + column.cards.length, 0) : null;
    }
    return page ? page.total : null;
  });

  /** Whether the read has answered at all — the "Reading…" state. */
  const answered = $derived(isMiniBoard ? board !== null : page !== null);
  const nothing = $derived(isMiniBoard ? board?.columns.length === 0 : page?.rows.length === 0);

  /**
   * What an empty tile says.
   *
   * The mockup's wording where it still applies, rewritten where it promised
   * an action M1 does not have — `signal-miller.html:2380,2416,2428,2437,2446`
   * offer *New ticket*, *Git…*, *Trigger build…*, all M2 write-backs. An empty
   * tile in M1 says what is missing and stops there.
   */
  const EMPTY: Record<string, string> = {
    tickets: "No ticket in this room yet.",
    code: "No pull request, commit or repository belongs to this room.",
    builds: "No pipeline runs in this room.",
    docs: "No page in this room yet.",
    notes: "No note in this room yet.",
  };
  const empty = $derived(EMPTY[spec.id] ?? `Nothing of this kind in this room yet.`);
</script>

<section class="tile">
  <div class="tile-h">
    <span class="lab">{spec.label}</span>
    <span class="cnt">{count ?? ""}</span>
    <span class="acts"></span>
  </div>
  <div class="tile-b">
    {#if error}
      <!-- Text: an `IpcError.message` can carry whatever a source said. -->
      <div class="empty"><p class="fail">{error}</p></div>
    {:else if !answered}
      <div class="empty"><p class="muted">Reading…</p></div>
    {:else if nothing}
      <div class="empty"><p>{empty}</p></div>
    {:else if board}
      <MiniBoardBody {board} {onopen} />
    {:else if page}
      {#each page.rows as row (row.entity_id)}
        <EntityLine {row} {onopen} />
      {/each}
    {/if}
  </div>
</section>
