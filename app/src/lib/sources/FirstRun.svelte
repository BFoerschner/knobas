<!--
  The first-run wizard — spec §14a: *"initialize the database → add the first
  source (the §3 flow) → initial sync with progress → land in the launcher."*

  Four panels over the mockup's `.steps` breadcrumb. The Add-source flow is
  embedded **inline** rather than in a modal: a dialog over an otherwise empty
  window is a dialog with nothing behind it.

  ## Why completion comes off the channel

  `sync_now_with_progress` resolves with the run's id as soon as the run is
  *recorded* (P3) — not when it finishes. A panel that reported completion from
  the command resolving would offer *Finish* the instant the sync started, over
  a run that has fetched nothing, and the progress bar would be decoration.
  So the phase that matters (`finished` / `failed`) arrives on the channel and
  nowhere else.

  ## Progress without an inline style

  A native `<progress>`, and no `style="width: …"`. `style-src 'self'` drops an
  inline style attribute in a bundle and honours it under `just dev`, so a
  computed bar would work all through development and be flat in the shipped
  app. There is no total until a run ends, so the element is indeterminate
  while fetching and the item counter carries the news.
-->
<script lang="ts">
  import { Channel } from "@tauri-apps/api/core";

  import { ipcErrorMessage } from "../ipc";
  import { completeFirstRun } from "../ipc/app";
  import {
    demoLoad,
    syncNowWithProgress,
    type SourceSummary,
    type SyncPhase,
    type SyncProgress,
  } from "../ipc/sources";
  import AddSource from "./AddSource.svelte";

  let {
    demo = false,
    source: initialSource = null,
    onfinish,
  }: {
    /** The `--demo` profile (P13), which is offered the Tidewater fixture. */
    demo?: boolean;
    /** A source configured already — the wizard resumed, or a test's fixture. */
    source?: SourceSummary | null;
    /** Hand the shell back. The wizard has recorded completion by then. */
    onfinish: () => void;
  } = $props();

  const STEPS = ["Database", "Source", "First sync", "Done"] as const;

  let stepIndex = $state(0);
  // svelte-ignore state_referenced_locally
  let source = $state<SourceSummary | null>(initialSource);
  let adding = $state(false);

  let phase = $state<SyncPhase | null>(null);
  let items = $state(0);
  let elapsed = $state(0);
  let failure = $state<string | null>(null);
  let starting = $state(false);
  let finishing = $state(false);

  const finished = $derived(phase === "finished");
  const failed = $derived(phase === "failed" || failure !== null);

  async function sync() {
    if (!source || starting) return;
    starting = true;
    phase = "started";
    items = 0;
    failure = null;

    try {
      // Inside the `try`: `Channel`'s constructor reaches into Tauri's
      // internals, and a failure there is a sync that never started — which
      // the reader has to be told about, not one that is swallowed by the
      // `void` at the call site.
      const channel = new Channel<SyncProgress>();
      channel.onmessage = (message) => {
        phase = message.phase;
        items = message.items;
        elapsed = message.elapsed_ms;
        if (message.phase === "failed") {
          // The message is a line an upstream server wrote, and it is
          // rendered as text. A failed phase with nothing to say still gets a
          // sentence, because "failed" alone is not something a person can act
          // on.
          failure = message.message ?? "The sync failed and said nothing about why.";
        }
      };
      await syncNowWithProgress(source.id, channel);
    } catch (cause) {
      // A rejection here means the run never started at all — a different
      // failure from one the channel reports, and swallowing it would leave
      // the panel waiting for progress that can never arrive.
      phase = "failed";
      failure = ipcErrorMessage(cause);
    } finally {
      starting = false;
    }
  }

  async function loadDemo() {
    starting = true;
    failure = null;
    try {
      const report = await demoLoad();
      items = report.upserted;
      phase = "finished";
      // `demo_load` registers the mock source and syncs it in one call, so
      // there is no separate sync step left to walk.
      stepIndex = 3;
    } catch (cause) {
      failure = ipcErrorMessage(cause);
    } finally {
      starting = false;
    }
  }

  /**
   * Record completion and hand the shell back.
   *
   * *Skip* takes this path too, and deliberately: the source is configured and
   * the sync can be retried from the sources view, so not recording completion
   * would show the wizard again on the next launch over a knobas that is
   * already set up.
   */
  async function finish() {
    if (finishing) return;
    finishing = true;
    try {
      await completeFirstRun();
    } catch (cause) {
      // Landing anyway. The alternative is trapping a person in a wizard
      // because a one-row write failed, and the worst case is that they see it
      // once more.
      failure = ipcErrorMessage(cause);
    } finally {
      finishing = false;
    }
    onfinish();
  }
</script>

