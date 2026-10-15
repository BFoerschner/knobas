<!--
  The job's Code section (#816, R-4.6 and R-4.7), over today's membership
  read: `list_entities` scoped by the job's context (ADR-0008). The job-reach
  read replaces it in #817 without changing what is drawn.

  - The **local checkout line** first: path, branch, changed count and
    *Open in…* (BUILT `entity_checkout`, `open_checkout`). Asked of the job's
    branches first, then its repositories, until one has a clone on this disk.
  - A one-line **rollup**: the repositories and the open pull requests.
  - Rows sorted by what asks for you, then newest first, **at most four**;
    past four, a footer with an in-section filter, *Show all N* and *All in
    Code view*.
  - **Every row opens the item's full view** (R-4.10), never a panel.
-->
<script lang="ts">
  import { untrack } from "svelte";

  import type { CheckoutCommand, CheckoutView, EntityRow } from "../../ipc/entity";
  import { kindSingular } from "../../shell/kinds";
  import { ago } from "../../shell/time";
  import { push } from "../../shell/toasts.svelte";
  import { ipcErrorMessage } from "../../ipc";
  import type { Layers } from "../layers.svelte";
  import { byAttention, itemHash, matches, openPrs, reposOf } from "./job";

  let {
    rows,
    checkout,
    checkoutOf,
    commands,
    onopen,
    layers,
  }: {
    /** The section's rows, or `null` while the read is out. */
    rows: EntityRow[] | null;
    /** The first clone found, or `null` (none found, or still looking). */
    checkout: CheckoutView | null;
    /** The entity the checkout was read for, which *Open in…* names. */
    checkoutOf: string | null;
    commands: CheckoutCommand[];
    onopen: (entityId: string, action: string) => Promise<void>;
    layers: Layers;
  } = $props();

  const CAP = 4;
  let filter = $state("");
  let expanded = $state(false);
  let menuOpen = $state(false);

  const sorted = $derived(byAttention(rows ?? []));
  const matching = $derived(sorted.filter((row) => matches(row, filter)));
  const shown = $derived(expanded || filter.trim() !== "" ? matching : matching.slice(0, CAP));
  const repos = $derived(reposOf(sorted));
  const open = $derived(openPrs(sorted));
  const apps = $derived(commands.filter((command) => command.template !== null));
  const tree = $derived(checkout?.tree?.status === "read" ? checkout.tree : null);
  const changed = $derived(tree ? tree.changed + tree.untracked : 0);

  $effect(() => {
    if (!menuOpen) return;
    // Untracked: registering reads the stack it writes.
    return untrack(() => layers.add({ kind: "dialog", close: () => (menuOpen = false) }));
  });

  async function run(action: string, label: string) {
    menuOpen = false;
    if (checkoutOf === null) return;
    try {
      await onopen(checkoutOf, action);
    } catch (cause) {
      push({ text: `Could not ${label.charAt(0).toLowerCase()}${label.slice(1)}: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }

  function sub(row: EntityRow): string {
    const when = row.updated_at ? `, ${ago(row.updated_at)}` : "";
    return `${kindSingular(row.kind).toLowerCase()}${when}`;
  }
</script>

<article class="tf-panel" aria-labelledby="tf-sec-code">
  <header>
    <h3 id="tf-sec-code">Code</h3>
    <a class="tf-open" href="#/code">Open Code view <svg viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true"><path d="M7 1.5h3.5V5M10.5 1.5 6 6M5 2H1.5v8.5H10V7" /></svg></a>
  </header>
  <div class="tf-roll">
    {#if rows === null}
      <span>…</span>
    {:else if repos.length > 0}
      <span>{repos.length} {repos.length > 1 ? "repos" : "repo"}: {#each repos as repo, index (repo)}{index > 0 ? ", " : ""}<span class="tf-mono">{repo}</span>{/each}</span>
      {#if open > 0}<span class="tf-signal"><span class="tf-dot tf-info"></span>{open} open PR</span>{/if}
    {:else if sorted.length === 0}
      <span>Nothing connected.</span>
    {/if}
  </div>
  {#if checkout?.path}
    <div class="tf-checkout">
      <span><span class="tf-dim">Local</span> <span class="tf-mono">{checkout.path}</span>{#if tree?.branch}{" on "}<span class="tf-mono">{tree.branch}</span>{/if}</span>
      {#if changed > 0}<span class="tf-chip tf-warn">{changed} changed</span>{/if}
      {#if apps.length > 0}
        <span class="tf-acts tf-openin">
          <button class="tf-btn tf-sm" aria-haspopup="menu" aria-expanded={menuOpen} onclick={() => (menuOpen = !menuOpen)}>Open in… <span class="tf-dim" aria-hidden="true">▾</span></button>
          {#if menuOpen}
            <div class="tf-menu" role="menu">
              {#each apps as app (app.action)}
                <button role="menuitem" onclick={() => run(app.action, app.label)}>{app.label}</button>
              {/each}
            </div>
          {/if}
        </span>
      {/if}
    </div>
  {/if}
  <div class="tf-rows">
    {#each shown as row (row.entity_id)}
      <a class="tf-r" href={itemHash(row)}>
        <span class="tf-dot {row.state ? `tf-${row.state.tone}` : ''}"></span>
        <span class="tf-t">
          {#if row.kind === "branch"}<span class="tf-mono">{row.title}</span>{:else}{row.title}{/if}
          <span class="tf-rsub">{#if row.path && row.kind !== "repo"}<span class="tf-repo">{row.path}</span>{" "}{/if}{sub(row)}</span>
        </span>
        {#if row.state}<span class="tf-rend"><span class="tf-chip tf-{row.state.tone}">{row.state.text}</span></span>{/if}
      </a>
    {:else}
      {#if rows !== null && sorted.length === 0}
        <div class="tf-r tf-dim tf-small">Nothing connected yet. Use the connections drawer to link something.</div>
      {:else if rows !== null}
        <div class="tf-r tf-dim">Nothing matches.</div>
      {/if}
    {/each}
  </div>
  {#if sorted.length > CAP}
    <div class="tf-secfoot">
      <label class="tf-minif">
        <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" aria-hidden="true"><circle cx="7" cy="7" r="4.5" /><path d="m10.5 10.5 3 3" /></svg>
        <input data-filter bind:value={filter} placeholder="Filter {sorted.length} code" aria-label="Filter Code" />
      </label>
      {#if filter.trim() !== ""}
        <span class="tf-dim tf-small">{matching.length} match</span>
      {:else}
        <button class="tf-linkish" onclick={() => (expanded = !expanded)}>{expanded ? "Show fewer" : `Show all ${sorted.length}`}</button>
      {/if}
      <a class="tf-open tf-push" href="#/code">All in Code view</a>
    </div>
  {/if}
</article>
