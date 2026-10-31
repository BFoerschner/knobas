<!--
  The job's Code section (#816, R-4.6 and R-4.7), over the job-reach read
  (#817): `list_entities` scoped by what the ticket reaches.

  - The **local checkout line** first: path, branch, changed count and
    *Open in…* (`tf/openin/TfOpenIn.svelte`, #844). Asked of the job's
    branches first, then its repositories, until one has a clone on this disk;
    with none, the first repository's line offers *Clone* (#846, R-8.25).
  - A one-line **rollup**: the repositories and the open pull requests.
  - The rest is every section's (`TfSection.svelte`).
-->
<script lang="ts">
  import type { CheckoutView, EntityRow, Reached } from "../../ipc/entity";
  import { kindSingular } from "../../shell/kinds";
  import { keyOf } from "../../time/format";
  import { ago } from "../../shell/time";
  import type { Layers } from "../layers.svelte";
  import type { Apps } from "../openin/apps.svelte";
  import { sessionFilter } from "../filter/filter-state.svelte";
  import TfOpenIn from "../openin/TfOpenIn.svelte";
  import { byAttention, itemHash, openPrs, originLabel, reposOf, type SectionRow } from "./job";
  import TfSection from "./TfSection.svelte";

  let {
    rows,
    reached = new Map(),
    checkout,
    checkoutOf,
    layers,
    apps,
  }: {
    /** The section's rows, or `null` while the read is out. */
    rows: EntityRow[] | null;
    /** The job-reach read, by entity id, for the rows' labels. */
    reached?: Map<string, Reached>;
    /** The first clone found, else the first repository's no-clone view, or `null` (still looking). */
    checkout: CheckoutView | null;
    /** The entity the checkout was read for, which *Open in…* names. */
    checkoutOf: string | null;
    layers?: Layers;
    apps?: Apps | undefined;
  } = $props();

  const sorted = $derived(byAttention(rows ?? []));
  const repos = $derived(reposOf(sorted));
  const open = $derived(openPrs(sorted));
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

  /** The repository the line is about, for *Clone*'s folder. */
  const repoTitle = $derived(
    sorted.find((row) => row.entity_id === checkout?.repo_entity_id)?.title ?? (checkout?.repo_entity_id ? keyOf(checkout.repo_entity_id) : ""),
  );

  function sub(row: EntityRow): string {
    const when = row.updated_at ? `, ${ago(row.updated_at)}` : "";
    return `${kindSingular(row.kind).toLowerCase()}${when}`;
  }
</script>

<TfSection id="code" title="Code" openLabel="Open Code view" openHref="#/code" footHref="#/code" onopen={() => (sessionFilter("code-repos").scope = "job")} rows={lines}>
  {#snippet roll()}
    {#if repos.length > 0}
      <span>{repos.length} {repos.length > 1 ? "repos" : "repo"}: {#each repos as repo, index (repo)}{index > 0 ? ", " : ""}<span class="tf-mono">{repo}</span>{/each}</span>
      {#if open > 0}<span class="tf-signal"><span class="tf-dot tf-info"></span>{open} open {open > 1 ? "PRs" : "PR"}</span>{/if}
    {:else}
      <span>{sorted.length} connected</span>
    {/if}
  {/snippet}
  {#snippet before()}
    {#if checkout?.path && checkoutOf !== null}
      <div class="tf-checkout">
        <span><span class="tf-dim">Local</span> <span class="tf-mono">{checkout.path}</span>{#if tree?.branch}{" on "}<span class="tf-mono">{tree.branch}</span>{/if}</span>
        {#if changed > 0}<span class="tf-chip tf-warn">{changed} changed</span>{/if}
        <span class="tf-trail"><TfOpenIn entityId={checkoutOf} title={repoTitle} {checkout} {layers} {apps} /></span>
      </div>
    {:else if checkout !== null && checkoutOf !== null}
      <div class="tf-checkout">
        <span><span class="tf-mono">{repoTitle}</span> <span class="tf-dim">is not on this machine yet.</span></span>
        <span class="tf-trail"><TfOpenIn entityId={checkoutOf} title={repoTitle} {checkout} {layers} {apps} /></span>
      </div>
    {/if}
  {/snippet}
</TfSection>
