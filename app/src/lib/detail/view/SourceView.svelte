<!--
  A source view (ADR-0020; `CONTEXT.md` **Source view**): what an adapter drew
  for one of its kinds, as tabs over the detail.

  The shell draws the vocabulary and never learns which adapter drew it -- so
  nothing here names a source, a kind or a field. The view's actions are a
  toolbar between the header and the tabs, on every tab (#702); an actions
  block draws one section's own. Either draws only the actions whose op the
  source declares and the shell can queue (#667), and a block never repeats
  one the toolbar has. A settings block is editable only where the source
  declares its op (#668). An action's dialog may offer this entity's own
  settings to change for one write (#669). One component per block, in this
  folder; a block this file does not know is not drawn (the vocabulary is
  frozen, and a new block arrives with its component in the same change).

  **A deferred tab is read once, on its first opening** (rule 3): its title and
  count come with the view, its blocks with `entityView(entity, section)`. The
  answer is kept for as long as the detail is open; a failed read says why and
  offers to try again rather than leaving an empty tab.

  **A section a block names opens in the tab it is drawn in** (#711): a tree's
  entry or a trail's step names a section, read the same way and drawn in
  place of the tab's own, under the same tab. Each tab keeps where it is and
  the steps that led there, so *Back* walks them, switching tabs keeps the
  place, and every place is read once. These steps are the tab's, not the
  app's history: a tab is not an address (#697), so a step inside one is not
  either, and ⌘[ still leaves the entity.
-->
<script lang="ts">
  import { untrack, type Snippet } from "svelte";
  import {
    entityView,
    type ViewAction,
    type ViewBlock,
    type SourceView,
    type ViewSection,
  } from "../../ipc/entity";
  import { ipcErrorMessage } from "../../ipc";
  import { PLACEHOLDER_MS } from "../view-read";
  import type { BuildParameter } from "../../ipc/sources";
  import ActionButtons from "./ActionButtons.svelte";
  import { offeredActions, queueable, type ActionValues } from "./actions";
  import { pendingComments } from "./pending-comments.svelte";
  import { settingsWritable, type SettingMark, type SettingsTarget } from "./settings";
  import DiffBlock from "./DiffBlock.svelte";
  import EntitiesBlock from "./EntitiesBlock.svelte";
  import FactsBlock from "./FactsBlock.svelte";
  import LogBlock from "./LogBlock.svelte";
  import SettingsBlock from "./SettingsBlock.svelte";
  import TextBlock from "./TextBlock.svelte";
  import FileBlock from "./FileBlock.svelte";
  import TreeBlock from "./TreeBlock.svelte";

  let {
    entityId,
    view,
    webUrl,
    sourceName,
    writeOps,
    onnavigate,
    onopenurl,
    onaction,
    settingMarks = {},
    reopenSetting = null,
    onsetting = () => Promise.resolve(false),
    summary,
    said = [],
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
    /**
     * Queue an action's write with its dialog's values and the settings it
     * changed; answers whether it was queued.
     */
    onaction: (action: ViewAction, values: ActionValues, changed: BuildParameter[]) => Promise<boolean>;
    /**
     * The edits of this entity's settings on their way through the queue, by
     * name (#668). Drawn only on a settings block that is of this entity.
     */
    settingMarks?: Record<string, SettingMark>;
    /** A row of this entity's settings to reopen with what was typed (#779). */
    reopenSetting?: { name: string; value: string } | null;
    /** Queue one setting's new value; answers whether it was queued. */
    onsetting?: (block: SettingsTarget, name: string, value: string, base: string | null) => Promise<boolean>;
    /**
     * A line the detail draws between the toolbar and the tabs, on every tab:
     * the builds of the commit this stands at (#732).
     */
    summary?: Snippet;
    /**
     * Words on their way to the discussion (#752): a comment or review queued
     * and not yet settled, drawn muted at the end of the view's **last tab**
     * -- where a discussion sits, and a place the shell can find without
     * naming a section (ADR-0020). Each goes when the queue settles it.
     */
    said?: { id: number; verdict: string | null; text: string; mark: string }[];
  } = $props();

  /** The open tab, by section id. The first section until the reader picks. */
  let picked = $state<string | null>(null);
  const current = $derived(
    view.sections.find((section) => section.id === picked) ?? view.sections[0] ?? null,
  );

  /** Whether the open tab is the last, where words on their way are drawn. */
  const onLastTab = $derived(current !== null && current === view.sections[view.sections.length - 1]);

  /**
   * Where each tab is, by tab id, and the places it was before, newest last
   * (#711). A tab absent here is at its own section.
   */
  let at = $state<Record<string, string>>({});
  let steps = $state<Record<string, string[]>>({});
  /** The place each tab last left, drawn (dimmed) while the next is read. */
  let leftFrom = $state<Record<string, string>>({});

  /** The section a tab draws: its own, or the one the reader walked to. */
  function placeOf(tab: ViewSection): string {
    return at[tab.id] ?? tab.id;
  }

  /** Another entity's view starts every tab at its own section. */
  let seenEntity = untrack(() => entityId);
  $effect(() => {
    const now = entityId;
    if (now === seenEntity) return;
    seenEntity = now;
    generation++;
    untrack(() => {
      at = {};
      steps = {};
      leftFrom = {};
      kept = {};
    });
  });

  /** A section's own read, by id: in flight, failed, or answered. */
  type Read = { state: "reading" } | { state: "failed"; message: string } | { state: "read"; blocks: ViewBlock[] };
  let reads = $state<Record<string, Read>>({});
  /**
   * The reads of the view that was, kept only to be drawn (dimmed) while the
   * same places are read again, so a write's re-read does not blank a tab.
   */
  let kept = $state<Record<string, Read>>({});
  /**
   * Bumped whenever the reads are dropped (another view, another entity): a
   * read answered for the generation before is not this one's to land.
   */
  let generation = 0;

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
    generation++;
    untrack(() => {
      kept = reads;
      reads = {};
    });
  });

  /**
   * This entity's own settings, from every tab already read with the view: what
   * an action whose dialog changes values for one write offers (#669). A
   * settings block of another entity is not this one's to change.
   */
  const ownSettings = $derived(
    view.sections
      .flatMap((section) => section.blocks)
      .flatMap((block) => (block.type === "settings" && block.entity === entityId ? block.values : [])),
  );

  /** Unique per instance, so tab and panel ids cannot collide. */
  const uid = Math.random().toString(36).slice(2, 9);

  function read(id: string) {
    const entity = entityId;
    const mine = generation;
    reads[id] = { state: "reading" };
    entityView(entity, id)
      .then((answer) => {
        if (entity !== entityId || mine !== generation) return;
        const blocks = answer?.sections.find((s) => s.id === id)?.blocks;
        reads[id] =
          blocks === undefined
            ? { state: "failed", message: "the source answered without this section" }
            : { state: "read", blocks };
      })
      .catch((rejection: unknown) => {
        if (entity !== entityId || mine !== generation) return;
        reads[id] = { state: "failed", message: ipcErrorMessage(rejection) };
      });
  }

  function open(section: ViewSection) {
    picked = section.id;
  }

  /** Whether `tab` draws a section it has to read: deferred, or walked to. */
  function readsFor(tab: ViewSection): string | null {
    const place = placeOf(tab);
    return tab.deferred || place !== tab.id ? place : null;
  }

  /** Walk the open tab to `section`, remembering where it was. */
  function walk(section: string) {
    const tab = current;
    if (!tab) return;
    const from = placeOf(tab);
    if (from === section) return;
    // A step taken from a place still being read (the reader clicked in the
    // place held on screen) leaves one never shown: not a step to go back to,
    // and not what stays drawn while the next is read.
    if (shown?.state === "read") {
      steps[tab.id] = [...(steps[tab.id] ?? []), from];
      leftFrom[tab.id] = from;
    }
    at[tab.id] = section;
  }

  /** The open tab back one step. */
  function back() {
    const tab = current;
    const trail = tab ? steps[tab.id] : undefined;
    if (!tab || !trail?.length) return;
    leftFrom[tab.id] = placeOf(tab);
    at[tab.id] = trail[trail.length - 1]!;
    steps[tab.id] = trail.slice(0, -1);
  }

  // The shown place's read, on its first showing -- a click, a key, a step
  // inside the tab, or the view opening on a first section that is deferred.
  $effect(() => {
    const section = current;
    const id = section ? readsFor(section) : null;
    if (id !== null && reads[id] === undefined) untrack(() => read(id));
  });

  /** *Back* for the open tab's blocks, where it has been somewhere before. */
  const onback = $derived(current && (steps[current.id]?.length ?? 0) > 0 ? back : undefined);

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
    const id = readsFor(current);
    const read: Read =
      id !== null ? (reads[id] ?? { state: "reading" }) : { state: "read", blocks: current.blocks };
    if (read.state !== "read") return read;
    return { state: "read", blocks: offered(read.blocks) };
  });

  /**
   * While a place is read, what the tab drew before stays drawn: the place it
   * left for this one, or this same place as the view before a re-read had
   * it. So a quick read swaps one listing for the next with no line between
   * them; a slow one dims it (`.busy`, after {@link PLACEHOLDER_MS}, the
   * panel's one rule for when waiting is worth saying, #703) rather than
   * blanking the tab.
   */
  const held = $derived.by((): ViewBlock[] | null => {
    if (!current || shown?.state !== "reading") return null;
    const from = leftFrom[current.id];
    const place = readsFor(current);
    const candidates = [
      from === undefined ? undefined : reads[from],
      place === null ? undefined : kept[place],
      from === undefined ? undefined : kept[from],
    ];
    const before = candidates.find((read) => read?.state === "read");
    return before?.state === "read" ? offered(before.blocks) : null;
  });

  /** The reader's pending line comments on this entity (#709). */
  const pending = $derived(pendingComments.of(entityId));

  /**
   * The toolbar's actions: those the source declares and the shell can queue,
   * and one that is only the pending line comments only while there are some.
   */
  const toolbar = $derived(offeredActions(view.actions, writeOps, pending.length));

  /**
   * The blocks as drawn: an actions block keeps only the actions the source
   * declares and the shell can queue, less any the toolbar already draws, and
   * goes when none is left.
   */
  function offered(blocks: ViewBlock[]): ViewBlock[] {
    return blocks.flatMap((block): ViewBlock[] => {
      if (block.type !== "actions") return [block];
      const actions = offeredActions(block.actions, writeOps).filter(
        (action) => !toolbar.some((t) => t.write_op === action.write_op && t.label === action.label),
      );
      return actions.length === 0 ? [] : [{ type: "actions", actions }];
    });
  }
</script>

{#snippet blocks(list: ViewBlock[])}
  {#each list as block, index (index)}
    <div class="sv-block">
      {#if block.type === "facts"}
        <FactsBlock facts={block.facts} />
      {:else if block.type === "text"}
        <TextBlock text={block.text} markdown={block.markdown} {onopenurl} />
      {:else if block.type === "entities"}
        <EntitiesBlock title={block.title} rows={block.rows} {onnavigate} {onopenurl} />
      {:else if block.type === "diff"}
        <DiffBlock
          files={block.files}
          truncated={block.truncated}
          {webUrl}
          {sourceName}
          {onopenurl}
          {entityId}
          gone={block.gone ?? []}
          canComment={block.commentable !== null && queueable(block.commentable.write_op, writeOps)}
        />
      {:else if block.type === "log"}
        <LogBlock text={block.text} truncated={block.truncated} />
      {:else if block.type === "settings"}
        <SettingsBlock
          title={block.title}
          values={block.values}
          writable={settingsWritable(block.write_op, writeOps)}
          marks={block.entity === entityId ? settingMarks : {}}
          reopen={block.entity === entityId ? reopenSetting : null}
          onsave={(name, value, base) => onsetting(block, name, value, base)}
        />
      {:else if block.type === "actions"}
        <ActionButtons actions={block.actions} settings={ownSettings} {onaction} />
      {:else if block.type === "tree"}
        <TreeBlock
          trail={block.trail}
          entries={block.entries}
          truncated={block.truncated}
          url={block.url}
          {sourceName}
          onopen={walk}
          {onback}
          {onopenurl}
        />
      {:else if block.type === "file"}
        <FileBlock
          trail={block.trail}
          path={block.path}
          text={block.text}
          language={block.language}
          size={block.size}
          truncated={block.truncated}
          binary={block.binary}
          url={block.url}
          {sourceName}
          onopen={walk}
          {onback}
          {onopenurl}
        />
      {/if}
    </div>
  {/each}
{/snippet}

{#if current}
  <div class="sv">
    {#if toolbar.length > 0}
      <div class="sv-bar" role="toolbar" aria-label="Actions">
        <ActionButtons actions={toolbar} settings={ownSettings} {pending} size="md" {onaction} />
      </div>
    {/if}
    {@render summary?.()}
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
      style:--sv-wait="{PLACEHOLDER_MS}ms"
    >
      {#if held}
        <div class="sv-held busy" aria-busy="true">
          {@render blocks(held)}
        </div>
      {:else if shown?.state === "reading"}
        <p class="sv-note sv-wait">Reading from {sourceName}…</p>
      {:else if shown?.state === "failed"}
        <p class="sv-note fail">
          Could not read this tab: {shown.message}
          <button class="btn sm" onclick={() => read(placeOf(current))}>Try again</button>
        </p>
      {:else if shown?.state === "read"}
        {#if shown.blocks.length === 0}
          <p class="sv-note">Nothing here.</p>
        {/if}
        {#key placeOf(current)}
          {@render blocks(shown.blocks)}
        {/key}
      {/if}
      {#if onLastTab && placeOf(current) === current.id && shown?.state !== "failed"}
        {#each said as item (item.id)}
          <div class="sv-block sv-sending" aria-label="Sending">
            <FactsBlock
              facts={[
                { label: "You", value: item.mark, tone: null },
                ...(item.verdict ? [{ label: "Review", value: item.verdict, tone: null }] : []),
              ]}
            />
            {#if item.text}
              <TextBlock text={item.text} markdown={true} {onopenurl} />
            {/if}
          </div>
        {/each}
      {/if}
    </div>
  </div>
{/if}

<style>
  /*
    The toolbar: the entity's writes, on one hairline-bounded strip between
    the header's meta grid and the tabs, at the meta grid's own inset. Quiet
    until something is primary (filled) or dangerous (the error tone).
  */
  .sv-bar {
    padding: 8px 14px;
    border-bottom: 1px solid var(--hair);
  }

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

  /*
    Dimmed, or said, only once a read takes long enough to notice: the
    panel's placeholder threshold (#703), so a 150 ms read flickers nothing.
  */
  .sv-held.busy {
    animation: sv-dim 0.12s var(--sv-wait) both;
  }

  .sv-wait {
    animation: sv-show 0s var(--sv-wait) both;
  }

  @keyframes sv-show {
    from {
      visibility: hidden;
    }
    to {
      visibility: visible;
    }
  }

  @keyframes sv-dim {
    from {
      opacity: 1;
    }
    to {
      opacity: 0.55;
    }
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

  /*
    Words on their way (#752): the discussion's own facts-then-text, stepped
    back, with a hairline in the colour of something moving on its left. It
    becomes the real entry, in place, when the queue settles it.
  */
  .sv-sending {
    opacity: 0.6;
    padding-left: 10px;
    border-left: 1px solid var(--link);
  }
</style>
