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

  **What this ticket does not draw, and why the gaps are gaps rather than
  stubs.** Creating and editing from the pane is #429 — a `+` on a column
  header with no dialog behind it is a promise the view cannot keep. The
  keyboard walk, the spine collapse and the search that reveals a path are
  #430. Inherited environment and owner, the *N problems inside* badge and the
  *open URL* / *copy SSH* actions are #431; the pane shows the value **set on
  this asset**, which is what that walk will read. Routes, wires, *Link to…*,
  the linked-work badge and monitoring are #432, #433, #435 and M4.1. The
  Monitors tab is M4.1's, so the tab strip has one tab in it: a disabled
  sibling would teach the reader only that the app is unfinished.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../ipc";
  import {
    assetTree as realAssetTree,
    getAsset as realGetAsset,
    type AssetDetail,
    type AssetRow,
    type AssetProperty,
  } from "../ipc/assets";
  import { latestRead } from "../shell/latest-read";
  import { ago } from "../shell/time";
  import { hashFor, type Router } from "../shell/router.svelte";
  import { addressOf, columnPathFor, emptyPath, heldByPath, type ColumnPath } from "./tree";

  /**
   * The bridge this view needs, injectable so a test needs no Tauri — the
   * shape `StandupView`'s `StandupPorts` and `DayReview`'s `DayPorts` have.
   */
  interface AssetPorts {
    assetTree: typeof realAssetTree;
    getAsset: typeof realGetAsset;
  }

  let {
    router,
    ports,
    now = () => new Date(),
  }: {
    router: Router;
    ports?: Partial<AssetPorts>;
    /** Injectable clock — what the history's *ago* readings are relative to. */
    now?: () => Date;
  } = $props();

  // svelte-ignore state_referenced_locally
  // Read once, at init: production omits this prop, and a bridge swapped
  // mid-life would leave what is on screen read through one set of ports and
  // re-read through another.
  const io: AssetPorts = { assetTree: realAssetTree, getAsset: realGetAsset, ...ports };

  /** The asset the address names, or `null` for the bare `#/assets/tree`. */
  const selectedId = $derived(
    router.route.view === "assets" ? router.route.assetId : null,
  );

  /** The pane's read. `null` until something is selected and read. */
  let detail = $state<AssetDetail | null>(null);
  /** One list per column, aligned with `path.parents`. */
  let columns = $state<AssetRow[][]>([]);
  let failure = $state<string | null>(null);
  /** Nothing has come back yet — told apart from "an estate with nothing in it". */
  let loaded = $state(false);

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

  /** The address names an asset: read it, or clear the pane when it names none. */
  $effect(() => {
    const id = selectedId;
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
    if (verb === "moved") return `Moved to ${said(fields.to)}`;
    const field = fields.field === "property" ? said(fields.key) : said(fields.field);
    return `${field}: ${said(fields.from)} → ${said(fields.to)}`;
  }

  /** A property's value as text. `null` is drawn as an em dash, not as "null". */
  function reading(property: AssetProperty): string {
    if (property.value === null) return "—";
    return String(property.value.value);
  }
</script>

<section class="view">
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

  <div class="tree">
    <div class="cols">
      {#if failure}
        <p class="empty fail">{failure}</p>
      {:else if loaded && columns.length > 0 && (columns[0]?.length ?? 0) === 0}
        <!--
          The estate with nothing in it, which is what a fresh install has
          until the import (#439) or a hand-created asset. It says so in its
          own words rather than drawing an empty column the reader would take
          for a failed read.
        -->
        <p class="empty">Nothing in the estate yet.</p>
      {:else}
        {#each columns as column, index (index)}
          <ol class="col">
            {#each column as row (row.id)}
              <li>
                <button
                  class="row {path.selected[index] === row.id ? 'on' : ''}"
                  aria-current={path.selected[index] === row.id ? "true" : undefined}
                  title={addressOf(row)}
                  onclick={() => select(row)}
                >
                  <span class="mg" title={row.type_label}>{row.monogram}</span>
                  <span class="nm">{row.name}</span>
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
          <h2>{detail.asset.name}</h2>
          <span class="kind">{detail.asset.type_label}</span>
        </header>

        <p class="crumb mono" title="Where this asset sits in the estate">
          {heldByPath(detail).join(" / ")}
        </p>

        <section class="grp">
          <h3 class="lab">Properties</h3>
          <dl class="props">
            {#each detail.properties as property (property.key)}
              <div class="prop {property.custom ? 'own' : ''}">
                <dt>{property.label}</dt>
                <dd class={property.value === null ? "faint" : ""}>{reading(property)}</dd>
              </div>
            {/each}
          </dl>
          {#if detail.properties.length === 0}
            <p class="empty">This type declares no properties, and none were added.</p>
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

<style>
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

  .col {
    display: flex;
    flex-direction: column;
    gap: 1px;
    width: 220px;
    flex: none;
    margin: 0;
    padding: 6px 0;
    list-style: none;
    border-right: 1px solid var(--hair);
    overflow-y: auto;
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
    margin: 0;
    font-family: var(--mono);
    overflow-wrap: anywhere;
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
