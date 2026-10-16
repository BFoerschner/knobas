<!--
  The job's Code section (#816, R-4.6 and R-4.7), over the job-reach read
  (#817): `list_entities` scoped by what the ticket reaches.

  - The **local checkout line** first: path, branch, changed count and
    *Open in…* (BUILT `entity_checkout`, `open_checkout`). Asked of the job's
    branches first, then its repositories, until one has a clone on this disk.
  - A one-line **rollup**: the repositories and the open pull requests.
  - The rest is every section's (`TfSection.svelte`).
-->
<script lang="ts">
  import { untrack } from "svelte";

  import type { CheckoutCommand, CheckoutView, EntityRow, Reached } from "../../ipc/entity";
  import { kindSingular } from "../../shell/kinds";
  import { ago } from "../../shell/time";
  import { push } from "../../shell/toasts.svelte";
  import { ipcErrorMessage } from "../../ipc";
  import type { Layers } from "../layers.svelte";
  import { byAttention, itemHash, openPrs, originLabel, reposOf, type SectionRow } from "./job";
  import TfSection from "./TfSection.svelte";

  let {
    rows,
    reached = new Map(),
    checkout,
    checkoutOf,
    commands,
    onopen,
    layers,
  }: {
    /** The section's rows, or `null` while the read is out. */
    rows: EntityRow[] | null;
    /** The job-reach read, by entity id, for the rows' labels. */
    reached?: Map<string, Reached>;
    /** The first clone found, or `null` (none found, or still looking). */
    checkout: CheckoutView | null;
    /** The entity the checkout was read for, which *Open in…* names. */
    checkoutOf: string | null;
    commands: CheckoutCommand[];
    onopen: (entityId: string, action: string) => Promise<void>;
    layers: Layers;
  } = $props();

  let menuOpen = $state(false);

  const sorted = $derived(byAttention(rows ?? []));
  const repos = $derived(reposOf(sorted));
  const open = $derived(openPrs(sorted));
  const apps = $derived(commands.filter((command) => command.template !== null));
  const tree = $derived(checkout?.tree?.status === "read" ? checkout.tree : null);
  const changed = $derived(tree ? tree.changed + tree.untracked : 0);
  const lines = $derived<SectionRow[] | null>(
    rows === null
      ? null
      : sorted.map((row) => ({
          id: row.entity_id,
          href: itemHash(row),
          title: row.title,
          mono: row.kind === "branch",
          repo: row.path && row.kind !== "repo" ? row.path : null,
          sub: sub(row),
          tone: row.state?.tone ?? null,
          chip: row.state,
          from: originLabel(reached.get(row.entity_id)),
        })),
  );

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

<TfSection id="code" title="Code" openLabel="Open Code view" openHref="#/code" footHref="#/code" rows={lines}>
  {#snippet roll()}
    {#if repos.length > 0}
      <span>{repos.length} {repos.length > 1 ? "repos" : "repo"}: {#each repos as repo, index (repo)}{index > 0 ? ", " : ""}<span class="tf-mono">{repo}</span>{/each}</span>
      {#if open > 0}<span class="tf-signal"><span class="tf-dot tf-info"></span>{open} open {open > 1 ? "PRs" : "PR"}</span>{/if}
    {:else}
      <span>{sorted.length} connected</span>
    {/if}
  {/snippet}
  {#snippet before()}
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
  {/snippet}
</TfSection>
