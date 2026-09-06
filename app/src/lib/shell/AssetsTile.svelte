<!--
  The room's Assets tile (spec #427 stories 41 and 42, issue #434) —
  `signal-miller.html:1828-1851`.

  **It owns its own read, like every other tile**, and it is handed the room's
  whole filter rather than a context id: which read this room gets is
  `assets-tile.ts`'s rule, and a tile told only "here is a context" could not
  have grown the derived rooms' cases (#435) without the room learning the rule
  too.

  **The membership is the backend's one statement** (ADR-0008, §16.11), not a
  filter drawn here. An asset is a member through its **ancestors**, so a VM
  somebody added brings the containers it holds, and a container linked to a
  member ticket brings its services — which is what "the tile fills itself"
  means and why there is nothing to add by hand for it to work.

  Each row is the asset, where it sits, and how it is: the path is the thing
  the Tree's own columns do not draw, and it is here because a flat list of
  twenty names with no path is a list of things a reader cannot place.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../ipc";
  import { contextAssets as realContextAssets, type MemberAsset } from "../ipc/assets";
  import { assetsTileRead, type RoomFilter } from "./assets-tile";
  import Monogram from "./Monogram.svelte";

  /**
   * The bridge, injectable — the shape `AssetsView`'s `AssetPorts` has, and
   * for its reason: a test of this tile needs no Tauri and no database.
   */
  interface AssetsTilePorts {
    contextAssets: typeof realContextAssets;
  }

  let {
    filter,
    ports,
    maximised = false,
    onopen,
    onmaximise,
  }: {
    /** The room's own filter — the whole of what decides this tile's read. */
    filter: RoomFilter;
    ports?: Partial<AssetsTilePorts>;
    /** Whether the room has maximised this tile (#250). */
    maximised?: boolean;
    /** Opening an asset: the room turns the id into an address. */
    onopen: (row: MemberAsset) => void;
    onmaximise: () => void;
  } = $props();

  const read = $derived(ports?.contextAssets ?? realContextAssets);

  /**
   * The context this tile reads, or `null` where the room's rule gives it no
   * read at all.
   *
   * A **string** and not the rule's own answer, because the effect below reads
   * it and the shell hands the room a freshly derived filter several times a
   * session (`Room.svelte`, `roomId`): a derived object is a new object every
   * time, so the effect would re-issue the read for a room that has not
   * changed, where a derived string is equal to itself and propagates
   * nothing.
   */
  const ctx = $derived.by(() => {
    const want = assetsTileRead(filter);
    return want.kind === "members" ? want.ctx : null;
  });

  let rows = $state<MemberAsset[] | null>(null);
  let error = $state<string | null>(null);

  /**
   * The generation of the newest request — `Tile.svelte`'s guard, for its
   * reason: switching rooms twice quickly leaves two reads in flight, and
   * without this the one that started first can land last.
   */
  let token = 0;

  $effect(() => {
    const want = ctx;
    const mine = ++token;
    // Cleared before the request: a tile showing the previous room's assets
    // while the new ones load is showing assets that are not in this room.
    rows = null;
    error = null;
    if (want === null) {
      // Not reachable while the room only mounts this tile for a stored room,
      // and still written: the room's rule and the tile's read are the same
      // function, so a tile mounted for a room with no read draws nothing
      // rather than reading for a context it does not have.
      rows = [];
      return;
    }
    void read(want)
      .then((answer) => {
        if (mine === token) rows = answer;
      })
      .catch((rejection) => {
        if (mine !== token) return;
        // Shown, never swallowed: a tile that renders empty on a failed read
        // is indistinguishable from a room with nothing in it.
        error = ipcErrorMessage(rejection);
      });
  });

  const count = $derived(rows === null ? null : rows.length);

  /**
   * How many members are unwell — the header's second clause.
   *
   * Off the rows already drawn rather than a count of its own, so the header
   * and the lamps under it cannot disagree. `health` and not `status`: a
   * healthy VM holding a dead container is a problem in this room, and the
   * rollup is what says so.
   */
  const wrong = $derived.by(() => {
    if (rows === null) return { down: 0, warn: 0 };
    return {
      down: rows.filter((row) => row.asset.health === "down").length,
      warn: rows.filter((row) => row.asset.health === "warn").length,
    };
  });

  const summary = $derived.by(() => {
    if (count === null) return "";
    if (count === 0) return "";
    if (wrong.down > 0) return `${count} here · ${wrong.down} down`;
    if (wrong.warn > 0) return `${count} here · ${wrong.warn} warning`;
    return `${count} here`;
  });
</script>

<section class="tile">
  <div class="tile-h">
    <span class="lab">Assets</span>
    <span class="cnt">{summary}</span>
    <span class="acts">
      <button class="btn sm ghost tile-max" onclick={onmaximise}>
        {maximised ? "Restore" : "Maximise"}
      </button>
    </span>
  </div>
  <div class="tile-b">
    {#if error}
      <!-- Text: an `IpcError.message` can carry whatever the backend said. -->
      <div class="empty"><p class="fail">{error}</p></div>
    {:else if rows === null}
      <div class="empty"><p class="muted">Reading…</p></div>
    {:else if rows.length === 0}
      <div class="empty">
        <p>No machine, container or database belongs to this context yet.</p>
        <p class="muted">
          Link one to a ticket in this room, or add it to the context, and it appears here with
          whatever it holds.
        </p>
      </div>
    {:else}
      {#each rows as row (row.asset.id)}
        <button class="arow" onclick={() => onopen(row)}>
          <span class="lamp {row.asset.health}" aria-hidden="true"></span>
          <Monogram text={row.asset.monogram} label={row.asset.type_label} />
          <span class="nm">{row.asset.name}</span>
          <!--
            "top level" rather than an empty cell: an asset at the top of the
            estate has a place, and a blank column reads as a value that failed
            to load.
          -->
          <span class="pth">{row.path ?? "top level"}</span>
          <span class="hl {row.asset.health}">{row.asset.health}</span>
        </button>
      {/each}
    {/if}
  </div>
</section>

<style>
  /* Five columns: the lamp, the type chip, the name, where it sits, and how it
     is. The path takes what the name does not, and both ellipsise — a tile is
     half the window wide and a container's path is longer than its name. */
  .arow {
    display: grid;
    grid-template-columns: 6px 22px minmax(0, 1fr) minmax(0, 1.4fr) 42px;
    align-items: center;
    gap: 8px;
    width: 100%;
    height: var(--row);
    padding: 0 12px;
    text-align: left;
    color: var(--text);
    font-size: 12px;
  }

  .arow:hover {
    background: var(--raised);
  }

  .nm,
  .pth {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .pth {
    color: var(--muted);
    font-family: var(--mono);
    font-size: 11px;
  }

  /* The lamp carries no text, so it carries no meaning on its own: the word
     beside it is the reading, and this is the thing the eye finds first in a
     column of twenty. */
  .lamp {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--hair2);
  }

  .lamp.up {
    background: var(--ok);
  }

  .lamp.warn {
    background: var(--amber);
  }

  .lamp.down {
    background: var(--fail);
  }

  .hl {
    font-family: var(--mono);
    font-size: 10px;
    text-align: right;
    color: var(--faint);
  }

  .hl.up {
    color: var(--ok);
  }

  .hl.warn {
    color: var(--amber);
  }

  .hl.down {
    color: var(--fail);
  }
</style>
