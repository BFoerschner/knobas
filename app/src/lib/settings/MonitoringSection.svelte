<!--
  The **Monitoring** section of the settings view (issue #443, spec #427's
  "Settings keys for the sample retention, the response-time threshold").

  ## Why a section and not two fields

  Because both numbers need a sentence saying what they cost. One of them
  *deletes* — retention is the only control here that throws away history
  nobody asked it to throw away — and the other silently changes what counts
  as a problem, which is the thing an alert is raised about. A pair of
  spinners with four-word labels would be a smaller surface and a worse one.

  ## Why the fields draw the answer and not the click

  `set_monitoring_settings` answers with what is now **stored**, clamped, and
  that answer is what this renders — the rule `BackupSection` and
  `PassiveSection` both follow on this surface. A field that kept the typed
  value would tell the reader they had set a retention of zero days.

  ## What changing the threshold does not do

  It does not redraw history. *Warn* is derived at sample time and stored with
  the sample, so a new threshold decides the next poll and leaves every hour
  already recorded as it was recorded. The sentence under the field says so,
  because the obvious assumption is the other one.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../ipc";
  import {
    monitoringSettings as realRead,
    setMonitoringSettings as realWrite,
    type MonitoringSettings,
  } from "../ipc/assets";
  import { latestRead } from "../shell/latest-read";
  import { retentionDaysSentence, thresholdSentence } from "./monitoring";

  let {
    ports,
  }: {
    /** The bridge, injectable so a test needs no Tauri — `PassiveSection`'s shape. */
    ports?: Partial<{
      monitoringSettings: typeof realRead;
      setMonitoringSettings: typeof realWrite;
    }>;
  } = $props();

  // svelte-ignore state_referenced_locally
  const io = {
    monitoringSettings: realRead,
    setMonitoringSettings: realWrite,
    ...ports,
  };

  /** `null` until the first read answers — *unknown*, which is not *the defaults*. */
  let stored = $state<MonitoringSettings | null>(null);
  let failure = $state<string | null>(null);
  let saving = $state(false);

  /**
   * The edited copy. Always an object and never `null`, because `bind:value`
   * re-reads its getter on a later tick — `BackupSection`'s `draft` for the
   * same reason.
   */
  let draft = $state<MonitoringSettings>({
    sample_retention_days: 90,
    response_time_warn_ms: 1500,
  });

  /** Which read is the current one (#107: two saves, two answers, no order). */
  const read = latestRead<MonitoringSettings>();

  function adopt(current: MonitoringSettings) {
    stored = current;
    draft = { ...current };
    failure = null;
  }

  function load() {
    return read(io.monitoringSettings, {
      ok: adopt,
      fail: (cause) => {
        // Not the defaults: "ninety days" is a claim about what is stored, and
        // a section that could not ask has not earned it.
        failure = ipcErrorMessage(cause);
      },
    });
  }

  $effect(() => {
    void load();
  });

  /**
   * A number field's value, never `NaN`.
   *
   * An `<input type="number">` a person has cleared reads as an empty string,
   * which `Number()` makes `NaN` and `JSON.stringify` makes `null` — and the
   * `u32` on the other side refuses to decode that, so the save comes back as
   * a rejection instead of a saved setting. Clearing a field before typing
   * into it is the ordinary way to use one, so this is the common path.
   *
   * The bounds are the ones `knobas_sync::samples` clamps to, and the backend
   * clamps anyway: this is so the reader sees the refusal in the field they
   * typed in rather than in the answer that came back.
   */
  function bounded(value: number, min: number, max: number, fallback: number): number {
    const parsed = Number.parseInt(String(value), 10);
    if (Number.isNaN(parsed)) return fallback;
    return Math.min(max, Math.max(min, parsed));
  }

  async function save() {
    const next: MonitoringSettings = {
      sample_retention_days: bounded(draft.sample_retention_days, 1, 3650, 90),
      response_time_warn_ms: bounded(draft.response_time_warn_ms, 0, 600_000, 1500),
    };
    saving = true;
    try {
      const saved = await io.setMonitoringSettings(next);
      await read(() => Promise.resolve(saved), { ok: adopt, fail: () => {} });
    } catch (cause) {
      failure = ipcErrorMessage(cause);
    } finally {
      saving = false;
    }
  }

  const edited = $derived(
    stored !== null &&
      (Number(draft.sample_retention_days) !== stored.sample_retention_days ||
        Number(draft.response_time_warn_ms) !== stored.response_time_warn_ms),
  );
</script>

<div class="tile-h">
  <span class="lab">Monitoring</span>
</div>

<div class="sec-b">
  <p>
    Every poll of a monitoring source leaves one reading per monitor behind, so
    that a monitor's past outlives the source's own pruning. The Monitors tab
    draws its bar from those readings.
  </p>

  {#if failure}
    <p class="fail">{failure}</p>
    <button class="btn" onclick={() => void load()}>Retry</button>
  {:else if stored !== null}
    <div class="flds">
      <span class="fld">
        <label for="mon-retention">Keep readings for</label>
        <input
          id="mon-retention"
          type="number"
          min="1"
          max="3650"
          bind:value={draft.sample_retention_days}
        />
        <span class="unit">days</span>
      </span>
      <span class="fld">
        <label for="mon-threshold">Warn above</label>
        <input
          id="mon-threshold"
          type="number"
          min="0"
          max="600000"
          bind:value={draft.response_time_warn_ms}
        />
        <span class="unit">ms</span>
      </span>
    </div>

    <p class="sub">{retentionDaysSentence(stored.sample_retention_days)}</p>
    <p class="sub">{thresholdSentence(stored.response_time_warn_ms)}</p>

    <button class="btn pri" disabled={saving || !edited} onclick={() => void save()}>Save</button>
  {/if}
</div>

<style>
  .sec-b {
    padding: 12px;
    font-size: 12px;
    line-height: 1.6;
  }

  .sec-b p {
    max-width: 78ch;
  }

  .sub {
    color: var(--muted);
  }

  .fail {
    color: var(--fail);
  }

  .flds {
    display: flex;
    gap: 18px;
    align-items: center;
    margin: 12px 0 8px;
    flex-wrap: wrap;
  }

  .fld {
    display: flex;
    gap: 6px;
    align-items: center;
  }

  .fld input {
    width: 7ch;
    font: inherit;
  }

  .unit {
    color: var(--muted);
  }

  .btn {
    margin-top: 10px;
  }
</style>
