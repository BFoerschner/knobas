<!--
  *Create monitor* in the new shell (#864, R-11.4): one dialog, prefilled from
  the asset (`monitors.ts`' `prefill`), with prototype-6's Name, Type and
  Target.

  Two writes in the rooms dialog's order and for its reason
  (`assets/MonitorDialog.svelte`): the name goes onto the asset first
  (`edit_asset`, `field: "monitors"`), because the name is what attaches the
  monitor when the next poll mirrors it; then the create is queued
  (`submit_write`, `CreateMonitor` with its kind).

  Prototype-6's *Every* and its *Cert expiry* type are not drawn: the create
  runs on Uptime Kuma's own one-minute schedule (`knobas_source_kuma::create`)
  and knobas makes no certificate check.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import type { AssetDetail, AssetEdit, AssetRow, MonitorTarget } from "../../ipc/assets";
  import type { MonitorKind, QueuedWrite, WriteOpPayload } from "../../ipc/sources";
  import TfDialog from "../settings/TfDialog.svelte";
  import { prefill } from "./monitors";

  let {
    detail,
    record,
    queue,
    onclose,
    oncreated,
  }: {
    /** The asset the monitor is for, as `get_asset` read it. */
    detail: AssetDetail;
    /** Record the monitor's name on the asset: the first write. */
    record: (assetId: string, edits: AssetEdit[]) => Promise<AssetRow>;
    /** Queue the create: the second. */
    queue: (payload: WriteOpPayload) => Promise<QueuedWrite>;
    onclose: () => void;
    /** Both writes went; the view says what was queued and follows it. */
    oncreated: (what: { name: string; write: QueuedWrite }) => void;
  } = $props();

  /** The kinds offered, in prototype-6's words and order. */
  const KINDS: { value: MonitorKind; label: string }[] = [
    { value: "http", label: "HTTP" },
    { value: "tcp", label: "TCP" },
    { value: "ping", label: "Ping" },
    { value: "docker", label: "Docker" },
  ];

  // svelte-ignore state_referenced_locally
  // Read once: a re-derive would overwrite what the reader has typed.
  const opening = prefill(detail);
  let name = $state(opening.name);
  let kind = $state<MonitorKind>(opening.kind);
  let target = $state(opening.target);
  // svelte-ignore state_referenced_locally
  let source = $state<MonitorTarget | null>(detail.monitor_targets[0] ?? null);

  let writing = $state(false);
  let failure = $state<string | null>(null);

  const uid = $props.id();
  const targets = $derived(detail.monitor_targets);
  const ready = $derived(!writing && name.trim() !== "" && target.trim() !== "" && source !== null);

  async function submit() {
    const chosen = source;
    if (!ready || chosen === null) return;
    writing = true;
    failure = null;
    const wanted = name.trim();
    try {
      await record(detail.asset.id, [{ field: "monitors", added: [wanted] }]);
      const write = await queue({ CreateMonitor: { entity: chosen.entity, name: wanted, url: target.trim(), kind } });
      oncreated({ name: wanted, write });
    } catch (rejection) {
      failure = ipcErrorMessage(rejection);
    } finally {
      writing = false;
    }
  }
</script>

<TfDialog title="Create a monitor for {detail.asset.name}" {onclose}>
  {#snippet body()}
    <div class="tf-formgrid">
      <label for="{uid}-name">Name</label>
      <input id="{uid}-name" type="text" autocomplete="off" bind:value={name} />
      <label for="{uid}-kind">Type</label>
      <select id="{uid}-kind" bind:value={kind}>
        {#each KINDS as option (option.value)}
          <option value={option.value}>{option.label}</option>
        {/each}
      </select>
      <label for="{uid}-target">Target</label>
      <input id="{uid}-target" class="tf-mono" type="text" autocomplete="off" bind:value={target} />
      {#if targets.length > 1}
        <label for="{uid}-src">Create it in</label>
        <select
          id="{uid}-src"
          value={source?.source_id ?? ""}
          onchange={(event) => (source = targets.find((row) => row.source_id === event.currentTarget.value) ?? null)}
        >
          {#each targets as row (row.source_id)}
            <option value={row.source_id}>{row.display_name}</option>
          {/each}
        </select>
      {/if}
    </div>
    {#if source !== null}
      <p class="tf-dim tf-note">Created in {source.display_name} through the write queue, attached to this asset.</p>
    {:else}
      <p class="tf-dim tf-note">
        No monitoring source can create monitors. Add an Uptime Kuma account under Settings › Sources.
      </p>
    {/if}
    {#if failure !== null}
      <p class="tf-mfail" role="alert">{failure}</p>
    {/if}
  {/snippet}
  {#snippet footer()}
    <button type="button" class="tf-btn tf-ghost" onclick={onclose}>Cancel</button>
    <button type="button" class="tf-btn tf-primary" disabled={!ready} onclick={() => void submit()}>
      {#if writing}Queueing…{:else}Create monitor{/if}
    </button>
  {/snippet}
</TfDialog>