<div class="view firstrun">
  <div class="room-bar">
    <h1>Welcome to knobas</h1>
    <span class="kind">first run</span>
  </div>

  <div class="view-b">
    <div class="panel">
      <div class="steps">
        {#each STEPS as label, index (label)}
          <span class={index === stepIndex ? "on" : index < stepIndex ? "done" : ""}>{label}</span>
        {/each}
      </div>

      {#if stepIndex === 0}
        <p class="lead">
          The database is up, migrated and ready — knobas started it for you, on its own port, in
          its own data directory. Nothing else on this machine was touched.
        </p>
        <p class="note">
          Next: point knobas at a system you work in. It mirrors what it finds locally, so search
          and the rooms work without a network round trip.
        </p>
        <div class="acts"><button class="btn pri" onclick={() => (stepIndex = 1)}>Next</button></div>
      {:else if stepIndex === 1}
        {#if source}
          <p class="lead">
            <b>{source.display_name}</b> is configured. knobas has not synced it yet.
          </p>
          <div class="acts">
            <button class="btn" onclick={() => (adding = true)}>Add another</button>
            <button class="btn pri" onclick={() => (stepIndex = 2)}>Next</button>
          </div>
        {:else}
          <p class="lead">Add the first source.</p>
          <div class="modules">
            {#if demo}
              <!--
                First, because it is what a person opening a demo build came
                for. Absent outside the demo profile: P13 keeps demo data and a
                real corpus in separate profiles, so the button that would mix
                them is not drawn rather than drawn and refused.
              -->
              <button class="mod" disabled={starting} onclick={() => void loadDemo()}>
                <span class="nm">Load the Tidewater dataset</span>
                <span class="sub">21 fixture items · no network, no credential</span>
              </button>
            {/if}
            <button class="mod" onclick={() => (adding = true)}>
              <span class="nm">Connect a real system</span>
              <span class="sub">Jira, Gitea, TeamCity · the §3 add-source flow</span>
            </button>
          </div>
          {#if failure}
            <p class="fail">{failure}</p>
          {/if}
        {/if}
      {:else if stepIndex === 2}
        <p class="lead">
          knobas will read everything <b>{source?.display_name}</b> will give it. The first run is
          the slow one; after this it only asks for what changed.
        </p>

        {#if phase === null}
          <div class="acts">
            <button class="btn pri" disabled={starting} onclick={() => void sync()}>
              Start the first sync
            </button>
          </div>
        {:else}
          <div class="progress">
            <!--
              Indeterminate while it runs: there is no total until the run ends
              (`items` grows and nothing knows where it stops), and a bar that
              invented one would be a lie that moves.
            -->
            <progress max={finished ? 1 : undefined} value={finished ? 1 : undefined}></progress>
            <p class="reading">
              <b>{failed ? "failed" : (phase ?? "starting")}</b>
              · {items} items · {(elapsed / 1000).toFixed(1)} s
            </p>
          </div>

          {#if failure}
            <!-- Text: this line came from a source system (gotcha 7). -->
            <p class="fail">{failure}</p>
          {/if}

          <div class="acts">
            {#if failed}
              <button class="btn pri" disabled={starting} onclick={() => void sync()}>Retry</button>
              <button class="btn" disabled={finishing} onclick={() => void finish()}>
                Skip for now
              </button>
            {:else if finished}
              <button class="btn pri" disabled={finishing} onclick={() => void finish()}>
                Finish
              </button>
            {/if}
          </div>
        {/if}
      {:else}
        <p class="lead">
          knobas mirrored <b>{items}</b> items. Press <kbd>⌘K</kbd> to search everything you just
          synced.
        </p>
        <div class="acts">
          <button class="btn pri" disabled={finishing} onclick={() => void finish()}>Finish</button>
        </div>
      {/if}
    </div>
  </div>
</div>

{#if adding}
  <AddSource
    onclose={() => (adding = false)}
    onsaved={(added) => {
      adding = false;
      source = added;
      stepIndex = 2;
    }}
  />
{/if}

<style>
  .panel {
    max-width: 640px;
    padding: 28px 14px;
    margin: 0 auto;
  }

  .lead {
    font-size: 13px;
    line-height: 1.6;
  }

  .note,
  .reading {
    margin-top: 8px;
    font: 400 11.5px/1.6 var(--mono);
    color: var(--muted);
  }

  .fail {
    margin-top: 10px;
    font: 400 11.5px/1.6 var(--mono);
    color: var(--fail);
    white-space: pre-wrap;
    word-break: break-word;
  }

  .acts {
    display: flex;
    gap: 6px;
    margin-top: 16px;
  }

  .modules {
    grid-template-columns: 1fr;
    margin-top: 14px;
  }

  .mod {
    grid-template-columns: 1fr;
  }

  .progress {
    margin-top: 16px;
  }

  progress {
    width: 100%;
    height: 4px;
    accent-color: var(--text);
  }
</style>
