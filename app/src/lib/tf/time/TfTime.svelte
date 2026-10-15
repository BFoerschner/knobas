<!--
  The Time view (#827, R-13.4): its header with *Log all…*, the three tabs,
  and the tab the address names. `#/time` and `#/time/<date>` are the Day;
  `#/time/week` and `#/time/standup` are later tickets' (#828, #829) and say
  so until they land.

  *Log all…* goes to the Week, where #828 builds its dialog over
  `log_all_preview` and `log_all`; the Day has no week to preview.
-->
<script lang="ts">
  import type { Timer } from "../../shell/timer.svelte";
  import TfDay from "./TfDay.svelte";

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

  const TABS = [
    ["day", "Day", "#/time"],
    ["week", "Week", "#/time/week"],
    ["standup", "Standup", "#/time/standup"],
  ] as const;
</script>

<header class="tf-fhead">
  <div>
    <h1>Time</h1>
    <div class="tf-sub">What you tracked, what knobas saw while you worked, and what is still to log to Jira.</div>
  </div>
  {#if tab !== "standup"}
    <div class="tf-acts"><a class="tf-btn tf-primary" href="#/time/week">Log all…</a></div>
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
  {:else}
    <div class="tf-tm-empty">
      <p>{tab === "week" ? "Week" : "Standup"} is not built in the new shell yet. The rooms UI still has it.</p>
    </div>
  {/if}
</div>
