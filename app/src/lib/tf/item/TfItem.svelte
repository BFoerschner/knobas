<!--
  An item's page, `#/item/<entity id>` (#1201): the block view the item's own
  source answers (ADR-0020), for an item whose kind no surface draws. That is
  a custom role's item (ADR-0029): a custom role has no surface of its own, and
  its view opens when one of its items is opened.

  The page draws the vocabulary and never learns which adapter drew it: facts,
  text and entity lists, and a log as text. A block of another type belongs to
  a built-in role's page and is named, not drawn. A deferred section is read
  when the reader opens it (ADR-0020 rule 3). A source with no view for the
  kind draws the mirror's text with no line; a failed read draws it under a
  line naming the failure (rule 2). The payload is never read (ADR-0007).

  Its writes are source actions (#1194, #1407): every write a custom role has
  is one. The view's actions are the header's toolbar, an `actions` block is
  the same buttons where it stands, and each opens the shared action dialog,
  which queues the write.
-->
<script module lang="ts">
  import {
    entityView,
    getEntity,
    submitWrite,
    type EntityDetail,
    type SourceView,
    type ViewAction,
    type ViewBlock,
    type ViewSection,
  } from "../../ipc/entity";
  import type { QueuedWrite, WriteOpPayload } from "../../ipc/sources";
  import { DETAIL_REFRESH_BRIDGE, type DetailRefreshPorts } from "../refresh.svelte";

  /** The page's two reads, the write queue its actions queue through, and its *Refresh* (#2107). */
  export interface ItemPorts {
    getEntity: (entityId: string) => Promise<EntityDetail>;
    entityView: (entityId: string, section: string | null) => Promise<SourceView | null>;
    submitWrite: (payload: WriteOpPayload) => Promise<QueuedWrite>;
    refresh?: DetailRefreshPorts;
  }

  const BRIDGE: ItemPorts = {
    getEntity,
    entityView,
    submitWrite: (payload) => submitWrite(payload),
    refresh: DETAIL_REFRESH_BRIDGE,
  };
</script>

<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import { actionName } from "../../detail/view/actions";
  import TfCrumbs from "../TfCrumbs.svelte";
  import TfEntityAction from "../TfEntityAction.svelte";
  import TfDetailBanners from "../TfDetailBanners.svelte";
  import TfDetailRefresh from "../TfDetailRefresh.svelte";
  import { createDetailRefresh } from "../refresh.svelte";
  import { untrack } from "svelte";

  let {
    id,
    ready,
    job,
    jobAt = null,
    ports = BRIDGE,
  }: {
    id: string;
    ready: boolean;
    /** The job the reader was last in, which the breadcrumb starts at (R-1.6). */
    job: string | null;
    /** The address the job was opened at (#1525), which its chip links; its key when omitted. */
    jobAt?: string | null;
    ports?: ItemPorts;
  } = $props();

  let detail = $state<EntityDetail | null>(null);
  let failed = $state<string | null>(null);
  /** `undefined` until the view answers; `null` when the source draws nothing for the kind. */
  let view = $state<SourceView | null | undefined>(undefined);
  let viewFailed = $state<string | null>(null);
  /** A deferred section's read, once the reader opened it. */
  type Opened = { section: ViewSection } | { failed: string };
  /** Deferred sections read so far, by id. */
  let opened = $state<Record<string, Opened>>({});
  /** The header's *Refresh* or *Sync now* (#2107). */
  const refresh = createDetailRefresh(untrack(() => ports.refresh));

  $effect(() => {
    if (!ready) return;
    const entity = id;
    let live = true;
    detail = null;
    failed = null;
    view = undefined;
    viewFailed = null;
    opened = {};
    shown = null;
    untrack(() => refresh.reset());
    ports.getEntity(entity).then(
      (found) => live && (detail = found),
      (error: unknown) => live && (failed = ipcErrorMessage(error)),
    );
    ports.entityView(entity, null).then(
      (answer) => live && (view = answer),
      (error: unknown) => {
        if (!live) return;
        view = null;
        viewFailed = ipcErrorMessage(error);
      },
    );
    return () => {
      live = false;
    };
  });

  /**
   * The detail and its view read again after a refresh or its source's run
   * (#2107), over what the page drew: a failed read keeps it.
   */
  async function reread() {
    const entity = id;
    const [found, answer] = await Promise.allSettled([ports.getEntity(entity), ports.entityView(entity, null)]);
    if (entity !== id) return;
    if (found.status === "fulfilled") detail = found.value;
    if (answer.status === "fulfilled") {
      view = answer.value;
      viewFailed = null;
    }
  }

  function readDeferred(section: ViewSection) {
    const entity = id;
    ports.entityView(entity, section.id).then(
      (answer) => {
        if (entity !== id) return;
        const found = answer?.sections.find((one) => one.id === section.id);
        opened[section.id] = found ? { section: found } : { failed: "the source drew nothing for it" };
      },
      (error: unknown) => {
        if (entity === id) opened[section.id] = { failed: ipcErrorMessage(error) };
      },
    );
  }

  const kindLabel = $derived(detail === null ? "" : (detail.kind_info?.label ?? detail.row.kind));
  const actions = $derived(view?.actions ?? []);

  /** The action whose dialog is open. */
  let shown = $state<ViewAction | null>(null);

  function press(action: ViewAction) {
    if (action.disabled === null) shown = action;
  }
</script>

{#snippet buttonsOf(list: ViewAction[])}
  {#each list as action, n (n)}
    <button
      type="button"
      class="tf-btn"
      class:tf-primary={action.primary}
      aria-disabled={action.disabled !== null ? "true" : undefined}
      title={action.disabled ?? undefined}
      onclick={() => press(action)}>{actionName(action.label)}</button
    >
  {/each}
{/snippet}

{#snippet textOf(text: string)}
  <div class="tf-panel tf-item-textp"><p class="tf-item-text">{text}</p></div>
{/snippet}

{#snippet blocksOf(blocks: ViewBlock[])}
  {#each blocks as block, n (n)}
    {#if block.type === "facts"}
      <dl class="tf-item-facts">
        {#each block.facts as fact, f (f)}
          <dt>{fact.label}</dt>
          <dd class:tf-err={fact.tone === "err"} class:tf-warn={fact.tone === "warn"} class:tf-ok={fact.tone === "ok"}>{fact.value}</dd>
        {/each}
      </dl>
    {:else if block.type === "text"}
      {@render textOf(block.text)}
    {:else if block.type === "entities"}
      {#if block.title !== null}<h3 class="tf-cgroup-h">{block.title}</h3>{/if}
      <ul class="tf-item-rows">
        {#each block.rows as row, r (r)}
          <li>
            {#if row.entity_id !== null}
              <a href="#/entity/{encodeURIComponent(row.entity_id)}">{row.title}</a>
            {:else}
              {row.title}
            {/if}
            {#if row.chip !== null}<span class="tf-chip tf-{row.tone ?? 'muted'}">{row.chip}</span>{/if}
            {#if row.meta !== null}<span class="tf-item-meta">{row.meta}</span>{/if}
          </li>
        {/each}
      </ul>
    {:else if block.type === "log"}
      <pre class="tf-item-log">{block.text}</pre>
    {:else if block.type === "actions"}
      <div class="tf-item-acts">{@render buttonsOf(block.actions)}</div>
    {:else}
      <p class="tf-item-meta">A {block.type} block is drawn on its role's page, not here.</p>
    {/if}
  {/each}
{/snippet}

<div class="tf-full">
  <!-- The breadcrumb names the kind (#1408): the title is the header's. -->
  {#if failed !== null}
    <TfCrumbs {job} {jobAt} name="Item" />
    <div class="tf-empty"><p class="tf-cfail">Could not read {id}: {failed}</p></div>
  {:else if detail !== null}
    <TfCrumbs {job} {jobAt} name={kindLabel} />
    <header class="tf-fhead">
      <div>
        <h1>{detail.row.title}</h1>
        <div class="tf-sub">{kindLabel} · {detail.source.display_name}</div>
      </div>
      {#if detail.web_url !== null || actions.length > 0 || refresh.offer(detail) !== null}
        <div class="tf-acts">
          {@render buttonsOf(actions)}
          <TfDetailRefresh {detail} {refresh} {reread} />
          {#if detail.web_url !== null}
            <a class="tf-btn" href={detail.web_url} target="_blank" rel="noreferrer">Open in {detail.source.display_name}</a>
          {/if}
        </div>
      {/if}
    </header>
    <TfDetailBanners {detail} failed={refresh.failed} />
    <div class="tf-item-body">
      {#if view === undefined}
        <!-- The view has not answered yet. -->
      {:else if view === null}
        {#if viewFailed !== null}<p class="tf-cfail">Showing the mirror: {viewFailed}</p>{/if}
        {#if detail.body_text !== ""}
          {@render textOf(detail.body_text)}
        {:else}
          <div class="tf-empty">The mirror holds no text for this {kindLabel.toLowerCase()}.</div>
        {/if}
      {:else}
        {#each view.sections as section (section.id)}
          <section aria-label={section.title}>
            <!-- One section is the item itself: its title is the page's. -->
            {#if view.sections.length > 1}<h2 class="tf-cgroup-h">{section.title}</h2>{/if}
            {#if !section.deferred}
              {@render blocksOf(section.blocks)}
            {:else if opened[section.id] !== undefined}
              {@const read = opened[section.id]!}
              {#if "section" in read}
                {@render blocksOf(read.section.blocks)}
              {:else}
                <p class="tf-cfail">Could not read {section.title}: {read.failed}</p>
              {/if}
            {:else}
              <button type="button" class="tf-btn" onclick={() => readDeferred(section)}
                >Show {section.title}{section.count !== null ? ` (${section.count})` : ""}</button
              >
            {/if}
          </section>
        {/each}
      {/if}
    </div>
  {/if}
</div>

{#if shown !== null && detail !== null}
  {@const action = shown}
  {@const name = actionName(action.label)}
  <TfEntityAction
    {action}
    entityId={id}
    title="{name} on {detail.row.title}"
    what="{name.toLowerCase()} on {detail.row.title}"
    source={detail.source}
    submitWrite={ports.submitWrite}
    onclose={() => (shown = null)}
  />
{/if}
