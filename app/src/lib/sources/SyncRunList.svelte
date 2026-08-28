<!--
  The sync log — one row per `sync_run`, newest first.

  The backend answers in that order and this component keeps it: the log is
  read from the top, and a view that re-sorted it would be answering a question
  nobody asked while making the first row mean something different from what a
  reader expects.

  Every string that came from a source system — `error` above all — is rendered
  as **text** (gotcha 7).
-->
<script lang="ts">
  import Flap from "../shell/Flap.svelte";
  import { ago } from "../shell/time";
  import type { SyncRunRow } from "../ipc/sources";
  import { formatDuration, runDuration } from "./diagnostics";

  let { runs, now }: { runs: SyncRunRow[]; now: Date } = $props();

  /** How an outcome reads, and how alarmed it looks. */
  function tone(run: SyncRunRow): "plain" | "ok" | "fail" | "amber" {
    if (!run.finished_at) return "amber";
    if (run.outcome === "ok") return "ok";
    if (run.outcome === null) return "plain";
    return "fail";
  }

  function outcome(run: SyncRunRow): string {
    // Not "0 ms · ok": a run in flight has no outcome yet, and borrowing the
    // shape of a finished one is how a stuck sync looks healthy.
    if (!run.finished_at) return "running";
    return run.outcome ?? "—";
  }
</script>

{#if runs.length === 0}
  <div class="empty">
    <p>No sync runs yet.</p>
  </div>
{:else}
  <div class="row hd g5w" aria-hidden="true">
    <span>Source</span>
    <span>Trigger</span>
    <span>Started</span>
    <span>Took</span>
    <span>Outcome</span>
  </div>
  {#each runs as entry (entry.id)}
    <div class="log-run" data-run-id={entry.id}>
      <div class="row g5w">
        <span class="k">{entry.source_id}</span>
        <span class="sub">{entry.trigger}</span>
        <span class="r">{ago(entry.started_at, now)}</span>
        <span class="r">{formatDuration(runDuration(entry.started_at, entry.finished_at))}</span>
        <span class="r">
          <!--
            A flap, because this is a value that changes while you watch: a run
            goes from `running` to its outcome under the reader's eyes, which is
            the whole of spec §2's rule for when a flap is warranted.
          -->
          <Flap value={outcome(entry)} tone={tone(entry)} label="run outcome" />
        </span>
      </div>
      <div class="counts">
        {entry.upserted} upserted · {entry.deleted} deleted · {entry.swept} swept
      </div>
      {#if entry.error}
        <!-- Text: an upstream server wrote this line (gotcha 7). -->
        <div class="log"><span class="fail">{entry.error}</span></div>
      {/if}
    </div>
  {/each}
{/if}

<style>
  .log-run {
    border-bottom: 1px solid var(--hair);
  }

  .log-run .row {
    border-bottom: none;
    cursor: default;
  }

  .counts,
  .log {
    padding: 0 12px 8px 8px;
    font: 400 11px/1.5 var(--mono);
    color: var(--faint);
  }

  .log {
    white-space: pre-wrap;
    word-break: break-word;
  }

  .fail {
    color: var(--fail);
  }
</style>
