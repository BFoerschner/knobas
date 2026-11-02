<!--
  PROTOTYPE: the board without the lists block (Björn, 2026-11-02: "remove the
  smart lists altogether"). A copy of `launcher/Board.svelte` minus the smart
  lists, their rename field and their Rename/Delete buttons. The inbox, today's
  time, the source strip and the recent items are unchanged. Saved lists are
  reached with `l ` in the box, and renamed or deleted from the ⌘K sheet.
-->
<script lang="ts">
  import type { LauncherHome } from "../../ipc";
  import { clockLabel, minutesLabel } from "../../time/format";
  import { failed, sentence } from "../../tf/inbox/inbox";
  import type { CredentialHealth, SourceSyncStatus } from "../../ipc/sources";
  import { isActionable, type HealthPhase } from "../../shell/health.svelte";
  import { sourceMonogram } from "../../shell/monogram";
  import { ago } from "../../shell/time";
  import { sourceState } from "../../tf/settings/settings";
  import { sharesKey } from "../format";
  import Row from "../Row.svelte";
  import type { LauncherRow } from "../rows";

  let {
    home,
    inboxCount = 0,
    sources,
    runs = [],
    reading: readingProp,
    rows,
    selected,
    optionId,
    now,
    onopen,
    onhover,
  }: {
    home: LauncherHome;
    inboxCount?: number;
    reading?: { phase: HealthPhase; error: string | null; retry: () => void } | undefined;
    sources: CredentialHealth[];
    runs?: SourceSyncStatus[];
    rows: LauncherRow[];
    selected: number;
    optionId: (index: number) => string;
    now?: Date | undefined;
    onopen: (row: LauncherRow) => void;
    onhover: (index: number) => void;
  } = $props();

  const reading = $derived(readingProp ?? { phase: "ready" as const, error: null, retry: () => {} });
  const inboxRows = $derived(rows.flatMap((row, index) => (row.kind === "inbox" ? [{ row, item: row.item, index }] : [])));
  const todayRow = $derived(rows.flatMap((row, index) => (row.kind === "today" ? [{ row, today: row.today, index }] : []))[0]);
  const recentFrom = $derived(rows.findIndex((row) => row.kind === "recent"));
  const shares = $derived(sharesKey(home.recent));
  const strip = $derived(
    sources.map((source) => {
      const actionable = isActionable(source.state);
      const run = sourceState(
        { enabled: true, last_run: null, role_misses: [], auth_kind: "Pat" },
        source,
        runs.find((row) => row.source_id === source.source_id) ?? null,
        now,
      );
      const runWarns = !actionable && run.tone === "warn";
      return {
        id: source.source_id,
        monogram: sourceMonogram(source.source_id),
        failing: actionable,
        warning: runWarns,
        note: actionable ? (source.detail ?? source.state) : runWarns ? run.text : source.checked_at ? `checked ${ago(source.checked_at, now)}` : "not checked yet",
      };
    }),
  );
</script>

{#if inboxRows.length > 0}
  <div class="tf-ln-h tf-ln-inboxh"><span class="tf-ln-lab">Inbox</span><span class="tf-ln-n">{inboxCount}</span></div>
  <div class="tf-ln-inbox">
    {#each inboxRows as { row, item, index } (row.id)}
      <div class="tf-ln-slrow" class:tf-ln-on={index === selected} role="presentation" onmouseenter={() => onhover(index)}>
        <button id={optionId(index)} class="tf-ln-sl" role="option" aria-selected={index === selected} onclick={() => onopen(row)}>
          <span class="tf-ln-nm">{#if failed(item)}<span class="tf-ln-errdot" aria-hidden="true">●</span>{" "}{/if}{sentence(item)}</span>
          <span class="tf-ln-c">{clockLabel(item.occurred_at, now ?? new Date(), true)}</span>
        </button>
      </div>
    {/each}
  </div>
{/if}

{#if todayRow}
  {@const { row, today, index } = todayRow}
  <div class="tf-ln-h"><span class="tf-ln-lab">Today</span><span class="tf-ln-n">time</span></div>
  <div class="tf-ln-today">
    <div class="tf-ln-slrow" class:tf-ln-on={index === selected} role="presentation" onmouseenter={() => onhover(index)}>
      <button id={optionId(index)} class="tf-ln-sl" role="option" aria-selected={index === selected} onclick={() => onopen(row)}>
        <span class="tf-ln-nm"><b>{minutesLabel(today.tracked)}</b> tracked</span>
        <span class="tf-ln-c" class:tf-ln-warn={today.toLog > 0}>{minutesLabel(today.toLog)} to log</span>
      </button>
    </div>
  </div>
{/if}

<div class="tf-ln-h">
  <span class="tf-ln-lab">Sources</span>{#if reading.phase === "ready" || sources.length > 0}<span class="tf-ln-n">{sources.length}</span>{/if}
</div>
{#if sources.length === 0 && reading.phase === "loading"}
  <p class="tf-ln-none">Reading the sources…</p>
{:else if sources.length === 0 && reading.phase === "failed"}
  <p class="tf-ln-none tf-ln-err">Could not read the sources{reading.error ? `: ${reading.error}` : "."}</p>
  <button class="tf-ln-opt" onclick={() => reading.retry()}>
    <span class="tf-ln-nm">Retry</span>
    <span class="tf-ln-s">Read the sources again.</span>
  </button>
{:else if sources.length === 0}
  <p class="tf-ln-none">No source is configured yet.</p>
{:else}
  <div class="tf-ln-strip">
    {#each strip as source (source.id)}
      <span class="tf-ln-src" class:tf-ln-fail={source.failing} class:tf-ln-warn={source.warning}>
        <span class="tf-ln-mg" title={source.id} aria-label={source.id}>{source.monogram}</span>
        <span class="tf-ln-sid">{source.id}</span>
        <span class="tf-ln-note">{source.note}</span>
      </span>
    {/each}
  </div>
{/if}

<div class="tf-ln-h"><span class="tf-ln-lab">Recent</span><span class="tf-ln-n">{home.recent.length}</span></div>
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
      state={item.state}
      named={shares(item)}
      {sources}
      {now}
      id={optionId(index)}
      selected={index === selected}
      onopen={() => onopen(row)}
      onhover={() => onhover(index)}
    />
  {/if}
{/each}

{#if home.recent.length === 0}
  <p class="tf-ln-none">Nothing has been synced yet. Add a source from the gear, or start knobas with <strong>--demo</strong>.</p>
{/if}
