<!--
  *Link an asset…* — an open monitor's way to say what it watches, on Monitors (2026-11-19, when Assets got its nav entry
  back: the panel said "No asset. Link one from its asset's page." and offered nothing).

  The picker is *Move to…*'s (`MoveDialog.svelte`): it walks the estate one level at a time, `assetTree(parentId)`, as
  the Tree does, because the estate is a tree of unknown depth and a flat list of every asset is what Miller columns exist
  to avoid. **Where you have walked to is what you link**, so a leaf is linked by opening it, and the top of the estate,
  which is no asset, links nothing.

  What it writes is the one `monitored-by` link (`create_link`, from the asset to the monitor), the link an import or
  *Create monitor for this asset* draws through a name, so the roster, the inbox's alert rule and *Assets nothing watches*
  read it as they read those. An asset the monitor already watches is marked, and linking it again is not offered.

  An estate with nothing in it, or nothing fitting, is why *New asset…* is in the footer: the asset is made at the top of
  the estate, named after the monitor, and linked (`oncreate`, the page's).
-->
<script lang="ts">
  import { tick } from "svelte";

  import { levelKeys } from "./level-keys";
  import PickerCrumb from "./PickerCrumb.svelte";
  import { kindTag } from "./tree";
  import { ipcErrorMessage } from "../ipc";
  import type { assetTree, AssetRow } from "../ipc/assets";
  import { latestRead } from "../shell/latest-read";
  import TfDialog from "../tf/settings/TfDialog.svelte";

  let {
    monitor,
    watched,
    tree,
    link,
    onclose,
    onlinked,
    oncreate,
  }: {
    /** The monitor being linked, by the name the dialog says. */
    monitor: { name: string };
    /** The ids of the assets it already watches. */
    watched: ReadonlySet<string>;
    tree: typeof assetTree;
    /** Draw the link from this asset to the monitor. */
    link: (assetId: string) => Promise<unknown>;
    onclose: () => void;
    /** The link was written, to the asset named. */
    onlinked: (asset: AssetRow) => void;
    /** *New asset…*: this dialog closes and the page opens the create dialog. */
    oncreate: () => void;
  } = $props();

  let crumb = $state<AssetRow[]>([]);
  let at = $state<string | null>(null);
  let rows = $state<AssetRow[]>([]);
  let failure = $state<string | null>(null);
  let writing = $state(false);
  /** Nothing has come back yet — told apart from a level holding nothing. */
  let loaded = $state(false);
  let picker = $state<HTMLElement | null>(null);
  let walked = false;
  let cameFrom: string | null = null;

  const columnRead = latestRead<AssetRow[]>();

  /** The asset the picker stands on: what *Link* links. */
  const here = $derived(crumb.at(-1) ?? null);
  const already = $derived(here !== null && watched.has(here.id));

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

  function land(row: string | null) {
    const options = [...(picker?.querySelectorAll<HTMLElement>(".opt:not(:disabled)") ?? [])];
    const spot = options.find((option) => option.dataset.row === row) ?? options[0] ?? picker?.querySelector<HTMLElement>(".opts");
    spot?.focus();
  }

  function into(row: AssetRow) {
    stand([...crumb, row]);
  }

  function up() {
    if (crumb.length > 0) stand(crumb.slice(0, -1), crumb.at(-1)?.id ?? null);
  }

  function stand(path: AssetRow[], from: string | null = null) {
    walked = true;
    cameFrom = from;
    crumb = path;
    at = path.at(-1)?.id ?? null;
    failure = null;
  }

  async function submit() {
    const target = here;
    if (writing || target === null || already) return;
    writing = true;
    failure = null;
    try {
      await link(target.id);
      onlinked(target);
    } catch (rejection) {
      failure = ipcErrorMessage(rejection);
    } finally {
      writing = false;
    }
  }

  const primary = $derived(writing ? "Linking…" : here === null ? "Link" : already ? "Already linked" : `Link to ${here.name}`);
</script>

<TfDialog why={here === null ? "Open the asset it watches." : already ? `${monitor.name} already watches ${here.name}.` : null} title="Link {monitor.name} to an asset" subtitle="What this monitor watches" {onclose}>
  {#snippet body()}
    <div class="picker" bind:this={picker}>
      <PickerCrumb {crumb} onstand={(path) => stand(path)} />

      <div class="lvl">
        <!-- The list is the one `Tab` stop, as *Move to…*'s: a press there walks it, and none of its keys sends anything. -->
        <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
        <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
        <ul class="opts" aria-label="The asset it watches" tabindex="0" onkeydown={(event) => levelKeys(event, { into: (id) => { const row = rows.find((candidate) => candidate.id === id); if (row) into(row); }, up })}>
          {#each rows as row (row.id)}
            <li>
              <button class="opt" data-row={row.id} title="Open {row.name}" tabindex="-1" onclick={() => into(row)}>
                <span class="mg">{kindTag(row.monogram)}</span>
                <span class="nm">{row.name}</span>
                {#if watched.has(row.id)}<span class="faint">watched by it</span>{/if}
                {#if row.has_children}
                  <span class="chev" aria-hidden="true">›</span>
                {/if}
              </button>
            </li>
          {/each}
        </ul>
        {#if loaded && rows.length === 0}
          <p class="empty">{here === null ? "No assets yet. Make the one it watches with New asset." : `Nothing inside ${here.name}.`}</p>
        {/if}
      </div>

      {#if failure}
        <p class="fail" role="alert">{failure}</p>
      {/if}
    </div>
  {/snippet}

  {#snippet footer()}
    <button class="tf-btn tf-ghost tf-first" onclick={oncreate}>New asset…</button>
    <button class="tf-btn tf-ghost" onclick={onclose}>Cancel</button>
    <button class="tf-btn tf-primary" disabled={writing || here === null || already} onclick={() => void submit()}>{primary}</button>
  {/snippet}
</TfDialog>

<style>
  /* *Move to…*'s picker (`MoveDialog.svelte`), the same rows and room. */
  .picker {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: 10px;
  }

  .lvl {
    max-height: 240px;
    overflow: auto;
    background: var(--under);
    border: 1px solid var(--hair);
    border-radius: 4px;
  }

  .opts {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .opts:focus-visible,
  .opt:focus-visible {
    outline: var(--focus-w) solid var(--focus-c, var(--focus));
    outline-offset: var(--focus-in);
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
    font-size: calc(var(--t-sm) * var(--fs));
  }

  .opt:hover {
    background: var(--raised);
  }

  .opt .faint {
    flex: none;
    font-size: calc(var(--t-xs) * var(--fs));
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
    font-size: calc(var(--t-2xs) * var(--fs));
    font-weight: 500;
    color: var(--faint);
  }

  .opt .chev {
    flex: none;
    color: var(--faint);
  }

  .empty {
    padding: 8px 10px;
    font-size: calc(var(--t-sm) * var(--fs));
    color: var(--muted);
  }

  .fail {
    color: var(--fail);
    font-size: calc(var(--t-xs) * var(--fs));
  }
</style>
