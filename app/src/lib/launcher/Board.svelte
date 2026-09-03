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
    now,
    onopen,
    onhover,
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
    now?: Date | undefined;
    onopen: (row: LauncherRow) => void;
    onhover: (index: number) => void;
  } = $props();

  /** Recent rows start after the lists in the flat selectable list. */
  const recentFrom = $derived(home.smart_lists.length);

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
  <button
    class="sl"
    class:on={i === selected}
    role="option"
    aria-selected={i === selected}
    onclick={() => row && onopen(row)}
    onmouseenter={() => onhover(i)}
  >
    <span class="nm">
      {list.label}
      <small>{list.description}</small>
    </span>
    <span class="c" class:chg={list.changed}>
      {list.count}{#if list.changed}<span class="dot" title="something in it is new">●</span>{/if}
    </span>
  </button>
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
  /* `.sl` — the smart-list line (round 3, lines 380-385). */
  .sl {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 6px;
    align-items: start;
    width: 100%;
    text-align: left;
    padding: 7px 12px;
    min-height: 32px;
    font-size: 12px;
    border-bottom: 1px solid var(--hair);
    color: var(--text);
  }
  .sl:hover,
  .sl.on {
    background: var(--raised);
  }
  .sl.on {
    box-shadow: inset 2px 0 0 var(--amber);
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
