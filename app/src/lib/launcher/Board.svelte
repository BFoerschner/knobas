<!--
  What an empty box shows (spec §4): the smart lists with their counts and
  change badges, and the newest items across every kind.

  **What is absent, and why it is absent rather than stubbed.** §4's board also
  lists contexts, an inbox preview and today's time. Contexts need links (M2),
  the inbox needs the notification model (M2) and time needs the timer (M3).
  An empty column with a heading over it teaches a reader that the app is
  broken; nothing at all teaches them what M1 is.

  The recent items use the same `Row` the results use, so their provenance
  looks identical — which is the point of §4 promising it "on every result".
-->
<script lang="ts">
  import type { LauncherHome } from "../ipc";
  import type { CredentialHealth } from "../ipc/sources";
  import { isActionable } from "../shell/health.svelte";
  import { sourceMonogram } from "../shell/monogram";
  import Monogram from "../shell/Monogram.svelte";
  import { ago } from "../shell/time";
  import Row from "./Row.svelte";
  import type { LauncherRow } from "./rows";

  let {
    home,
    sources,
    rows,
    selected,
    renaming,
    now,
    onopen,
    onhover,
    onstartrename,
    onrename,
    ondelete,
  }: {
    home: LauncherHome;
    /**
     * The credential health the strip draws — the launcher's, which is the
     * live `source:health` store when the shell supplies one and the board's
     * own per-opening copy when it does not.
     *
     * Passed in rather than read off `home.sources` here: the chips above this
     * strip are drawn from the live store, and two copies is two chances for
     * the strip and a result row's badge to say different things about the
     * same source — which is the divergence #27's review found and this PR
     * claimed to have closed.
     */
    sources: CredentialHealth[];
    rows: LauncherRow[];
    selected: number;
    /**
     * The saved list whose name is being edited, or `null` (#506, story 58).
     *
     * Owned by the launcher rather than by this component, because the `Tab`
     * chain on a saved row is what starts a rename from the keyboard and the
     * chain is the launcher's — two owners of one editing state is two ways
     * for the input to be open.
     */
    renaming: string | null;
    now?: Date | undefined;
    onopen: (row: LauncherRow) => void;
    onhover: (index: number) => void;
    /** Begin renaming a saved list — the row's name becomes a field. */
    onstartrename: (id: string) => void;
    /**
     * Commit a rename, or abandon it: `label` is `null` for Escape and the
     * blur that follows it.
     *
     * One callback for both, because the launcher has one thing to do either
     * way — close the field — and two would let it close on one path and not
     * the other.
     */
    onrename: (id: string, label: string | null) => void;
    ondelete: (id: string) => void;
  } = $props();

  /** Recent rows start after the lists in the flat selectable list. */
  const recentFrom = $derived(home.smart_lists.length);

  /**
   * The saved list whose *Delete* has been pressed once.
   *
   * Two presses, and the second one is the delete. A saved list is a query
   * somebody wrote down and there is no undo, so the one-pixel miss that costs
   * a row is worth a second press; and the arming lives here rather than in
   * the launcher because it is not a state anything else can act on.
   *
   * Cleared whenever the cursor moves to another row, so a half-armed button
   * cannot sit waiting on a row nobody is looking at.
   */
  let armed = $state<string | null>(null);

  /**
   * The sentence above, as code: an armed *Delete* belongs to the row under
   * the cursor, exactly as `Launcher.svelte`'s action chain belongs to the row
   * it was opened on.
   *
   * Derived rather than cleared by an effect, so the button is never armed and
   * on the wrong row for the tick it would take an effect to notice.
   */
  const confirming = $derived(
    armed !== null && home.smart_lists[selected]?.id === armed ? armed : null,
  );

  /** One line per source: what it is, and whether knobas can still read it. */
  const strip = $derived(
    sources.map((source) => ({
      id: source.source_id,
      monogram: sourceMonogram(source.source_id),
      // `shell/health.svelte` owns the reading of a health state, so this
      // strip and a result row's badge cannot come to different conclusions
      // about the same source.
      failing: isActionable(source.state),
      note: isActionable(source.state)
        ? (source.detail ?? source.state)
        : source.checked_at
          ? `checked ${ago(source.checked_at, now)}`
          : "not checked yet",
    })),
  );
