<!--
  Settings › Sources (#830, R-13a.1): prototype-6's `STGV.sources`. A row per
  source with its state, address, what it holds and its last sync, each
  opening the source's page; *+ Add a source* and *Sync all* above them. A
  credential that expires within two weeks, or has expired, says so under the
  row's state (#1499). With no source yet (ux31, where a fresh profile is sent
  to add one): the restore line, since knobas restores a backup only into a
  database with no data, and on the demo profile *Load the Tidewater dataset*
  beside *Add a source…*.
-->
<script lang="ts">
  import type { SourceSummary, SourceSyncStatus } from "../../ipc/sources";
  import { expiryNote, signInFails, type Health } from "../../shell/health.svelte";
  import { push } from "../../shell/toasts.svelte";
  import { createAllSync } from "../role-sync.svelte";
  import type { SettingsPorts } from "./ports";
  import { sourceDrafts } from "./source-drafts.svelte";
  import { EXPIRY_CLASS, bareAddress, holdings, settingsHash, sourceHash, sourceState } from "./settings";
  import TfLoadDemo from "./TfLoadDemo.svelte";

  let {
    sources,
    statuses,
    health,
    now,
    ports,
    onstarted,
    onstopped,
    onadd,
    demo = false,
    onloaded,
  }: {
    sources: SourceSummary[];
    statuses: Record<string, SourceSyncStatus>;
    health: Pick<Health, "get">;
    now: Date;
    ports: Pick<SettingsPorts, "syncAll" | "syncStatus" | "listen" | "demoLoad">;
    onstarted: (ids: string[]) => void;
    onstopped: (ids: string[]) => void;
    /** Opens *Add a source* (#903). */
    onadd: () => void;
    /** The `--demo` profile (P13): with no source, *Load the Tidewater dataset* stands beside *Add a source…*. */
    demo?: boolean;
    /** The Tidewater dataset loaded: the list is read again. */
    onloaded?: (() => void) | undefined;
  } = $props();

  /** What *Sync all* starts. `trigger_all` skips a source only a person can make answer, which is what `signInFails` reads. */
  const runnable = $derived(sources.filter((source) => source.enabled && !signInFails(health.get(source.id)?.state)));

  /** Each source *Sync all* would start is running already, so a press would add nothing (F-630). */
  const allRunning = $derived(runnable.length > 0 && runnable.every((source) => statuses[source.id]?.running === true));

  /** What started, and who didn't: `runs` already leaves out a source whose sign-in fails, because it gets no run. */
  function startedText(runs: number): string {
    const stuck = sources.filter((source) => source.enabled && signInFails(health.get(source.id)?.state)).map((source) => source.display_name || source.id);
    const syncing = `Syncing ${runs} source${runs === 1 ? "" : "s"}.`;
    return stuck.length === 0 ? syncing : `${syncing} ${new Intl.ListFormat("en").format(stuck)} can't sign in.`;
  }

  async function syncAll() {
    if (allRunning) return;
    const ids = runnable.map((source) => source.id);
    onstarted(ids);
    // One for each press: its answer takes this toast's place when its runs end (F-652).
    const run = createAllSync(ports, async () => {
      const runs = await ports.syncAll();
      push({ text: startedText(runs.length), subject: "sync:all" });
      return runs;
    });
    await run.start();
    if (run.failed !== null) {
      onstopped(ids);
      push({ text: `Could not sync: ${run.failed}`, tone: "err" });
    }
  }
</script>

<header class="tf-stg-h tf-stg-hline">
  <div>
    <h1>Sources</h1>
    <p>Where knobas reads from. Each source syncs on its own schedule; writes go back through the queue.</p>
  </div>
  <div class="tf-acts">
    <button class="tf-btn tf-primary" onclick={onadd}>Add a source…</button>
    <!-- No reseed: Settings reads the list again (`onloaded`), and its read replaces the health store. -->
    {#if demo && sources.length === 0}<TfLoadDemo load={ports.demoLoad} health={null} {onloaded} />{/if}
    {#if sources.length > 0}
      <button class="tf-btn" aria-disabled={allRunning ? "true" : undefined} title={allRunning ? "Every source is syncing" : undefined} onclick={() => void syncAll()}>Sync all</button>
    {/if}
  </div>
</header>
{#if sources.length === 0}
  <p class="tf-stg-p tf-stg-gap">
    No sources yet. Add one and its tickets, pull requests, builds and pages come into knobas.
  </p>
  <p class="tf-stg-p">
    Have a backup or a share export? Restore it before you add a source: knobas restores only into a database that holds no data yet.
    <a href={settingsHash("backup")}>Backup and export</a>
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
          <!-- What was typed on its page and left there (F-617): said here too, or it is easy to forget. -->
          {#if sourceDrafts.has(source.id)}<span class="tf-stg-exp tf-stg-warnt">Changes not saved</span>{/if}
        </span>
        <span class="tf-stg-go" aria-hidden="true">›</span>
      </a>
    {/each}
  </div>
{/if}
