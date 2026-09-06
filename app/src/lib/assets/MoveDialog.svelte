<!--
  *Move to…* — the parent picker (issue #429, spec #427 story 3).

  An asset's place is one parent field (ADR-0014), so moving it is choosing a
  parent and nothing else. The picker walks the estate the way the Tree does —
  one read per level, `assetTree(parentId)` — because the estate is a tree of
  unknown depth and a flat list of every asset is the thing Miller columns
  exist to avoid.

  **Where you have walked to is what you are moving into.** There is no
  separate "target" to select: the breadcrumb says where the picker is standing
  and *Move here* puts the asset there, which makes the top of the estate an
  ordinary destination (walk back to *Estate*) rather than a special button.

  ## The picker does not hide the moves the backend refuses

  A reader can walk into the asset's own subtree and press *Move here*, and
  `move_asset` refuses it with the sentence it has for exactly this — naming
  both ends **and the asset that closes the loop**. Two reasons that is right
  rather than lazy:

  * Hiding those rows means walking the ancestors in the frontend, which is a
    second copy of `assets::cycle_through` — and the copy that goes stale.
  * The refusal is the only place the loop is *explained*. A row that quietly
    was not there teaches a reader nothing about why.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../ipc";
  import type { assetTree, AssetRow, moveAsset } from "../ipc/assets";
  import { latestRead } from "../shell/latest-read";
  import Modal from "../shell/Modal.svelte";

  let {
    asset,
    heldBy,
    tree,
    move,
    onclose,
    onmoved,
  }: {
    /** The asset being moved — its name and its current parent. */
    asset: AssetRow;
    /** Its ancestors, outermost first — `AssetDetail.held_by`. */
    heldBy: AssetRow[];
    tree: typeof assetTree;
    move: typeof moveAsset;
    onclose: () => void;
    /** The move was written. The Tree re-reads at the new path. */
    onmoved: (row: AssetRow) => void;
  } = $props();

  // svelte-ignore state_referenced_locally
  // Read once, at init, and the two are one fact: the picker opens **where the
  // asset already is**, so moving it one branch sideways is one click rather
  // than a walk down from the top. `heldBy` is that path and `at` is its foot,
  // which is why they are seeded together and never separately.
  let crumb = $state<AssetRow[]>([...heldBy]);
  // svelte-ignore state_referenced_locally
  let at = $state<string | null>(asset.parent_id);
  let rows = $state<AssetRow[]>([]);
  let failure = $state<string | null>(null);
  let writing = $state(false);
  /** Nothing has come back yet — told apart from a level holding nothing. */
  let loaded = $state(false);

  const columnRead = latestRead<AssetRow[]>();

  /** Where the picker is standing, in words. */
  const here = $derived(crumb.at(-1)?.name ?? "the top of the estate");

  /**
   * The level the picker is standing on.
   *
   * Driven off `at` rather than off the click that set it, for the Tree's own
   * reason: one place decides what is on screen, so a click and a *Up* cannot
   * leave the crumb and the rows disagreeing.
   */
  $effect(() => {
    const parent = at;
    void columnRead(() => tree(parent), {
      ok: (answer) => {
        rows = answer;
        failure = null;
        loaded = true;
      },
      fail: (cause) => {
        rows = [];
        failure = ipcErrorMessage(cause);
        loaded = true;
      },
    });
  });

  /** Stand on `row`: it becomes the destination and the level below is drawn. */
  function into(row: AssetRow) {
    stand([...crumb, row]);
  }

  /** Stand at the foot of `path` — the top of the estate when it is empty. */
  function stand(path: AssetRow[]) {
    crumb = path;
    at = path.at(-1)?.id ?? null;
    failure = null;
  }

  async function submit() {
    if (writing) return;
    writing = true;
    failure = null;
    try {
      onmoved(await move(asset.id, at));
    } catch (rejection) {
      // The cycle refusal lands here, in the words `assets::move_to` chose:
      // it names the asset that closes the loop, which is the fact a reader
      // can act on.
      failure = ipcErrorMessage(rejection);
    } finally {
      writing = false;
    }
  }
</script>

<Modal title="Move to…" subtitle={asset.name} {onclose}>
  {#snippet body()}
    <div class="picker">
      <p class="crumb mono">
        <button class="link" disabled={crumb.length === 0} onclick={() => stand([])}>
          Estate
        </button>
        {#each crumb as step, index (step.id)}
          <span class="sep" aria-hidden="true">/</span>
          <button
            class="link"
            disabled={index === crumb.length - 1}
            onclick={() => stand(crumb.slice(0, index + 1))}
          >
            {step.name}
          </button>
        {/each}
      </p>

      <div class="lvl">
        <ul class="opts" aria-label="Where to move it">
          {#each rows as row (row.id)}
            <li>
              <!--
                Every row is a destination, **including a leaf**: making an
                asset that holds nothing hold something is the commonest move
                there is, and a picker that only opened branches could not do
                it. The chevron still says which rows have a level under them.
              -->
              <button class="opt" title="Open {row.name}" onclick={() => into(row)}>
                <span class="mg">{row.monogram}</span>
                <span class="nm">{row.name}</span>
                {#if row.has_children}
                  <span class="chev" aria-hidden="true">›</span>
                {/if}
              </button>
            </li>
          {/each}
        </ul>
        {#if loaded && rows.length === 0}
          <p class="empty">Nothing here yet.</p>
        {/if}
      </div>

      {#if failure}
        <p class="fail" role="alert">{failure}</p>
      {/if}
    </div>
  {/snippet}

  {#snippet footer()}
    <button class="btn" onclick={onclose}>Cancel</button>
    <button class="btn pri" disabled={writing} onclick={() => void submit()}>
      {writing ? "Moving…" : `Move into ${here}`}
    </button>
  {/snippet}
</Modal>

<style>
  .picker {
    display: grid;
    gap: 10px;
  }

  .crumb {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 4px;
    margin: 0;
    font-size: 11px;
    color: var(--muted);
  }

  .crumb button:disabled {
    color: var(--text);
    cursor: default;
  }

  /* Bounded: a level with fifty rows must not push the footer buttons out of
     reach, which is `LinkDialog`'s rule for its own result list. */
  .lvl {
    max-height: 240px;
    overflow: auto;
    border: 1px solid var(--hair);
    border-radius: 2px;
  }

  .opts {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .opt {
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

  .opt:hover {
    background: var(--raised);
  }

  .opt .nm {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .opt .mg {
    flex: none;
    font: 500 10px var(--mono);
    color: var(--faint);
  }

  .opt .chev {
    flex: none;
    color: var(--faint);
  }

  .empty {
    padding: 8px 10px;
    font-size: 12px;
    color: var(--muted);
  }
</style>
