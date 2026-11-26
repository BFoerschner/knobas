<!--
  Settings › Monitoring (#831, R-13a.5): prototype-6's `STGV.monitoring` over
  `monitoring_settings` and `set_monitoring_settings`. A field
  saves on `Enter` or when it is left, says *Saved*, and `Esc` puts the stored value
  back (`saved-field.svelte.ts`); a refused value goes back to the stored one.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import type { MonitoringSettings } from "../../ipc/assets";
  import { push } from "../../shell/toasts.svelte";
  import type { SectionPorts } from "./ports";
  import { SavedWord, savedField } from "./saved-field.svelte";
  import { boundedNumber, clampNote } from "./sections";

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

  const said = { response_time_warn_ms: new SavedWord(), sample_retention_days: new SavedWord() };

  /** The attachment of one field: `Enter`, leaving and `Esc`. */
  const typed = (field: keyof MonitoringSettings) =>
    savedField({
      saved: () => String(stored?.[field] ?? ""),
      save: (input) => save(field, input),
      word: said[field],
      restored: () => (refused = { ...refused, [field]: undefined }),
    });

  /** Whether a new value was stored. */
  async function save(field: keyof MonitoringSettings, input: HTMLInputElement): Promise<boolean> {
    if (stored === null) return false;
    const [min, max] = BOUNDS[field];
    const value = boundedNumber(input.value, min, max, stored[field]);
    // A value out of range is brought into it, and that is said rather than done silently.
    refused = { ...refused, [field]: clampNote(input.value, min, max, value) ?? undefined };
    if (value === stored[field]) {
      input.value = String(value);
      return false;
    }
    let done = true;
    try {
      stored = await ports.setMonitoringSettings({ ...stored, [field]: value });
    } catch (cause) {
      done = false;
      refused = { ...refused, [field]: `Not saved: ${ipcErrorMessage(cause)}. Still ${stored[field]}.` };
    }
    input.value = String(stored[field]);
    return done;
  }
</script>

<header class="tf-stg-h"><h1>Monitoring</h1><p>How Uptime Kuma data is read.</p></header>
{#if stored !== null}
  <div class="tf-stg-row">
    <label class="tf-stg-l" for="tf-mon-slow">Slow above<small>A response slower than this reads as Slow.</small></label>
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
        {@attach typed("response_time_warn_ms")}
      />
      <span>ms</span>
      <small class="tf-stg-saved" role="status">{said.response_time_warn_ms.shown ? "Saved" : ""}</small>
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
        {@attach typed("sample_retention_days")}
      />
      <span>days</span>
      <small class="tf-stg-saved" role="status">{said.sample_retention_days.shown ? "Saved" : ""}</small>
      {#if refused.sample_retention_days}<small class="tf-stg-why" id="tf-mon-keep-why" role="alert">{refused.sample_retention_days}</small>{/if}
    </div>
  </div>
{/if}
