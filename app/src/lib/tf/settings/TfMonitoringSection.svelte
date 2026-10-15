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

  let stored = $state<MonitoringSettings | null>(null);

  $effect(() => {
    void ports
      .monitoringSettings()
      .then((read) => (stored = read))
      .catch((cause) => push({ text: `Could not read the monitoring settings: ${ipcErrorMessage(cause)}`, tone: "err" }));
  });

  async function save(field: keyof MonitoringSettings, input: HTMLInputElement) {
    if (stored === null) return;
    const value = boundedNumber(input.value, 1, 1_000_000, stored[field]);
    if (value === stored[field]) {
      input.value = String(value);
      return;
    }
    try {
      stored = await ports.setMonitoringSettings({ ...stored, [field]: value });
    } catch (cause) {
      push({ text: `Could not save: ${ipcErrorMessage(cause)}`, tone: "err" });
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
        min="1"
        class="tf-stg-in tf-stg-n90"
        value={stored.response_time_warn_ms}
        onchange={(event) => void save("response_time_warn_ms", event.currentTarget)}
      />
      <span>ms</span>
    </div>
  </div>
  <div class="tf-stg-row">
    <label class="tf-stg-l" for="tf-mon-keep">Keep samples for</label>
    <div class="tf-stg-c tf-stg-inl">
      <input
        id="tf-mon-keep"
        type="number"
        min="1"
        class="tf-stg-in tf-stg-n70"
        value={stored.sample_retention_days}
        onchange={(event) => void save("sample_retention_days", event.currentTarget)}
      />
      <span>days</span>
    </div>
  </div>
{/if}
