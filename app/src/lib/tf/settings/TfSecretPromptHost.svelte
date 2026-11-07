<!--
  The credential prompt as the shell mounts it, over the page you are on (F-048): it reads the source it was asked about,
  which the shell's own list holds only part of, and draws `TfSecretPrompt`. A source it can't read goes to Settings' page
  for the address, which says what is wrong there. Focus goes back to the link that asked, when that is still there.
-->
<script lang="ts">
  import { health as sharedHealth, type Health } from "../../shell/health.svelte";
  import { storedWhere } from "../../shell/secret-store";
  import type { SourceSummary } from "../../ipc/sources";
  import type { Layers } from "../layers.svelte";
  import { realSettingsPorts, type SettingsPorts } from "./ports";
  import { secretPrompt } from "./secret-prompt.svelte";
  import TfSecretPrompt from "./TfSecretPrompt.svelte";

  let {
    sourceId,
    layers,
    health = sharedHealth,
    ports = realSettingsPorts,
  }: {
    sourceId: string;
    layers: Layers;
    health?: Pick<Health, "get" | "patch">;
    ports?: Pick<SettingsPorts, "listSources" | "listAdapters" | "appStatus" | "setSourceSecret" | "syncNow"> | undefined;
  } = $props();

  let read = $state<{ source: SourceSummary; acceptsAccount: boolean; inMemory: boolean } | null>(null);

  function close() {
    const back = secretPrompt.returnTo;
    // Asked now: signing in can take the link away, as the status bar's goes once the source is well.
    const onThePage = back?.closest(".tf-main") !== null;
    secretPrompt.source = null;
    secretPrompt.returnTo = null;
    // `TfDialog` puts focus back on what held it on opening; a link a mouse pressed need not have held it.
    queueMicrotask(() => {
      if (back?.isConnected) back.focus({ preventScroll: true });
      // A link on the page is the keeper's (`focus-keep.ts`), which notes only the page. The status bar's has nothing else.
      else if (back && !onThePage) document.querySelector<HTMLElement>(".tf-main")?.focus({ preventScroll: true });
    });
  }

  $effect(() => {
    const id = sourceId;
    let live = true;
    void Promise.all([ports.listSources(), ports.listAdapters().catch(() => []), ports.appStatus().catch(() => null)]).then(
      ([sources, adapters, status]) => {
        if (!live) return;
        const source = sources.find((row) => row.id === id);
        if (source === undefined) return leaveFor(id);
        read = {
          source,
          acceptsAccount: adapters.find((adapter) => adapter.id === source.adapter_kind)?.accepts_account ?? false,
          inMemory: status?.secrets_in_memory ?? false,
        };
      },
      () => {
        if (live) leaveFor(id);
      },
    );
    return () => {
      live = false;
    };
  });

  function leaveFor(id: string) {
    secretPrompt.source = null;
    secretPrompt.returnTo = null;
    location.hash = `#/settings/source/${encodeURIComponent(id)}/reenter`;
  }
</script>

{#if read !== null}
  <TfSecretPrompt
    source={read.source}
    health={health.get(read.source.id) ?? null}
    acceptsAccount={read.acceptsAccount}
    stored={storedWhere(read.inMemory)}
    {ports}
    {layers}
    onhealth={(next) => health.patch(next)}
    onclose={close}
  />
{/if}
