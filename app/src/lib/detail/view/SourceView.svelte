<!--
  A source view (ADR-0020; `CONTEXT.md` **Source view**): what an adapter drew
  for one of its kinds, as tabs over the detail.

  The shell draws the vocabulary and never learns which adapter drew it -- so
  nothing here names a source, a kind or a field. An actions block draws only
  the actions whose op the source declares (#667). One component per block, in
  this folder; a block this file does not know is not drawn (the vocabulary is
  frozen, and a new block arrives with its component in the same change).

  **A deferred tab is read once, on its first opening** (rule 3): its title and
  count come with the view, its blocks with `entityView(entity, section)`. The
  answer is kept for as long as the detail is open; a failed read says why and
  offers to try again rather than leaving an empty tab.
-->
<script lang="ts">
  import { untrack } from "svelte";
  import {
    entityView,
    type ViewAction,
    type ViewBlock,
    type SourceView,
    type ViewSection,
  } from "../../ipc/entity";
  import { ipcErrorMessage } from "../../ipc";
  import ActionsBlock from "./ActionsBlock.svelte";
  import { offeredActions } from "./actions";
  import DiffBlock from "./DiffBlock.svelte";
  import EntitiesBlock from "./EntitiesBlock.svelte";
  import FactsBlock from "./FactsBlock.svelte";
  import LogBlock from "./LogBlock.svelte";
  import SettingsBlock from "./SettingsBlock.svelte";
  import TextBlock from "./TextBlock.svelte";

  let {
    entityId,
    view,
    webUrl,
    sourceName,
    writeOps,
    onnavigate,
    onopenurl,
    onaction,
  }: {
    entityId: string;
    view: SourceView;
    /** The entity's page at its source, for a block that was cut. */
    webUrl: string | null;
    sourceName: string;
    /**
     * The write ops the entity's source declares. An action is drawn only
     * where its op is among them (#667): `submit_write` refuses any other.
     */
    writeOps: string[];
    onnavigate: (hash: string) => void;
    onopenurl: (url: string) => void;
    /** Queue an action's write; answers whether it was queued. */
    onaction: (action: ViewAction, option: string | null) => Promise<boolean>;
  } = $props();

  /** The open tab, by section id. The first section until the reader picks. */
  let picked = $state<string | null>(null);
  const current = $derived(
    view.sections.find((section) => section.id === picked) ?? view.sections[0] ?? null,
  );

  /** A deferred section's own read, by id: in flight, failed, or answered. */
  type Read = { state: "reading" } | { state: "failed"; message: string } | { state: "read"; blocks: ViewBlock[] };
  let reads = $state<Record<string, Read>>({});

  /**
   * A view read again (after a write, #667) drops the deferred tabs it read
   * before: their blocks were of the view that was. `seenView` is a plain
   * `let`, so writing it does not re-run this; the first run only records it.
   */
  let seenView: SourceView | null = null;
  $effect(() => {
    const now = view;
    const before = seenView;
    seenView = now;
    if (before === null || before === now) return;
    untrack(() => (reads = {}));
  });

  /** Unique per instance, so tab and panel ids cannot collide. */
  const uid = Math.random().toString(36).slice(2, 9);

  function read(section: ViewSection) {
    const entity = entityId;
    reads[section.id] = { state: "reading" };
    entityView(entity, section.id)
      .then((answer) => {
        if (entity !== entityId) return;
        const blocks = answer?.sections.find((s) => s.id === section.id)?.blocks;
        reads[section.id] =
          blocks === undefined
            ? { state: "failed", message: "the source answered without this section" }
            : { state: "read", blocks };
      })
      .catch((rejection: unknown) => {
        if (entity !== entityId) return;
        reads[section.id] = { state: "failed", message: ipcErrorMessage(rejection) };
      });
  }

  function open(section: ViewSection) {
    picked = section.id;
  }

  // The shown tab's deferred read, on its first showing -- a click, a key,
  // or the view opening on a first section that is itself deferred.
  $effect(() => {
    const section = current;
    if (section?.deferred && reads[section.id] === undefined) untrack(() => read(section));
  });

  let tabs = $state<(HTMLButtonElement | null)[]>([]);

  /** Left and right move between tabs, Home and End to the ends (WAI-ARIA tabs). */
  function key(event: KeyboardEvent, index: number) {
    const last = view.sections.length - 1;
    const to =
      event.key === "ArrowRight"
        ? index === last ? 0 : index + 1
        : event.key === "ArrowLeft"
          ? index === 0 ? last : index - 1
          : event.key === "Home"
            ? 0
            : event.key === "End"
              ? last
              : null;
    if (to === null) return;
    event.preventDefault();
    const section = view.sections[to];
    if (!section) return;
    open(section);
    tabs[to]?.focus();
  }

  const shown = $derived.by((): Read | null => {
    if (!current) return null;
    const read: Read = current.deferred
      ? (reads[current.id] ?? { state: "reading" })
      : { state: "read", blocks: current.blocks };
    if (read.state !== "read") return read;
    return { state: "read", blocks: offered(read.blocks) };
  });

  /**
   * The blocks as drawn: an actions block keeps only the actions the source
   * declares, and goes when none is left -- an action is absent, not
   * disabled, where it cannot be taken.
   */
  function offered(blocks: ViewBlock[]): ViewBlock[] {
    return blocks.flatMap((block): ViewBlock[] => {
      if (block.type !== "actions") return [block];
      const actions = offeredActions(block.actions, writeOps);
      return actions.length === 0 ? [] : [{ type: "actions", actions }];
    });
  }
</script>

{#if current}
  <div class="sv">
    <div class="sv-tabs" role="tablist" aria-label="Views">
      {#each view.sections as section, index (section.id)}
        <button
          bind:this={tabs[index]}
          class="sv-tab"
          class:on={section.id === current.id}
          role="tab"
          id="sv-tab-{uid}-{index}"
          aria-selected={section.id === current.id}
          aria-controls="sv-panel-{uid}"
          tabindex={section.id === current.id ? 0 : -1}
          onclick={() => open(section)}
          onkeydown={(event) => key(event, index)}
        >
          {section.title}
          {#if section.count !== null}
            <span class="sv-count">{section.count}</span>
          {/if}
        </button>
      {/each}
    </div>

    <div
      class="sv-panel"
      role="tabpanel"
      id="sv-panel-{uid}"
      aria-labelledby="sv-tab-{uid}-{view.sections.indexOf(current)}"
    >
      {#if shown?.state === "reading"}
        <p class="sv-note">Reading from {sourceName}…</p>
      {:else if shown?.state === "failed"}
        <p class="sv-note fail">
          Could not read this tab: {shown.message}
          <button class="btn sm" onclick={() => read(current)}>Try again</button>
        </p>
      {:else if shown?.state === "read"}
        {#if shown.blocks.length === 0}
          <p class="sv-note">Nothing here.</p>
        {/if}
        {#each shown.blocks as block, index (index)}
          <div class="sv-block">
            {#if block.type === "facts"}
              <FactsBlock facts={block.facts} />
            {:else if block.type === "text"}
              <TextBlock text={block.text} markdown={block.markdown} {onopenurl} />
            {:else if block.type === "entities"}
              <EntitiesBlock title={block.title} rows={block.rows} {onnavigate} {onopenurl} />
            {:else if block.type === "diff"}
              <DiffBlock files={block.files} truncated={block.truncated} {webUrl} {sourceName} {onopenurl} />
            {:else if block.type === "log"}
              <LogBlock text={block.text} truncated={block.truncated} />
            {:else if block.type === "settings"}
              <SettingsBlock title={block.title} values={block.values} />
            {:else if block.type === "actions"}
              <ActionsBlock actions={block.actions} {onaction} />
            {/if}
          </div>
        {/each}
      {/if}
    </div>
  </div>
{/if}

<style>
  /*
    The tab strip sits flush under the meta grid, the room bar's `.tab`
    treatment at the panel's scale: mono, muted, the open one underlined in
    the text colour. No pills, no fills.
  */
  .sv-tabs {
    display: flex;
    gap: 2px;
    padding: 0 10px;
    border-bottom: 1px solid var(--hair);
    overflow-x: auto;
  }

  .sv-tab {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 30px;
    padding: 0 8px;
    font: 500 12px var(--mono);
    color: var(--muted);
    border-bottom: 2px solid transparent;
    margin-bottom: -1px;
    white-space: nowrap;
  }

  .sv-tab:hover {
    color: var(--text);
  }

  .sv-tab.on {
    color: var(--text);
    border-bottom-color: var(--text);
  }

  .sv-tab:focus-visible {
    outline-offset: -2px;
  }

  .sv-count {
    font: 400 11px var(--mono);
    color: var(--faint);
  }

  .sv-panel {
    padding: 10px 14px 12px;
    border-bottom: 1px solid var(--hair);
  }

  .sv-block + .sv-block {
    margin-top: 12px;
  }

  .sv-note {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    color: var(--muted);
    font-size: 12px;
  }

  .sv-note.fail {
    color: var(--fail);
  }
</style>
