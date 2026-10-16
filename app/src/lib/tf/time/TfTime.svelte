<!--
  The Time view (#827, R-13.4): its header with *Log all…*, the three tabs,
  and the tab the address names. `#/time` and `#/time/<date>` are the Day;
  `#/time/week` and `#/time/week/<date>` the Week (#828); `#/time/standup` the
  Standup (#829).

  *Log all…* previews the week on screen: the Week's, or the week the Day's
  date falls in (#828). The worklogs it queues are followed here, so one that
  will not go is told even after the dialog closes.
-->
<script lang="ts">
  import { onDestroy } from "svelte";

  import type { Worklog } from "../../ipc/time";
  import { followWrite } from "../../shell/follow-write";
  import type { Timer } from "../../shell/timer.svelte";
  import { dayKey } from "../../time/day";
  import { weekWindows } from "../../time/week";
  import { layers } from "../layers.svelte";
  import TfDay from "./TfDay.svelte";
  import TfLogAll from "./TfLogAll.svelte";
  import TfStandup from "./TfStandup.svelte";
  import TfWeek from "./TfWeek.svelte";
  import { protocolDay } from "./standup-view";

  let {
    ready = true,
    rest,
    timer,
    ontimer,
    onlog,
    revision = 0,
    onchanged,
  }: {
    /** Whether the database answers yet: the day's reads wait for it. */
    ready?: boolean;
    /** What follows `time` in the address. */
    rest: string[];
    timer: Pick<Timer, "current">;
    ontimer: (() => void) | undefined;
    onlog?: ((entityId: string, day: string) => void) | undefined;
    revision?: number;
    onchanged?: (() => void) | undefined;
  } = $props();

  const DATE = /^\d{4}-\d{2}-\d{2}$/;
  const tab = $derived(
    rest[0] === "week" ? "week" : rest[0] === "standup" ? "standup" : "day",
  );
  const day = $derived(rest[0] !== undefined && DATE.test(rest[0]) ? rest[0] : null);
  const week = $derived(rest[0] === "week" && rest[1] !== undefined && DATE.test(rest[1]) ? rest[1] : null);

  /** The week *Log all…* previews: the one on screen. */
  let logAll = $state<ReturnType<typeof weekWindows> | null>(null);
  function openLogAll() {
    logAll = weekWindows((tab === "week" ? week : day) ?? dayKey(new Date()));
  }

  /** The worklogs *Log all* queued, followed until they settle or this view goes (#779). */
  const following: (() => void)[] = [];
  onDestroy(() => {
    for (const stop of following.splice(0)) stop();
  });
  function logged(made: Worklog[]) {
    logAll = null;
    for (const worklog of made) {
      if (worklog.write_queue_id !== null) following.push(followWrite({ id: worklog.write_queue_id }, { name: "Worklog" }));
    }
    // A refusal still wrote the rest of the week, so the tabs re-read either way.
    onchanged?.();
  }

  const TABS = [
    ["day", "Day", "#/time"],
    ["week", "Week", "#/time/week"],
    ["standup", "Standup", "#/time/standup"],
  ] as const;
</script>

<header class="tf-fhead">
  <div>
    <h1>Time</h1>
    <div class="tf-sub">
      {tab === "standup"
        ? `Your standup for ${protocolDay(dayKey(new Date()))}, from what knobas saw since your last working day.`
        : "What you tracked, what knobas saw while you worked, and what is still to log to Jira."}
    </div>
  </div>
  {#if tab !== "standup"}
    <div class="tf-acts"><button class="tf-btn tf-primary" onclick={openLogAll}>Log all…</button></div>
  {/if}
</header>
<nav class="tf-tabs" aria-label="Time">
  {#each TABS as [name, label, href] (name)}
    <a {href} aria-current={name === tab ? "page" : undefined}>{label}</a>
  {/each}
</nav>
<div class="tf-tm-wrap">
  {#if tab === "day"}
    <TfDay databaseReady={ready} {day} {timer} {ontimer} {onlog} {revision} {onchanged} />
  {:else if tab === "week"}
    <TfWeek databaseReady={ready} {week} {timer} {revision} />
  {:else}
    <TfStandup databaseReady={ready} />
  {/if}
</div>
{#if logAll !== null}
  <TfLogAll days={logAll} {timer} {layers} onclose={() => (logAll = null)} ondone={logged} />
{/if}
