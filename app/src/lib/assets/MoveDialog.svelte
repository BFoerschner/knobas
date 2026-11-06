<!--
  *Move to…* — the parent picker (issue #429, spec #427 story 3).

  An asset's place is one parent field (ADR-0014), so moving it is choosing a
  parent and nothing else. The picker walks the estate the way the Tree does —
  one read per level, `assetTree(parentId)` — because the estate is a tree of
  unknown depth and a flat list of every asset is the thing Miller columns
  exist to avoid.

  **Where you have walked to is what you are moving into.** There is no
  separate "target" to select: the breadcrumb says where the picker is standing
  and *Move into …* puts the asset there, which makes the top of the estate an
  ordinary destination (walk back to *Estate*) rather than a special button.

  ## The asset's own row cannot be walked into

  It stays in the list, marked *this asset*, and it is not a button that does
  anything: walking into it is the only way to reach its subtree, so one
  disabled row closes every move that would make a loop, with no walk over the
  ancestors here. `assets::cycle_through` stays the rule. If the estate changes
  under an open picker, `move_asset` refuses in its own words and the sentence
  lands in the dialog.
-->
<script lang="ts">
  import { tick } from "svelte";

  import { levelKeys } from "./level-keys";
  import PickerCrumb from "./PickerCrumb.svelte";
  import { kindTag } from "./tree";
  import { ipcErrorMessage } from "../ipc";
  import type { assetTree, AssetRow, moveAsset } from "../ipc/assets";
  import { latestRead } from "../shell/latest-read";
  import TfDialog from "../tf/settings/TfDialog.svelte";

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
    /** The move was written, into the place named in words. The Tree re-reads at the new path. */
    onmoved: (into: string) => void;
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
  let picker = $state<HTMLElement | null>(null);
  /** A walk asked for this read, so focus goes on to the level it draws. */
  let walked = false;
  /** Going up, the row walked out of, which focus goes back to. */
  let cameFrom: string | null = null;

  const columnRead = latestRead<AssetRow[]>();

  /** Where the picker is standing, in words. */
  const here = $derived(crumb.at(-1)?.name ?? "the top of the estate");
  /** Standing on the asset's own parent: moving it there would change nothing. */
  const alreadyHere = $derived(at === asset.parent_id);

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
        if (walked) void tick().then(() => land(cameFrom));
        walked = false;
      },
      fail: (cause) => {
        rows = [];
        failure = ipcErrorMessage(cause);
        loaded = true;
      },
    });
  });

  /** Focus on the level just drawn: the row walked out of, else its first, else the list when the level holds nothing, so focus never falls out of it. */
  function land(row: string | null) {
    const options = [...(picker?.querySelectorAll<HTMLElement>(".opt:not(:disabled)") ?? [])];
    const spot = options.find((option) => option.dataset.row === row) ?? options[0] ?? picker?.querySelector<HTMLElement>(".opts");
    spot?.focus();
  }

  /** Stand on `row`: it becomes the destination and the level below is drawn. */
  function into(row: AssetRow) {
    stand([...crumb, row]);
  }

  /** `←`: up a level, back on the row walked out of. */
  function up() {
    if (crumb.length > 0) stand(crumb.slice(0, -1), crumb.at(-1)?.id ?? null);
  }

  /** Stand at the foot of `path` — the top of the estate when it is empty. */
  function stand(path: AssetRow[], from: string | null = null) {
    walked = true;
    cameFrom = from;
    crumb = path;
    at = path.at(-1)?.id ?? null;
    failure = null;
  }

  async function submit() {
    if (writing) return;
    writing = true;
    failure = null;
    try {
      await move(asset.id, at);
      onmoved(here);
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

<TfDialog why={alreadyHere ? "Open the place it should go." : null} title="Move {asset.name}" {onclose}>
  {#snippet body()}
    <div class="picker" bind:this={picker}>
      <PickerCrumb {crumb} onstand={(path) => stand(path)} />

      <div class="lvl">
        <!-- The list is the one `Tab` stop, and where the dialog arrives: a press there walks it, and none of its keys sends anything. -->
        <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
        <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
        <ul class="opts" aria-label="Where to move it" tabindex="0" onkeydown={(event) => levelKeys(event, { into: (id) => { const row = rows.find((candidate) => candidate.id === id); if (row) into(row); }, up })}>
          {#each rows as row (row.id)}
            <li>
              <!--
                Every row is a destination, **including a leaf**: making an
                asset that holds nothing hold something is the commonest move
                there is, and a picker that only opened branches could not do
                it. The chevron still says which rows have a level under them.
              -->
              {#if row.id === asset.id}
                <button class="opt" disabled title="{row.name} can't move inside itself">
                  <span class="mg">{kindTag(row.monogram)}</span>
                  <span class="nm">{row.name}</span>
                  <span class="faint">this asset</span>
                </button>
              {:else}
                <button class="opt" data-row={row.id} title="Open {row.name}" tabindex="-1" onclick={() => into(row)}>
                  <span class="mg">{kindTag(row.monogram)}</span>
                  <span class="nm">{row.name}</span>
                  {#if row.has_children}
                    <span class="chev" aria-hidden="true">›</span>
                  {/if}
                </button>
              {/if}
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
    <button class="tf-btn tf-ghost" onclick={onclose}>Cancel</button>
    <button class="tf-btn tf-primary" disabled={writing || alreadyHere} title={alreadyHere ? `${asset.name} is already here` : undefined} onclick={() => void submit()}>
      {writing ? "Moving…" : alreadyHere ? "Already here" : `Move into ${here}`}
    </button>
  {/snippet}
</TfDialog>

<style>
  /* `minmax(0, 1fr)`: an automatic track would take the nowrap crumb's width and push the list out of the dialog (F-543). */
  .picker {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: 10px;
  }

  /* Bounded: a level with fifty rows must not push the footer buttons out of
     reach, which is `LinkDialog`'s rule for its own result list. */
  .lvl {
    max-height: 240px;
    overflow: auto;
    border: 1px solid var(--hair);
    border-radius: 4px;
  }

  .opts {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  /* Inside the list, which scrolls: a ring outside it is cut to a line along the row's edge (F-537). */
  .opts:focus-visible,
  .opt:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: -2px;
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
    font-size: var(--t-sm);
  }

  .opt:hover:not(:disabled) {
    background: var(--raised);
  }

  .opt:disabled {
    color: var(--ink-2);
    cursor: not-allowed;
  }

  .opt .faint {
    flex: none;
    font-size: var(--t-xs);
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
    font-size: var(--t-2xs);
    font-weight: 500;
    color: var(--faint);
  }

  .opt .chev {
    flex: none;
    color: var(--faint);
  }

  .empty {
    padding: 8px 10px;
    font-size: var(--t-sm);
    color: var(--muted);
  }
</style>
