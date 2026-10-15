<!--
  Settings › Time (#831, R-13a.4): prototype-6's `STGV.time`. The switch is
  `passive_attribution`; the other rows state rules the backend already
  keeps (observations kept 30 days, `time::passive::RETENTION_DAYS`).
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import { push } from "../../shell/toasts.svelte";
  import type { SectionPorts } from "./ports";

  let { ports }: { ports: Pick<SectionPorts, "passiveAttribution" | "setPassiveAttribution"> } = $props();

  /** `null` until the read answers: *unknown* is not *off*. */
  let passive = $state<boolean | null>(null);
  let busy = $state(false);

  $effect(() => {
    void ports
      .passiveAttribution()
      .then((on) => (passive = on))
      .catch((cause) => push({ text: `Could not read the passive setting: ${ipcErrorMessage(cause)}`, tone: "err" }));
  });

  async function flip() {
    if (passive === null || busy) return;
    busy = true;
    try {
      // What the backend stored, not what the click asked for.
      passive = await ports.setPassiveAttribution(!passive);
    } catch (cause) {
      push({ text: `Could not save: ${ipcErrorMessage(cause)}`, tone: "err" });
    } finally {
      busy = false;
    }
  }
</script>

<header class="tf-stg-h"><h1>Time</h1><p>How knobas helps you account for your day.</p></header>
<div class="tf-stg-row">
  <div class="tf-stg-l">
    Passive blocks<small
      >While the knobas window is in front, it notes what you have open (a ticket, pull request, page or asset) and
      offers that time in the day. It never logs it on its own. While another application is in front it records
      nothing, so time in your editor is not in it.</small
    >
  </div>
  <div class="tf-stg-c">
    {#if passive !== null}
      <button class="tf-stg-sw" role="switch" aria-checked={passive} aria-label="Passive blocks" disabled={busy} onclick={flip}
        ><i></i></button
      >
    {/if}
  </div>
</div>
<div class="tf-stg-row">
  <div class="tf-stg-l">
    Kept for<small
      >After that knobas clears what it saw, and those days show "no record" for passive time. Blocks you tracked and
      worklogs are kept.</small
    >
  </div>
  <div class="tf-stg-c"><span>About a month</span></div>
</div>
<div class="tf-stg-row">
  <div class="tf-stg-l">The week</div>
  <div class="tf-stg-c"><span>Monday to Sunday, in minutes, not rounded</span></div>
</div>
<div class="tf-stg-row">
  <div class="tf-stg-l">
    Worklogs go to<small>A label is not a ticket, so time on a label is logged only after you pick a ticket for it.</small>
  </div>
  <div class="tf-stg-c"><span>Jira, through the write queue</span></div>
</div>
