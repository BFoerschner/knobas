<!--
  One run's row (#1452, ux18), as a pipeline's Runs tab and Builds' *Recent* (2026-11-25) draw it: one component, so
  the two lists read the same. A pipeline's runs lead with the number and its state chip; *Recent* leads with the chip
  and gives the next cell to the run's pipeline: its name in ink, the number after it, then the folders it sits in as
  quiet text, the folders giving way first (cut at their start, so the nearest folder stays), then the name, never the
  number (Fable's ruling 4).

  The rest is the pipeline's row as it was: the unread dot hung in the first cell's gutter, the branch with the job's
  chip, who or what started it, the tests, the duration bar or *running*, the start time, and *Mark as read*, *Log* and
  *Run again…* on hover. A click anywhere on the row opens the run; the number stays the keyboard's link.
-->
<script lang="ts">
  import type { Attachment } from "svelte/attachments";
  import TfKey from "../TfKey.svelte";
  import TfStateChip from "../TfStateChip.svelte";
  import Marked from "../filter/Marked.svelte";
  import { stateWord } from "../state-word";
  import { secondsLabel, timeLabel } from "../../time/format";
  import { refusedAttrs } from "../../shell/refused-writes";
  import type { RunRow } from "../../ipc/entity";
  import { failedRun, runTone, testsWords } from "./builds";
  import { DURBAR_WIDTH, chipTone, durbarWidth, isStartedHere, triggerLabel } from "./pipeline";
  import { runsRead } from "./runs-read.svelte";

  let {
    run,
    attach,
    href,
    typed = "",
    job = null,
    jobBranches = [],
    max,
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
    /** The filter's words, lit where they match. */
    typed?: string;
    job?: string | null;
    /** The job's branches: a run on one carries the job's chip. */
    jobBranches?: readonly string[];
    /** The longest duration its bar is measured against. */
    max: number;
    /** A run a read brought to the top after the list drew: lit once. */
    arrived?: boolean;
    /** Why *Run again…* cannot run, or `null`. */
    cannotRun?: string | null;
    /** A run started here and still in the write queue. */
    inQueue?: boolean;
    /** *Recent*'s pipeline cell: its name and the quiet text of its place. `null` on a pipeline's own runs. */
    pipeline?: { name: string; place: string } | null;
    onopen: (event: MouseEvent) => void;
    onagain: () => void;
  } = $props();

  const tone = $derived(runTone(run));
  const here = $derived(isStartedHere(run));
  const failure = $derived(!here && failedRun(run));
  const unread = $derived(failure && runsRead.unread(run));
  const number = $derived(run.number !== null ? `#${run.number}` : "queued");
</script>

<tr {@attach attach} data-run={run.entity_id} data-failed={failure ? "" : undefined} class:tf-row={!here || inQueue} class:tf-run-new={arrived} onclick={onopen}>
  <td class="tf-nowrap tf-run-no">
    <!-- Unread (ux18): hung in the cell's gutter, so marking it moves nothing. -->
    {#if unread}<span class="tf-unread" title="Unread. M marks it read."><span class="tf-sr">Unread</span></span>{/if}
    {#if here}
      <!-- The run's echo (#2104): *queued*, then *sent*; no source state yet, so its chip is the write's (F-352). -->
      <TfStateChip text={stateWord(run.state?.text ?? "queued")} class="tf-info" />
    {:else if pipeline === null}
      <a {href}><b><Marked text={number} typed={typed} /></b></a>
    {/if}
    {#if run.state !== null && !here}<TfStateChip text={tone === "q" ? "Queued" : run.state.text} class={chipTone(run)} />{/if}
  </td>
  {#if pipeline !== null}
    <td class="tf-run-pipe"
      ><span class="tf-run-pipein" title={pipeline.place === "" ? pipeline.name : `${pipeline.place} › ${pipeline.name}`}
        ><span class="tf-run-pname"><Marked text={pipeline.name} {typed} /></span
        ><a class="tf-run-pno" {href}><b><Marked text={number} {typed} /></b></a
        >{#if pipeline.place !== ""}<span class="tf-run-place"><bdi><Marked text={pipeline.place} {typed} /></bdi></span>{/if}</span
      ></td
    >
  {/if}
  <!-- A flex row, so the name takes the ellipsis: it is the chip that ties a run to the job, and an inline chip after the name is the first thing cut (F-357). -->
  <td class="tf-mono tf-run-br"
    ><span class="tf-run-brin"
      >{#if run.branch !== null}<span class="tf-run-brname" title={run.branch}><Marked text={run.branch} {typed} /></span>{/if}{#if run.branch !== null && jobBranches.includes(run.branch) && job !== null}<TfKey chip value={job} class="tf-jobchip-sm" />{/if}</span
    ></td
  >
  <td class="tf-dim tf-run-trig" title={triggerLabel(run)}><Marked text={triggerLabel(run)} {typed} /></td>
  <td class="tf-nowrap tf-run-tests"
    >{#if run.tests !== null}{#if run.tests.failed > 0}<span class="tf-tests-failed">{run.tests.failed} failed</span>,{" "}{/if}{testsWords({ ...run.tests, failed: 0 })}{/if}</td
  >
  <td class="tf-nowrap tf-num tf-run-dur">
    {#if run.duration_seconds !== null}
      <svg class="tf-durbar" width={DURBAR_WIDTH} height="4" aria-hidden="true"
        ><rect width={durbarWidth(run.duration_seconds, max)} height="4" rx="2" /></svg
      >{secondsLabel(run.duration_seconds)}
    {:else if tone === "run"}
      <progress class="tf-progress" aria-label="Running"></progress> running
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
