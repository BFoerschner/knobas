<!--
  The room's Assets tile (spec #427 stories 41 to 45, issues #434 and #435) —
  `signal-miller.html:1828-1851`.

  **It owns its own read, like every other tile**, and it is handed the room's
  whole filter rather than a context id: which read this room gets is
  `assets-tile.ts`'s rule, and a tile told only "here is a context" could not
  have grown the derived rooms' cases without the room learning the rule too.

  **Four kinds of room, three reads, one list.** A stored room lists its member
  assets, *All work* lists the estate's top level, a source room lists what
  that source's monitors watch, and a project room draws no tile at all. Each
  read answers with the same rows — an asset, and where it sits — so the body
  below is one list whatever the room, and what changes with the room is the
  question, the count line's word and what an empty answer means.

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
  import { untrack } from "svelte";

  import { problemBadge } from "../assets/tree";
  import { ipcErrorMessage } from "../ipc";
  import {
    assetTree as realAssetTree,
    contextAssets as realContextAssets,
    sourceAssets as realSourceAssets,
    type MemberAsset,
  } from "../ipc/assets";
  import { assetsTileRead, type AssetsTileRead, type RoomFilter } from "./assets-tile";
  import Monogram from "./Monogram.svelte";

  /**
   * The bridge, injectable — the shape `AssetsView`'s `AssetPorts` has, and
   * for its reason: a test of this tile needs no Tauri and no database.
   */
  interface AssetsTilePorts {
    contextAssets: typeof realContextAssets;
    assetTree: typeof realAssetTree;
    sourceAssets: typeof realSourceAssets;
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

  const io = $derived({
    contextAssets: ports?.contextAssets ?? realContextAssets,
    assetTree: ports?.assetTree ?? realAssetTree,
    sourceAssets: ports?.sourceAssets ?? realSourceAssets,
  });

  /** The room's read, as the rule answers it. One representation, kept. */
  const read = $derived(assetsTileRead(filter));

  /**
   * The same answer as a **string**, which is the only thing the effect below
   * depends on.
   *
   * The shell hands the room a freshly derived filter several times a session
   * (`Room.svelte`, `roomId`), and a derived object is a new object every
   * time: an effect depending on one would re-issue the read for a room that
   * has not changed, where a derived string is equal to itself and propagates
   * nothing. So the key is the dependency and the union itself is taken
   * `untrack`ed — one answer, read two ways, rather than an encoding somebody
   * has to parse back.
   */
  const want = $derived(keyOf(read));

  /** Two rooms with the same key read the same thing. Exhaustive on purpose:
   * a fifth member of the union has to fail to compile here. */
  function keyOf(of: AssetsTileRead): string {
    switch (of.kind) {
      case "members":
        return `members ${of.ctx}`;
      case "source":
        return `source ${of.source}`;
      case "roots":
        return "roots";
      case "none":
        return "none";
    }
  }

  /**
   * What the room's read answers with.
   *
   * *All work*'s is `assetTree(null)`, the Tree's own first column, rather
   * than a command of its own: story 43 asks for **the estate's top level with
   * problem counts**, which is exactly that read's answer, and a second
   * backend statement saying the same thing is a second statement to keep in
   * step. The path is `null` on every row because a root sits nowhere, and the
   * row draws *top level* for it — the same words a member asset at the top of
   * the estate draws.
   */
  function issue(of: AssetsTileRead): Promise<MemberAsset[]> {
    switch (of.kind) {
      case "members":
        return io.contextAssets(of.ctx);
      case "source":
        return io.sourceAssets(of.source);
      case "roots":
        return io.assetTree(null).then((rows) => rows.map((asset) => ({ asset, path: null })));
      case "none":
        return Promise.resolve([]);
    }
  }

  let rows = $state<MemberAsset[] | null>(null);
  let error = $state<string | null>(null);

  /**
   * The generation of the newest request — `Tile.svelte`'s guard, for its
   * reason: switching rooms twice quickly leaves two reads in flight, and
   * without this the one that started first can land last.
   */
  let token = 0;

  $effect(() => {
    // The key is the dependency; the union it stands for is not, or a fresh
    // filter object for the same room would re-issue the read.
    void want;
    const asked = untrack(() => read);
    const mine = ++token;
    // Cleared before the request: a tile showing the previous room's assets
    // while the new ones load is showing assets that are not in this room.
    rows = null;
    error = null;
    if (asked.kind === "none") {
      // Not reachable while the room only mounts this tile for a room the rule
      // gives a read, and still written: the room's rule and the tile's read
      // are the same function, so a tile mounted for a room with no read draws
      // nothing rather than reading for a scope it does not have.
      rows = [];
      return;
    }
    void issue(asked)
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
      <!--
        One empty state per kind of room, because an empty tile has a different
        *cause* in each and a single sentence would be wrong in two of them: a
        stored room is empty until somebody links something, *All work* is
        empty only when the estate itself is, and a source room is empty until
        its monitors exist. The second line is what to do about it, and there
        is nothing to do about the third.
      -->
      <div class="empty">
        {#if read.kind === "members"}
          <p>No machine, container or database belongs to this context yet.</p>
          <p class="muted">
            Link one to a ticket in this room, or add it to the context, and it appears here with
            whatever it holds.
          </p>
        {:else if read.kind === "source"}
          <p>No asset is monitored by {read.source} yet.</p>
          <p class="muted">
            An asset appears here once one of this source's monitors is linked to it.
          </p>
        {:else}
          <p>Nothing is in the estate yet.</p>
          <p class="muted">Add a site or a server in the Assets view and it appears here.</p>
        {/if}
      </div>
    {:else}
      {#each rows as row (row.asset.id)}
        {@const badge = problemBadge(row.asset)}
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
          <!--
            Story 43's *with problem counts*, and the Tree's own badge rule
            (`assets/tree.ts`) rather than a second one: a row here is a row
            there, and *All work*'s tile draws the estate's top level, where
            the whole point of a root is what is underneath it. Absent at zero,
            and coloured by `inside` — so a `down` VM holding one `warn`
            container is a red lamp beside an amber badge.
          -->
          {#if badge}
            <span
              class="badge {badge.tone}"
              title="{badge.count} {badge.count === 1 ? 'problem' : 'problems'} inside"
            >{badge.count}</span>
          {:else}
            <span></span>
          {/if}
          <span class="hl {row.asset.health}">{row.asset.health}</span>
        </button>
      {/each}
    {/if}
  </div>
</section>

<style>
  /* Six columns: the lamp, the type chip, the name, where it sits, what is
     wrong inside it, and how it is. The path takes what the name does not, and
     both ellipsise — a tile is half the window wide and a container's path is
     longer than its name. The badge column is fixed rather than `auto` so a
     row with a badge and a row without draw their health in the same place;
     an empty span holds it open. */
  .arow {
    display: grid;
    grid-template-columns: 6px 22px minmax(0, 1fr) minmax(0, 1.4fr) 22px 42px;
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

  /* The Tree's own badge, at tile scale: amber or red, hollow, and the number
     monospaced so a column of them lines up at a glance. */
  .badge {
    min-width: 16px;
    padding: 0 3px;
    border: 1px solid currentColor;
    border-radius: 8px;
    font-family: var(--mono);
    font-size: 10px;
    line-height: 13px;
    text-align: center;
  }

  .badge.warn {
    color: var(--amber);
  }

  .badge.down {
    color: var(--fail);
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
