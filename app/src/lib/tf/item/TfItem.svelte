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

  Its writes (a source action, #1204) are not drawn here yet.
-->
<script module lang="ts">
  import { entityView, getEntity, type EntityDetail, type SourceView, type ViewBlock, type ViewSection } from "../../ipc/entity";

  /** The page's two reads. */
  export interface ItemPorts {
    getEntity: (entityId: string) => Promise<EntityDetail>;
    entityView: (entityId: string, section: string | null) => Promise<SourceView | null>;
  }

  const BRIDGE: ItemPorts = { getEntity, entityView };
</script>

<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";

  let { id, ready, ports = BRIDGE }: { id: string; ready: boolean; ports?: ItemPorts } = $props();

  let detail = $state<EntityDetail | null>(null);
  let failed = $state<string | null>(null);
  /** `undefined` until the view answers; `null` when the source draws nothing for the kind. */
  let view = $state<SourceView | null | undefined>(undefined);
  let viewFailed = $state<string | null>(null);
  /** Deferred sections read so far, by id; a string is the read's failure. */
  let opened = $state<Record<string, ViewSection | string>>({});

  $effect(() => {
    if (!ready) return;
    const entity = id;
    let live = true;
    detail = null;
    failed = null;
    view = undefined;
    viewFailed = null;
    opened = {};
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

  function open(section: ViewSection) {
    const entity = id;
    ports.entityView(entity, section.id).then(
      (answer) => {
        if (entity !== id) return;
        opened[section.id] = answer?.sections.find((one) => one.id === section.id) ?? "the source drew nothing for it";
      },
      (error: unknown) => {
        if (entity === id) opened[section.id] = ipcErrorMessage(error);
      },
    );
  }

  const kindLabel = $derived(detail === null ? "" : (detail.kind_info?.label ?? detail.row.kind));
</script>

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
      <div class="tf-panel tf-prdesc"><p class="tf-item-text">{block.text}</p></div>
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
    {:else}
      <p class="tf-item-meta">A {block.type} block is drawn on its role's page, not here.</p>
    {/if}
  {/each}
{/snippet}

<div class="tf-full">
  {#if failed !== null}
    <div class="tf-empty"><p class="tf-cfail">Could not read {id}: {failed}</p></div>
  {:else if detail !== null}
    <header class="tf-fhead">
      <div>
        <h1>{detail.row.title}</h1>
        <div class="tf-sub">{kindLabel} · {detail.source.display_name}</div>
      </div>
      {#if detail.web_url !== null}
        <div class="tf-acts"><a class="tf-btn" href={detail.web_url} target="_blank" rel="noreferrer">Open in {detail.source.display_name}</a></div>
      {/if}
    </header>
    <div class="tf-item-body">
      {#if view === undefined}
        <!-- The view has not answered yet. -->
      {:else if view === null}
        {#if viewFailed !== null}<p class="tf-cfail">Showing the mirror: {viewFailed}</p>{/if}
        {#if detail.body_text !== ""}
          <div class="tf-panel tf-prdesc"><p class="tf-item-text">{detail.body_text}</p></div>
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
            {:else if typeof opened[section.id] === "object"}
              {@render blocksOf((opened[section.id] as ViewSection).blocks)}
            {:else if typeof opened[section.id] === "string"}
              <p class="tf-cfail">Could not read {section.title}: {opened[section.id]}</p>
            {:else}
              <button type="button" class="tf-btn" onclick={() => open(section)}
                >Show {section.title}{section.count !== null ? ` (${section.count})` : ""}</button
              >
            {/if}
          </section>
        {/each}
      {/if}
    </div>
  {/if}
</div>
