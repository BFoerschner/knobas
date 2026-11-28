<!--
  One run's row (#1452, ux18), as a pipeline's Runs tab and Builds' run scopes (*All pipelines* and a folder, #2570)
  draw it: one component, so the lists read the same. A pipeline's runs lead with the number and its state chip; a run
  scope leads with the chip and gives the next cell to the run's pipeline (#2572, the mockup's *The run row*): its
  folders, quiet, ending at one line, and its name and number starting at it, so down the table the names stand in one
  column. What gives way: the folders from their start (the one nearest the name stays), then the name's lead, then its
  last word (`nameParts`); never the number or the chip. The folders are a link to that folder.

  The rest is the pipeline's row as it was: the bubble's dot hung in the first cell's gutter (#2572), the branch with the
  job's chip, who or what started it, the tests, the duration, the start time, and *Mark as read* (a failed or running
  run), *Log* and *Run again…* on hover. A run in flight counts up in the duration's cell (#2574): the bar of its
  pipeline's usual time filling in the info tone, then its clock, each second; ended, the same bar and the words say
  what it took. The cell is a fixed width and its words start at one x, so queued, running and ended move nothing. A click anywhere else on the row opens the run; the number stays the keyboard's link.
-->
<script lang="ts" module>
  import type { Bubble as DotBubble } from "./builds";

  /** A row's dot in words (#2572): its title, and what a screen reader says for its colour. #2575 owns the wording. */
  const DOT_WORDS: Record<DotBubble, string> = {
    blue: "Running, not opened yet.",
    green: "Passed while you were away.",
    yellow: "Passed with warnings while you were away.",
    red: "Failed, unread. M marks it read.",
    grey: "Cancelled while you were away.",
  };
</script>

<script lang="ts">
  import type { Attachment } from "svelte/attachments";
  import TfKey from "../TfKey.svelte";
  import TfStateChip from "../TfStateChip.svelte";
  import Marked from "../filter/Marked.svelte";
  import { stateWord } from "../state-word";
  import { secondsLabel, tickingLabel, timeLabel } from "../../time/format";
  import { refusedAttrs } from "../../shell/refused-writes";
  import type { RunRow } from "../../ipc/entity";
  import { failedRun, runTone, testsWords, type Bubble } from "./builds";
  import { nameParts } from "./recent";
  import { DURBAR_WIDTH, chipTone, durbarWidth, isStartedHere, triggerLabel, type RunMarks } from "./pipeline";
  import { runsRead } from "./runs-read.svelte";
  import { elapsedSeconds, runClock } from "./run-clock.svelte";

  let {
    run,
    attach,
    href,
    typed = "",
    marks = null,
    job = null,
    jobBranches = [],
    max,
    usual = null,
    arrived = false,
    cannotRun = null,
    inQueue = false,
    pipeline = null,
    onopen,
    onagain,
  }: {
    run: RunRow;
    /** The windowed list's measure of the row. */
    attach: Attachment<HTMLElement>;
    /** The run's page. */
    href: string;
    /** The filter's words, lit where they match while the ranking's answer is out. */
    typed?: string;
    /** The letters the ranking matched, per cell (#2535); `pipeline`'s name and place on *Recent*. */
    marks?: (RunMarks & { name?: readonly number[]; place?: readonly number[] }) | null;
    job?: string | null;
    /** The job's branches: a run on one carries the job's chip. */
    jobBranches?: readonly string[];
    /** The longest duration its bar is measured against. */
    max: number;
    /**
     * Its pipeline's usual time (#2574): the median of the finished runs the list holds (`historyFacts`' *Usual
     * duration*), drawn as the track a running run's bar fills along, on the scale of `max`. `null` while unknown.
     */
    usual?: number | null;
    /** A run a read brought to the top after the list drew: lit once. */
    arrived?: boolean;
    /** Why *Run again…* cannot run, or `null`. */
    cannotRun?: string | null;
    /** A run started here and still in the write queue. */
    inQueue?: boolean;
    /**
     * A run scope's pipeline cell (`rowPipeline`): its name, the folders said before it, their link and the whole path.
     * `null` on a pipeline's own runs.
     */
    pipeline?: { name: string; place: string; folder: string | null; whole: string } | null;
    onopen: (event: MouseEvent) => void;
    onagain: () => void;
  } = $props();

  const tone = $derived(runTone(run));
  const here = $derived(isStartedHere(run));
  const failure = $derived(!here && failedRun(run));
  const unread = $derived(failure && runsRead.unread(run));
  const number = $derived(run.number !== null ? `#${run.number}` : "queued");
  const dot = $derived(here ? null : runsRead.bubble(run));
  const parts = $derived(pipeline === null ? null : nameParts(pipeline.name));
  /** How long a run in flight has run, read again each second while it runs; `null` for any other. */
  const elapsed = $derived(tone === "run" && !here && run.started !== null ? elapsedSeconds(run.started, runClock.now) : null);
</script>

