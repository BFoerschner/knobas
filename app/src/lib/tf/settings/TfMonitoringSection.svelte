<!--
  Settings › Monitoring (#831, R-13a.5): prototype-6's `STGV.monitoring` over
  `monitoring_settings` and `set_monitoring_settings`. A field saves when it
  is left; a refused value goes back to the stored one.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import type { MonitoringSettings } from "../../ipc/assets";
  import { push } from "../../shell/toasts.svelte";
  import type { SectionPorts } from "./ports";
  import { boundedNumber } from "./sections";

  let { ports }: { ports: Pick<SectionPorts, "monitoringSettings" | "setMonitoringSettings"> } = $props();

  /** The bounds `knobas_sync::samples` clamps to, as the rooms `MonitoringSection` holds them. */
  const BOUNDS: Record<keyof MonitoringSettings, readonly [number, number]> = {
    sample_retention_days: [1, 3650],
    response_time_warn_ms: [0, 600_000],
  };

  let stored = $state<MonitoringSettings | null>(null);
  /** Why the last value typed into a field was not what got stored, said under that field. */
  let refused = $state<Partial<Record<keyof MonitoringSettings, string>>>({});

  $effect(() => {
    void ports
      .monitoringSettings()
      .then((read) => (stored = read))
      .catch((cause) => push({ text: `Could not read the monitoring settings: ${ipcErrorMessage(cause)}`, tone: "err" }));
  });

  async function save(field: keyof MonitoringSettings, input: HTMLInputElement) {
    if (stored === null) return;
    const [min, max] = BOUNDS[field];
    const value = boundedNumber(input.value, min, max, stored[field]);
    // A value out of range is brought into it, and that is said rather than done silently.
    const typed = Number.parseInt(input.value, 10);
    const inRange = typed >= min && typed <= max;
    refused = { ...refused, [field]: inRange ? undefined : `Takes ${min} to ${max}, so this is ${value}.` };
    if (value === stored[field]) {
      input.value = String(value);
      return;
    }
    try {
      stored = await ports.setMonitoringSettings({ ...stored, [field]: value });
    } catch (cause) {
      refused = { ...refused, [field]: `Not saved: ${ipcErrorMessage(cause)}. Still ${stored[field]}.` };
    }
    input.value = String(stored[field]);
  }
</script>

<header class="tf-stg-h"><h1>Monitoring</h1><p>How Uptime Kuma data is read.</p></header>
{#if stored !== null}
  <div class="tf-stg-row">
    <label class="tf-stg-l" for="tf-mon-slow">Slow above<small>A response slower than this draws as a warning.</small></label>
    <div class="tf-stg-c tf-stg-inl">
      <input
        id="tf-mon-slow"
        type="number"
        min="0"
        max="600000"
        class="tf-stg-in tf-stg-n90"
        value={stored.response_time_warn_ms}
        aria-invalid={refused.response_time_warn_ms ? "true" : undefined}
        aria-describedby={refused.response_time_warn_ms ? "tf-mon-slow-why" : undefined}
        onchange={(event) => void save("response_time_warn_ms", event.currentTarget)}
      />
      <span>ms</span>
      {#if refused.response_time_warn_ms}<small class="tf-stg-why" id="tf-mon-slow-why" role="alert">{refused.response_time_warn_ms}</small>{/if}
    </div>
  </div>
  <div class="tf-stg-row">
    <label class="tf-stg-l" for="tf-mon-keep">Keep samples for</label>
    <div class="tf-stg-c tf-stg-inl">
      <input
        id="tf-mon-keep"
        type="number"
        min="1"
        max="3650"
        class="tf-stg-in tf-stg-n70"
        value={stored.sample_retention_days}
        aria-invalid={refused.sample_retention_days ? "true" : undefined}
        aria-describedby={refused.sample_retention_days ? "tf-mon-keep-why" : undefined}
        onchange={(event) => void save("sample_retention_days", event.currentTarget)}
      />
      <span>days</span>
      {#if refused.sample_retention_days}<small class="tf-stg-why" id="tf-mon-keep-why" role="alert">{refused.sample_retention_days}</small>{/if}
    </div>
  </div>
{/if}
