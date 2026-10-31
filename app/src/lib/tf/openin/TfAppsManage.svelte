<!--
  *Manage applications* (#844, R-8.23), prototype-6's `openAppsManage`: the
  applications with the default as a radio button, *↑* to move one up, and
  *Edit…*; *Add an application…* and *Done*. Each change saves the list at
  once (`set_checkout_applications`).
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import { push } from "../../shell/toasts.svelte";
  import { layers as sharedLayers, type Layers } from "../layers.svelte";
  import TfDialog from "../settings/TfDialog.svelte";
  import type { Apps } from "./apps.svelte";
  import type { Sample } from "./openin";
  import TfAppDialog from "./TfAppDialog.svelte";

  let {
    apps,
    sample,
    layers = sharedLayers,
    onclose,
  }: {
    apps: Apps;
    sample: Sample | null;
    layers?: Layers;
    onclose: () => void;
  } = $props();

  /** The edit dialog over this one: an application's place, `"add"`, or nothing. */
  let editing = $state<number | "add" | null>(null);

  const list = $derived(apps.settings?.applications ?? []);
  const current = $derived(apps.settings?.default_application ?? null);

  async function save(next: typeof list, defaultId: string | null) {
    try {
      await apps.save(next, defaultId);
    } catch (cause) {
      push({ text: `Could not save the applications: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }

  function up(index: number) {
    if (index === 0) return;
    const next = [...list];
    const [moved] = next.splice(index, 1);
    next.splice(index - 1, 0, moved!);
    void save(next, current);
  }
</script>

{#if editing === null}
  <TfDialog title="Applications for Open in…" {onclose} {layers}>
    {#snippet body()}
      <p class="tf-dsub">Open in… ▾ lists these, the default first. Pick the default with the round button; ↑ orders the rest. Saved in Settings › Checkouts.</p>
      <div class="tf-applist">
        {#each list as app, index (app.id)}
          <div class="tf-approw">
            <input
              type="radio"
              name="tf-app-default"
              aria-label="Default"
              checked={app.id === current}
              onchange={() => void save(list, app.id)}
            />
            <span class="tf-appname"
              ><b>{app.name}</b><span class="tf-mono tf-dim"
                >{app.command}{#if app.file_command}<br />{app.file_command}{/if}</span
              ></span
            >
            <span class="tf-trail">
              <button type="button" class="tf-btn tf-sm tf-ghost" disabled={index === 0} aria-label="Move up" onclick={() => up(index)}
                >↑</button
              >
              <button type="button" class="tf-btn tf-sm" onclick={() => (editing = index)}>Edit…</button>
            </span>
          </div>
        {/each}
      </div>
      <span class="tf-dim tf-dnote">Pick the default with the round button.</span>
    {/snippet}
    {#snippet footer()}
      <button type="button" class="tf-btn tf-first" onclick={() => (editing = "add")}>Add an application…</button>
      <button type="button" class="tf-btn tf-primary" onclick={onclose}>Done</button>
    {/snippet}
  </TfDialog>
{:else}
  <TfAppDialog {apps} {sample} {layers} index={editing === "add" ? null : editing} onclose={() => (editing = null)} />
{/if}
