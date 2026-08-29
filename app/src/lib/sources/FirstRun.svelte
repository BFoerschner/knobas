<!--
  The first-run wizard — spec §14a: *"initialize the database → add the first
  source (the §3 flow) → initial sync with progress → land in the launcher."*

  Four panels over the mockup's `.steps` breadcrumb.

  **Known deviation from task 20 step 3.** The brief asks for the Add-source
  flow "embedded inline rather than in a modal; a modal over an otherwise empty
  window is a dialog with nothing behind it". It is reused here exactly as the
  sources view mounts it — `AddSource` *is* a `Modal` and has no inline mode —
  so step 2 does open a dialog over the wizard. Recorded rather than papered
  over: the reasons the brief gives are about how it looks, and giving
  `AddSource` a second presentation is a change to a component the sources view
  also owns. Not attempted inside this PR's review round.

  ## Why completion comes off the channel

  `sync_now_with_progress` resolves with the run's id as soon as the run is
  *recorded* (P3) — not when it finishes. A panel that reported completion from
  the command resolving would offer *Finish* the instant the sync started, over
  a run that has fetched nothing, and the progress bar would be decoration.
  So the phase that matters (`finished` / `failed`) arrives on the channel and
  nowhere else.

  ## …and why there is no timeout

  ADR-0005 — *a run id always comes with an ending* — is what makes waiting on
  that channel safe: whether this wizard started the run, joined one already in
  flight, or arrived after it was over, the ending arrives. So there is
  deliberately **no** timeout here and no *taking longer than expected*
  affordance. ADR-0005 records that as rejected, and says why: a long-but-working
  first sync of a large Jira is indistinguishable from a hang by wall-clock, so
  any threshold is either too short to be safe or too long to help — and if the
  ending contract ever breaks, what is wanted is a red test, not a wizard that
  quietly changes the subject.

  ## What *mirrored N items* counts

  The corpus, read from `SourceSummary.item_count` — never the run's `upserted`.
  `CONTEXT.md` carries the distinction, and the reason it matters is the same
  ADR: adding a source wakes the scheduler, so the run that mirrored everything
  may be one this wizard only joined at its end. Reporting what *that* run wrote
  put *knobas mirrored 0 items* over a full mirror.

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
  import { health as sharedHealth, type Health } from "../shell/health.svelte";
  import {
    demoLoad,
    listSources,
    syncNowWithProgress,
    type SourceSummary,
    type SyncPhase,
    type SyncProgress,
  } from "../ipc/sources";
  import AddSource from "./AddSource.svelte";

  let {
    demo = false,
    source: initialSource = null,
    health = sharedHealth,
    onfinish,
  }: {
    /** The `--demo` profile (P13), which is offered the Tidewater fixture. */
    demo?: boolean;
    /** A source configured already — the wizard resumed, or a test's fixture. */
    source?: SourceSummary | null;
    /**
     * The live `source:health` store, so the shell behind the wizard knows
     * about the source it configures. A prop with the shell's singleton as its
     * default, the way `SourcesView` takes it.
     */
    health?: Health;
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
  let corpus = $state<number | null>(null);
  let elapsed = $state(0);
  let failure = $state<string | null>(null);
  let starting = $state(false);
  let finishing = $state(false);

  const finished = $derived(phase === "finished");
  const failed = $derived(phase === "failed" || failure !== null);

  /**
   * The number the panel puts in front of a person.
   *
   * The corpus once it is known, and the run's own count until then. The two
   * are different halves and `CONTEXT.md` names them: a *Mirror* count is a
   * corpus, *Upserted* is what one run wrote, and *"a run that writes nothing
   * over a full mirror upserted zero"*. While the bar is moving, the run's
   * count is the only number that exists and it is the honest one — it is
   * saying how far this run has got. The sentence at the end is about the
   * mirror, and it has to be true whichever run this wizard ended up watching:
   * adding a source wakes the scheduler, so the run that did the mirroring may
   * be one this wizard only joined at its ending.
   */
  const mirrored = $derived(corpus ?? items);

  /**
   * Read the source's corpus off `SourceSummary.item_count`, which already
   * carries it and already crosses the bridge.
   *
   * Deliberately not widened onto the progress channel: that channel reports
   * one run's progress, and the corpus is not one run's to report.
   */
  async function readCorpus(id: string) {
    try {
      const rows = await listSources();
      corpus = rows.find((row) => row.id === id)?.item_count ?? null;
    } catch {
      // The sync itself worked; a count that could not be re-read is not
      // something to put a failure panel over. The reading falls back to what
      // the run said, which is the only other number there is.
      corpus = null;
    }
  }

  async function sync() {
    if (!source || starting) return;
    const id = source.id;
    starting = true;
    phase = "started";
    items = 0;
    corpus = null;
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
        if (message.phase === "finished") {
          // The run is over, so the mirror is whatever it now is — including
          // when this wizard was handed a run that had already finished and
          // this ending is the only message it ever received.
          void readCorpus(id);
        }
        if (message.phase === "failed") {
          // The message is a line an upstream server wrote, and it is
          // rendered as text. A failed phase with nothing to say still gets a
          // sentence, because "failed" alone is not something a person can act
          // on.
          failure = message.message ?? "The sync failed and said nothing about why.";
        }
      };
      await syncNowWithProgress(id, channel);
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
      // The fallback, for a corpus that cannot be re-read. `demo_load`
      // registers *and* syncs the mock source in one call, so for this one path
      // the two numbers coincide.
      items = report.upserted;
      await readCorpus(report.source_id);
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
              · {mirrored} items · {(elapsed / 1000).toFixed(1)} s
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
          knobas mirrored <b>{mirrored}</b> items. Press <kbd>⌘K</kbd> to search everything you just
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
      // `add_source` emits no `source:health`, and the shell's seed already
      // ran against a database that had no sources in it. Without this the
      // wizard finishes onto a shell that has never heard of the source it
      // just configured: no monogram in the strip, no room tab, and — on the
      // *Skip* path, where no sync follows to change `auth_state` — nothing
      // that would ever tell it.
      health.patch(added.health);
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