</script>

<div class="secl"><span class="lab">Smart lists</span><span class="n">saved local queries</span></div>
{#each home.smart_lists as list, i (list.id)}
  {@const row = rows[i]}
  <!-- The hover is on the row and not on the button inside it, so moving the
       pointer onto *Rename* or *Delete* selects the row those act on rather
       than leaving the cursor two rows above. -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="slrow" class:on={i === selected} onmouseenter={() => onhover(i)}>
    {#if renaming === list.id}
      <!--
        Renaming in place, on the row it renames. `blur` abandons rather than
        commits, and Enter is the only thing that commits: a name half-typed
        when the reader clicked away is not a name they chose.
      -->
      <!-- svelte-ignore a11y_autofocus -->
      <input
        class="ren"
        type="text"
        autofocus
        value={list.label}
        aria-label="Rename {list.label}"
        onkeydown={(event) => {
          if (event.key === "Enter") {
            event.preventDefault();
            event.stopPropagation();
            onrename(list.id, event.currentTarget.value);
          } else if (event.key === "Escape") {
            event.preventDefault();
            // Never past this input: the launcher's own Escape ladder would
            // read the same keystroke as "clear the box" and then "close".
            event.stopPropagation();
            onrename(list.id, null);
          }
        }}
        onblur={() => onrename(list.id, null)}
      />
    {:else}
      <button
        class="sl"
        role="option"
        aria-selected={i === selected}
        disabled={list.needs_attention}
        onclick={() => row && onopen(row)}
      >
        <span class="nm">
          {list.label}
          <small class:att={list.needs_attention}>{list.description}</small>
        </span>
        <span class="c" class:chg={list.changed}>
          {#if list.needs_attention}
            <span class="att">Needs attention</span>
          {:else}
            {list.count}{#if list.changed}<span class="dot" title="something in it is new"
                >●</span
              >{/if}
          {/if}
        </span>
      </button>
      {#if list.saved}
        <!--
          The two controls a saved list has and a built-in does not. On the row
          rather than in a menu, because there are two of them; reachable from
          the keyboard through the row's `Tab` chain, which is where every
          other per-row action in this box lives.
        -->
        <span class="own">
          <button
            class="mini"
            onclick={() => {
              onhover(i);
              onstartrename(list.id);
            }}>Rename</button
          >
          <button
            class="mini del"
            class:armed={confirming === list.id}
            onclick={() => {
              // Select first: `confirming` belongs to the row under the
              // cursor, so a press on a row the cursor is not on has to move
              // it there or the button could never arm.
              onhover(i);
              if (confirming === list.id) {
                armed = null;
                ondelete(list.id);
              } else {
                armed = list.id;
              }
            }}
          >
            {confirming === list.id ? "Confirm" : "Delete"}
          </button>
        </span>
      {/if}
    {/if}
  </div>
{/each}

<!--
  The source-health strip. Not selectable, and deliberately not: a row here is
  a *reading*, and the thing to do about a bad one is the sources view, which
  `>` already offers. Making it another Enter target would put two different
  meanings on one key.
-->
<!-- The count and the gate read the same list the strip does, not the copy the
     board arrived with — a header saying "2" over one line is the same lie in
     a smaller place. -->
<div class="secl"><span class="lab">Sources</span><span class="n">{sources.length}</span></div>
{#if sources.length === 0}
  <p class="none">No source is configured yet.</p>
{:else}
  <div class="strip">
    {#each strip as source (source.id)}
      <span class="src" class:fail={source.failing}>
        <Monogram text={source.monogram} tone={source.failing ? "err" : "ok"} label={source.id} />
        <span class="sid">{source.id}</span>
        <span class="note">{source.note}</span>
      </span>
    {/each}
  </div>
{/if}

<div class="secl"><span class="lab">Recent</span><span class="n">{home.recent.length}</span></div>
{#each home.recent as item, i (item.entity_id)}
  {@const index = recentFrom + i}
  {@const row = rows[index]}
  {#if row}
    <Row
      entityId={item.entity_id}
      kind={item.kind}
      sourceId={item.source_id}
      title={item.title}
      syncedAt={item.synced_at}
      path={item.path}
      sources={home.sources}
      {now}
      selected={index === selected}
      onopen={() => onopen(row)}
      onhover={() => onhover(index)}
    />
  {/if}
{/each}

{#if home.recent.length === 0}
  <p class="none">
    Nothing has been synced yet. Add a source from the gear, or start knobas
    with <span class="mono">--demo</span>.
  </p>
{/if}

<style>
  .secl {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 24px;
    padding: 0 12px 0 8px;
    border-bottom: 1px solid var(--hair);
    background: var(--panel);
    position: sticky;
    top: 0;
    z-index: 1;
  }
  .n {
    margin-left: auto;
    font: 400 11px var(--mono);
    color: var(--faint);
  }
  /* `.slrow` — the smart-list line and, on a saved one, its two controls. The
     highlight is on the row so that the controls sit inside it rather than
     beside a highlighted button. */
  .slrow {
    display: flex;
    align-items: stretch;
    border-bottom: 1px solid var(--hair);
  }
  .slrow:hover,
  .slrow.on {
    background: var(--raised);
  }
  .slrow.on {
    box-shadow: inset 2px 0 0 var(--amber);
  }
  /* `.sl` — the smart-list line (round 3, lines 380-385). */
  .sl {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 6px;
    align-items: start;
    flex: 1;
    min-width: 0;
    text-align: left;
    padding: 7px 12px;
    min-height: 32px;
    font-size: 12px;
    color: var(--text);
  }
  .sl:disabled {
    cursor: default;
  }
  /* The rename field, sized like the row it replaces so the rail does not
     jump when it opens. */
  .ren {
    flex: 1;
    min-width: 0;
    margin: 5px 12px;
    padding: 2px 6px;
    font: 500 12px var(--mono);
    color: var(--text);
    background: var(--panel);
    border: 1px solid var(--amber);
    border-radius: 3px;
  }
  .own {
    display: flex;
    align-items: center;
    gap: 4px;
    padding-right: 10px;
  }
  .mini {
    font: 400 10px var(--mono);
    color: var(--faint);
    padding: 2px 6px;
    border: 1px solid var(--hair);
    border-radius: 3px;
  }
  .mini:hover {
    color: var(--text);
    border-color: var(--muted);
  }
  .mini.del:hover,
  .mini.armed {
    color: var(--fail);
    border-color: var(--fail);
  }
  /* A saved query today's grammar cannot run: the reason where the blurb goes
     and the words where the count goes, so the row reads as a state and not as
     an empty list. */
  .att {
    color: var(--fail);
  }
  .nm {
    min-width: 0;
  }
  .nm small {
    display: block;
    font: 400 10px/1.35 var(--mono);
    color: var(--faint);
    margin-top: 2px;
  }
  .c {
    font: 500 11px var(--mono);
    color: var(--muted);
    white-space: nowrap;
  }
  .c.chg {
    color: var(--amber);
  }
  .dot {
    margin-left: 4px;
    font-size: 9px;
  }
  .none {
    padding: 14px 12px;
    color: var(--muted);
    font-size: 12px;
    line-height: 1.5;
  }
  .strip {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 14px;
    padding: 8px 12px;
  }
  .src {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font: 400 11px var(--mono);
    color: var(--muted);
  }
  .src.fail {
    color: var(--fail);
  }
  .sid {
    color: var(--text);
  }
  .src.fail .sid {
    color: var(--fail);
  }
  .note {
    color: var(--faint);
  }
  .src.fail .note {
    color: var(--fail);
  }
</style>