<!-- What a row's dot says (#2572): the bubble that counts its run, in its colour, as words for who can't see it. -->
{#snippet bubbleDot(bubble: Bubble)}
  {@const said = DOT_WORDS[bubble]}
  <span class={`tf-unread tf-u-${bubble}`} title={said}><span class="tf-sr">{said}</span></span>
{/snippet}

<tr {@attach attach} data-run={run.entity_id} data-failed={failure ? "" : undefined} class:tf-row={!here || inQueue} class:tf-run-new={arrived} onclick={onopen}>
  <td class="tf-nowrap tf-run-no">
    <!-- The bubble's dot (ux18, #2572): hung in the cell's gutter, so marking it moves nothing. -->
    {#if dot !== null}{@render bubbleDot(dot)}{/if}
    {#if here}
      <!-- The run's echo (#2104): *queued*, then *sent*; no source state yet, so its chip is the write's (F-352). -->
      <TfStateChip text={stateWord(run.state?.text ?? "queued")} class="tf-info" />
    {:else if pipeline === null}
      <a {href}><b><Marked text={number} {typed} pairs={marks?.number ?? []} /></b></a>
    {/if}
    {#if run.state !== null && !here}<TfStateChip text={tone === "q" ? "Queued" : run.state.text} class={chipTone(run)} />{/if}
  </td>
  {#if pipeline !== null && parts !== null}
    <!-- The folders end at the cell's one line and the name starts at it; `rtl` cuts the folders at their start. -->
    <td class="tf-run-pipe"
      ><span class="tf-run-id" title={pipeline.whole}
        >{#if pipeline.folder !== null}<a class="tf-run-path" href={pipeline.folder} title={`Show the runs in ${pipeline.place}`}
            ><bdi><Marked text={pipeline.place} {typed} pairs={marks?.place ?? []} /> ›</bdi></a
          >{:else}<span class="tf-run-path"></span>{/if}<span class="tf-run-who"
          ><span class="tf-run-nm"
            >{#if parts[0] !== ""}<span class="tf-run-nmh"><Marked text={parts[0]} {typed} pairs={marks?.name ?? []} /></span>{/if}<span class="tf-run-nmt"
              ><Marked text={parts[1]} {typed} pairs={marks?.name ?? []} from={parts[0].length} /></span
            ></span
          ><a class="tf-run-pno" {href}><Marked text={number} {typed} pairs={marks?.number ?? []} /></a></span
        ></span
      ></td
    >
  {/if}
  <!-- A flex row, so the name takes the ellipsis: it is the chip that ties a run to the job, and an inline chip after the name is the first thing cut (F-357). -->
  <td class="tf-mono tf-run-br"
    ><span class="tf-run-brin"
      >{#if run.branch !== null}<span class="tf-run-brname" title={run.branch}><Marked text={run.branch} {typed} pairs={marks?.branch ?? []} /></span>{/if}{#if run.branch !== null && jobBranches.includes(run.branch) && job !== null}<TfKey chip value={job} class="tf-jobchip-sm" />{/if}</span
    ></td
  >
  <td class="tf-dim tf-run-trig" title={triggerLabel(run)}><Marked text={triggerLabel(run)} {typed} pairs={marks?.trigger ?? []} /></td>
  <td class="tf-nowrap tf-run-tests"
    >{#if run.tests !== null}{#if run.tests.failed > 0}<span class="tf-tests-failed">{run.tests.failed} failed</span>,{" "}{/if}{testsWords({ ...run.tests, failed: 0 })}{/if}</td
  >
  <td class="tf-nowrap tf-num tf-run-dur">
    {#if run.duration_seconds !== null}
      <svg class="tf-durbar" width={DURBAR_WIDTH} height="4" aria-hidden="true"
        ><rect width={durbarWidth(run.duration_seconds, max)} height="4" rx="2" /></svg
      >{secondsLabel(run.duration_seconds)}
    {:else if elapsed !== null}
      <!-- The bar's room is held with nothing known (no finished run held), so the clock stands where it will. -->
      <svg class="tf-durbar tf-durbar-live" width={DURBAR_WIDTH} height="4" aria-hidden="true"
        >{#if usual !== null && max > 0}<rect class="tf-durbar-usual" width={Math.min(DURBAR_WIDTH, durbarWidth(usual, max))} height="4" rx="2" />{/if}{#if max > 0}<rect
            class="tf-durbar-fill"
            width={Math.min(DURBAR_WIDTH, durbarWidth(elapsed, max))}
            height="4"
            rx="2"
          />{/if}</svg
      ><span class="tf-info tf-run-clock">{tickingLabel(elapsed)}</span>
    {:else if here}
      <!-- Why it waits (F-352): in the queue, or sent and not yet read back. -->
      <span class="tf-dim">{inQueue ? "in the write queue" : "until the next sync"}</span>
    {:else}
      waiting
    {/if}
  </td>
  <td class="tf-nowrap tf-dim tf-num tf-run-at"><span class="tf-rowval">{run.started !== null ? timeLabel(run.started) : ""}</span></td>
  <td class="tf-nowrap tf-run-acts">
    {#if !here}
      <span class="tf-rowend"><span class="tf-rowacts">
        {#if failure}
          <!-- An icon, the words its name and tip: as words the group ran over the duration and cut it, and a
               press on the duration of a hovered row marked the run (ux18). A tick to read, the dot to unread. -->
          {@const said = unread ? "Mark as read" : "Mark as unread"}
          <button type="button" class="tf-iconbtn tf-sm" aria-label={said} title={`${said} (M)`} onclick={() => void runsRead.toggle(run)}
            ><svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"
              >{#if unread}<path d="M3 8.5l3 3 7-7.5" />{:else}<circle cx="8" cy="8" r="3" fill="currentColor" stroke="none" />{/if}</svg
            ></button
          >
        {/if}
        <a class="tf-btn tf-sm tf-ghost" href={`${href}/log`}>Log</a>
        {#if run.finished !== null}
          <button type="button" class="tf-btn tf-sm tf-ghost" {...refusedAttrs(cannotRun)} onclick={onagain}>Run again…</button>
        {/if}
      </span></span>
    {/if}
  </td>
</tr>
