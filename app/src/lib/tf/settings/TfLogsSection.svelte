<!--
  Settings › Logs and database (#831, R-13a.8): prototype-6's `STGV.logs`.
  The size and count are `db_stats`, the version `app_status`, the folder the
  rooms UI's `LogsSection` opens (#747), and *Re-index search* `reindex_fts`.
  Prototype-6's "search index up to date" has no field behind it, so it is
  not drawn.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import type { DbStats } from "../../ipc/sources";
  import { push } from "../../shell/toasts.svelte";
  import type { SectionPorts } from "./ports";
  import { databaseWords } from "./sections";

  let { ports }: { ports: Pick<SectionPorts, "appStatus" | "dbStats" | "reindexFts" | "openLogs"> } = $props();

  let stats = $state<DbStats | null>(null);
  let version = $state<string | null>(null);
  let indexing = $state(false);
  let failure = $state<string | null>(null);

  $effect(() => {
    void ports
      .dbStats()
      .then((read) => (stats = read))
      .catch((cause) => (failure = ipcErrorMessage(cause)));
    void ports
      .appStatus()
      .then((read) => (version = read.app_version))
      .catch(() => {});
  });

  async function openLogs() {
    try {
      await ports.openLogs();
    } catch (cause) {
      push({ text: `Could not open the log folder: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }

  async function reindex() {
    indexing = true;
    push({ text: "Re-indexing search: about a minute" });
    try {
      await ports.reindexFts();
      push({ text: "Search is re-indexed" });
    } catch (cause) {
      push({ text: `Could not re-index search: ${ipcErrorMessage(cause)}`, tone: "err" });
    } finally {
      indexing = false;
    }
  }
</script>

<header class="tf-stg-h tf-stg-hline">
  <h1>Logs and database</h1>
  <div class="tf-acts">
    <button class="tf-btn" onclick={() => void openLogs()}>Open the log folder</button>
    <button class="tf-btn tf-ghost" disabled={indexing} onclick={() => void reindex()}>Re-index search</button>
  </div>
</header>
<div class="tf-stg-row">
  <div class="tf-stg-l">Database</div>
  <div class="tf-stg-c">
    {#if failure !== null}
      <span class="tf-stg-errt">Could not read the database: {failure}</span>
    {:else}
      <span>{stats === null ? "—" : databaseWords(stats)}</span>
    {/if}
  </div>
</div>
<div class="tf-stg-row">
  <div class="tf-stg-l">Version</div>
  <div class="tf-stg-c"><span>{version === null ? "—" : `knobas ${version}`}</span></div>
</div>
