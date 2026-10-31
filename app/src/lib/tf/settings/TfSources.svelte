<!--
  Settings › Sources (#830, R-13a.1): prototype-6's `STGV.sources`. A row per
  source with its state, address, what it holds and its last sync, each
  opening the source's page; *+ Add a source* and *Sync all* above them. A
  credential that expires within two weeks, or has expired, says so under the
  row's state (#1499).
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import type { SourceSummary, SourceSyncStatus } from "../../ipc/sources";
  import { expiryNote, type Health } from "../../shell/health.svelte";
  import { push } from "../../shell/toasts.svelte";
  import type { SettingsPorts } from "./ports";
  import { EXPIRY_CLASS, bareAddress, holdings, sourceHash, sourceState } from "./settings";

  let {
    sources,
    statuses,
    health,
    now,
    ports,
    onstarted,
    onstopped,
    onadd,
  }: {
    sources: SourceSummary[];
    statuses: Record<string, SourceSyncStatus>;
    health: Pick<Health, "get">;
    now: Date;
    ports: Pick<SettingsPorts, "syncAll">;
    onstarted: (ids: string[]) => void;
    onstopped: (ids: string[]) => void;
    /** Opens *Add a source* (#903). */
    onadd: () => void;
  } = $props();

  async function syncAll() {
    const ids = sources.filter((source) => source.enabled).map((source) => source.id);
    onstarted(ids);
    try {
      const runs = await ports.syncAll();
      push({ text: `Syncing all ${runs.length} source${runs.length === 1 ? "" : "s"}` });
    } catch (cause) {
      onstopped(ids);
      push({ text: `Could not sync: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }
</script>

<header class="tf-stg-h tf-stg-srch">
  <div>
    <h1>Sources</h1>
    <p>Where knobas reads from. Each source syncs on its own schedule; writes go back through the queue.</p>
  </div>
  <div class="tf-acts">
    <button class="tf-btn tf-primary" onclick={onadd}>+ Add a source</button>
    {#if sources.length > 0}
      <button class="tf-btn" onclick={() => void syncAll()}>Sync all</button>
    {/if}
  </div>
</header>
{#if sources.length === 0}
  <p class="tf-stg-p tf-stg-gap">
    No sources yet. Add one and its tickets, pull requests, builds and pages come into knobas.
  </p>
{:else}
  <div class="tf-stg-srcs">
    {#each sources as source (source.id)}
      {@const reading = sourceState(source, health.get(source.id), statuses[source.id] ?? null, now)}
      <!-- A credential running out (#1499), only once it is amber: a healthy 90-day token never colours the list. -->
      {@const expiry = expiryNote(health.get(source.id)?.secret_expires_at, now)}
      <a class="tf-stg-src" href={sourceHash(source.id)}>
        <span class="tf-dot tf-{reading.tone}"></span>
        <span class="tf-stg-sn"><b>{source.display_name}</b><span class="tf-dim">{bareAddress(source.base_url)}</span></span>
        <span class="tf-stg-si">{holdings(source)}</span>
        <span class="tf-stg-si {reading.failed ? 'tf-stg-errt' : ''}">
          {reading.text}
          {#if expiry !== null && expiry.tone !== "plain"}
            <span class="tf-stg-exp {EXPIRY_CLASS[expiry.tone]}">Credential {expiry.text}</span>
          {/if}
        </span>
        <span class="tf-stg-go" aria-hidden="true">›</span>
      </a>
    {/each}
  </div>
{/if}
