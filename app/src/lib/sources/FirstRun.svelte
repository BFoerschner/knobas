<!--
  The first-run wizard — spec §14a: *"initialize the database → add the first
  source (the §3 flow) → initial sync with progress → land in the launcher."*

  Four panels under a step breadcrumb, and **DONE is the terminal
  one for a real source and a demo load alike** (#156, ruled 2026-08-29). It was
  not always: `stepIndex = 3` was assigned only in `loadDemo`, so a real first
  sync ended at step 2 offering a bare *Finish*, and the sentence below —
  *knobas mirrored N items*, which #84/ADR-0005, #120 and #137 were each ruled
  about — could only ever be read after a demo load. The breadcrumb drew *Done*
  for everybody regardless. The turnover is off the run's **ending** and costs
  no extra click: a finished step 2 offered one button, DONE offers one button,
  and what the reader gains is the sentence and the ⌘K pointer. A *failed*
  ending stays at step 2, where *Retry* and *Skip for now* are.

  ## The ticket-first look (#1400)

  A full view in the `.tf` scope, drawn with `tf.css` (`css/first-run.css`):
  the page title, a *First run* chip, the step breadcrumb in sentence case,
  `tf-btn` buttons. The shell is not mounted while the wizard has the window,
  so the wizard is its own `.tf` root and answers `Esc` for its dialog itself.

  *Connect a real system* opens **the dialog Settings opens**, `TfAddSource`,
  with the ports Settings gives it, so *Install an adapter* (ADR-0028) is here
  too and there is one add-source flow (#1399). It is a dialog over the
  wizard, as the rooms `AddSource` was: task 20 step 3 asked for the flow
  inline, and that deviation stands, recorded rather than papered over.

  ## A way out (#1399)

  Spec §14a's reason for the wizard is guidance, not a gate. *Set up later in
  Settings* on the Source step records completion as *Finish* does and lands
  on Settings › Sources, so a reader with no built-in source or no credential
  yet is not held here on every launch.

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

  …and the corpus read is a **round trip**, fired when the run ends, so there is
  a window in which the run is over and the count is not back. The panel has a
  state for that window (`corpusPending`) and renders it as pending rather than
  as a number, because the only number it holds during it is the run's — the
  very one the paragraph above is about. Awaiting the read before showing the
  panel was the other way to close it, and it puts a round trip between the
  ending and the news that the sync worked; this way the panel turns over the
  instant the run does and only the count arrives late. On a fast machine the
  window is one microtask, which is exactly why it needs a test that widens it
  (`FirstRun.test.svelte.ts` holds `list_sources` behind a latch) rather than an
  eye.

  …and when the read cannot be had at all, the panel says *knobas mirrored your
  items* and no number (#137). The count is the only part of that sentence it
  cannot vouch for, so the count is the only part it drops; the run's own
  `Upserted` does not stand in for it. See `mirrored`.

  ## Progress without an inline style

  A native `<progress>`, and no `style="width: …"`. `style-src 'self'` drops an
  inline style attribute in a bundle and honours it under `just dev`, so a
  computed bar would work all through development and be flat in the shipped
  app. Items have no total until a run ends, so the bar never draws them.

  It draws the run's **walk** instead (#1379): an adapter that reports how many
  units of its outer walk are done, and of how many, gets `<progress max={of}
  value={walked}>` and a reading that names the unit — *120 of 300
  repositories · 0 items · 8.3 s*. That is what moves while an unchanged run
  pushes nothing, the case a customer read as stuck (#1378). An adapter that
  reports no total, or no walk at all, keeps the indeterminate bar.
-->
<script lang="ts" module>
  /** Where the shell lands once the wizard hands it back. */
  export type FirstRunLanding = "tickets" | "sources" | "backup";
</script>

<script lang="ts">
  import { Channel } from "@tauri-apps/api/core";
  import { walkAfter, walkMax, type Walk } from "./walk";

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
  import { layers as sharedLayers, type Layers } from "../tf/layers.svelte";
  import { realSettingsPorts, type SettingsPorts } from "../tf/settings/ports";
  import TfAddSource from "../tf/settings/TfAddSource.svelte";

  let {
    demo = false,
    source: initialSource = null,
    health = sharedHealth,
    ports = realSettingsPorts,
    layers = sharedLayers,
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
    /** What *Add a source* reads and writes: the ports Settings gives it. */
    ports?: Pick<SettingsPorts, "listAdapters" | "testSource" | "addSource" | "previewInstallAdapter" | "installAdapter">;
    /** The layer stack the dialog registers with, so `Esc` can close it. */
    layers?: Layers;
    /**
     * Hand the shell back. The wizard has recorded completion by then.
     * `tickets` is §14a's landing; `sources` is Settings › Sources, for a
     * reader who set up later (#1399); `backup` is Settings › Backup and
     * export, for a reader who restores before adding a source (#1469).
     */
    onfinish: (landing: FirstRunLanding) => void;
  } = $props();

  const STEPS = ["Database", "Source", "First sync", "Done"] as const;

  let stepIndex = $state(0);
  // svelte-ignore state_referenced_locally
  let source = $state<SourceSummary | null>(initialSource);
  let adding = $state(false);

  let phase = $state<SyncPhase | null>(null);
  let items = $state(0);
  /**
   * The corpus, and **three states, not two**.
   *
   * `undefined` is *nobody has asked yet, or the answer has not come back*;
   * `null` is *asked, and there is no count to be had* (the source is not in
   * the answer, or the read threw); a number is the count. Collapsing the first
   * two is what put *knobas mirrored 0 items* over a full mirror for as long as
   * `list_sources` took to answer: the read is fired when the run ends and the
   * panel renders before it returns, so a state that could not say *not yet*
   * had to say something, and what it said was the run's own count — which for
   * the run this wizard usually ends up watching is zero.
   */
  let corpus = $state<number | null | undefined>(undefined);
  let elapsed = $state(0);
  /** The latest walk reading (`walkAfter`), or `null` before the run reported one. */
  let walk = $state<Walk | null>(null);
  const walkOf = $derived(walkMax(walk));
  /** `120 of 300 repositories · `, or `7 pages · ` with no total, or nothing. */
  const walkReading = $derived(
    walk === null ? "" : `${walk.walked}${walk.of === undefined ? "" : ` of ${walk.of}`} ${walk.unit} · `,
  );
  let failure = $state<string | null>(null);
  let starting = $state(false);
  let finishing = $state(false);

  const finished = $derived(phase === "finished");
  const failed = $derived(phase === "failed" || failure !== null);

  /**
   * The run has ended and the corpus read that settles the sentence has not
   * answered yet.
   *
   * Only after the ending, because that is the only moment the claim changes
   * hands: while the bar is moving the run's own count is the honest number and
   * nothing is pending. A *failed* run is not pending either — no corpus read
   * was fired, and the count beside a failure is the run's.
   */
  const corpusPending = $derived(finished && corpus === undefined);

  /**
   * The number the panel puts in front of a person — or an ellipsis while it
   * genuinely does not have one.
   *
   * The corpus once it is known, and the run's own count until the run ends.
   * The two are different halves and `CONTEXT.md` names them: a *Mirror* count
   * is a corpus, *Upserted* is what one run wrote, and *"a run that writes
   * nothing over a full mirror upserted zero"*. While the bar is moving, the
   * run's count is the only number that exists and it is the honest one — it is
   * saying how far this run has got. The sentence at the end is about the
   * mirror, and it has to be true whichever run this wizard ended up watching:
   * adding a source wakes the scheduler, so the run that did the mirroring may
   * be one this wizard only joined at its ending.
   *
   * Which leaves the gap between those two: the run has ended, so the run's
   * count is no longer the claim being made, and the corpus is not back yet.
   * The panel says so rather than filling it with the number it happens to
   * hold — *waiting on a count* is a true thing to render, and it is not
   * *mirrored 0 items*. A mirror that really is empty is `0` here and reads as
   * zero, which is the distinction this whole tri-state exists to make.
   *
   * **And when the corpus cannot be read at all, there is no number** (#137,
   * ruled 2026-08-29). `readCorpus` settles on `null` when the source is not in
   * the answer or when the read threw twice, and the panel then renders `null`
   * here — *no digit after the word "mirrored" that this panel cannot vouch for
   * as a corpus count*. It used to fall back to `items`, on the reasoning that
   * the run's count "is the only other number there is": true, and the wrong
   * conclusion. It is a per-run `Upserted`, `CONTEXT.md` forbids *mirrored* for
   * it by name, and it is nearest to zero in exactly the case ADR-0005 was
   * written for — the wizard that joined a deduplicated run over a full mirror.
   * A number the panel distrusts is worth less than no number, and this is the
   * first sentence knobas ever says about a source.
   *
   * `items` survives here only in its other job: naming **one run's live
   * progress** while the bar is moving, and the count beside a failure, neither
   * of which claims to be the mirror.
   */
  const mirrored = $derived.by(() => {
    if (corpusPending) return "…";
    // Asked, and there is no count to be had. The one state that renders no
    // number at all.
    if (corpus === null) return null;
    // Nobody has asked yet, which — given `corpusPending` above — means the run
    // has not ended. Spelled out rather than left as a `?? items` tail on the
    // line below, because a tail reads as the fallback this issue deleted and
    // the next reader would be right to delete it. This is the other job.
    if (corpus === undefined) return String(items);
    return String(corpus);
  });

  /**
   * The stats row's `· N items ·` segment, or a dash where the count would go.
   *
   * The same rule as the sentence and for the same reason: the row sat beside
   * `finished`, so a number in it was read as what was mirrored. A dash rather
   * than dropping the segment, so the row keeps its shape between a pending
   * read, a count and no count.
   *
   * **Both of the other two states are now unreachable here, and are kept on
   * purpose** (#156). The row renders only at step 2, and since the ending
   * moves a real source to DONE this derived can no longer see either of the
   * states the paragraph above is about: `"…"` needs `corpusPending`, and
   * `null` needs a settled `readCorpus`, and both need `finished`. So the row
   * only ever reads `· N items ·` off the run's own progress now, and the two
   * remaining branches are dead — untested with it, since the assertions that
   * pinned them moved to the DONE panel where the states actually render.
   *
   * They stay because **retiring them retires the surface #137's constraint 3
   * was placed on**, and re-aiming a ruling is Björn's call, not an
   * implementer's or a reviewer's — the same fork this issue's ruling
   * deliberately left open. If he takes fork 2 (the real path ends at step 2)
   * every branch here is load-bearing again; if he does not, this derived
   * collapses to a template literal and the dash goes with it.
   */
  const itemsReading = $derived(mirrored === null ? "—" : `${mirrored} items`);

  /**
   * Read the source's corpus off `SourceSummary.item_count`, which already
   * carries it and already crosses the bridge.
   *
   * Deliberately not widened onto the progress channel: that channel reports
   * one run's progress, and the corpus is not one run's to report.
   */
  async function readCorpus(id: string) {
    // One immediate retry, then concede (#137). The panel is already rendering
    // the wait as `…`, so a second attempt costs nothing visible, and one
    // `list_sources` failing at this exact instant is likelier transient than
    // terminal. It changes the odds, not the claim: after the last attempt the
    // panel still refuses to put a number it cannot vouch for in front of a
    // person, which is what the `null` below is.
    for (let attempt = 0; attempt < 2; attempt += 1) {
      try {
        const rows = await listSources();
        // Resolved, so this is the answer whatever it says: a source missing
        // from it has no count to retry for.
        corpus = rows.find((row) => row.id === id)?.item_count ?? null;
        return;
      } catch {
        // The sync itself worked; a count that could not be re-read is not
        // something to put a failure panel over.
      }
    }
    corpus = null;
  }

  async function sync() {
    if (!source || starting) return;
    const id = source.id;
    starting = true;
    phase = "started";
    items = 0;
    walk = null;
    corpus = undefined;
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
        walk = walkAfter(walk, message);
        if (message.phase === "finished") {
          // The run is over, so the mirror is whatever it now is — including
          // when this wizard was handed a run that had already finished and
          // this ending is the only message it ever received.
          //
          // Deliberately not awaited: this is a channel callback, and holding
          // the panel's turnover behind a `list_sources` round trip would make
          // the news that the sync worked arrive later than the sync did.
          // `corpusPending` is what makes that safe — the panel can render
          // without a count because it has a way to say it has not got one.
          void readCorpus(id);
          // **DONE, for a real source too** (#156). This used to be assigned in
          // exactly one place, `loadDemo`, so a real first sync ended at step 2
          // under a bare *Finish* and the sentence below — the one #84/ADR-0005,
          // #120 and #137 were each ruled about — rendered only after a demo
          // load, over a breadcrumb that promised *Done* to everybody.
          //
          // Off the **ending**, so it covers every shape ADR-0005 enumerates:
          // this wizard started the run, joined it in flight, or was served a
          // terminal message synthesised for a run already over. Only
          // `finished`; a failure stays at step 2 with its Retry.
          stepIndex = 3;
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
      // No fallback to `report.upserted`, even though `demo_load` registers
      // *and* syncs in one call so the two numbers coincide here. That makes
      // the fallback harmless on this path, not right (#137): one rule
      // everywhere, and a demo load whose re-read fails says *your items* like
      // any other.
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
   * *Skip* and *Set up later in Settings* (#1399) take this path too, and deliberately: the source is configured and
   * the sync can be retried from the sources view, so not recording completion
   * would show the wizard again on the next launch over a knobas that is
   * already set up.
   */
  async function finish(landing: FirstRunLanding) {
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
    onfinish(landing);
  }

  /**
   * `Esc` over the wizard. The shell's ladder (`tf/escape.ts`) is not mounted
   * while the wizard has the window, so this is its rung 1 and nothing more:
   * close the topmost dialog. With none open, `Esc` does nothing, as on
   * Tickets.
   */
  function onkeydown(event: KeyboardEvent) {
    if (event.key !== "Escape" || event.defaultPrevented) return;
    const open = layers.open;
    const index = open.map((layer) => layer.kind).lastIndexOf("dialog");
    if (index === -1) return;
    event.preventDefault();
    open[index]?.close();
  }
</script>

<svelte:window {onkeydown} />

<div class="tf tf-fr-root">
  <main class="tf-main">
    <div class="tf-full tf-fr">
      <header class="tf-fhead">
        <div class="tf-fr-title">
          <h1>Welcome to knobas</h1>
          <span class="tf-chip">First run</span>
        </div>
      </header>

      <div class="tf-fr-body">
        <ol class="tf-fr-steps" aria-label="Setup">
          {#each STEPS as label, index (label)}
            <li class:tf-fr-done={index < stepIndex} aria-current={index === stepIndex ? "step" : undefined}>
              {label}
            </li>
          {/each}
        </ol>

        {#if stepIndex === 0}
          <p class="tf-fr-lead">
            The database is up, migrated and ready — knobas started it for you, on its own port, in
            its own data directory. Nothing else on this machine was touched.
          </p>
          <p class="tf-fr-note">
            Next: point knobas at a system you work in. It mirrors what it finds locally, so search
            and every page work without a network round trip.
          </p>
          <div class="tf-fr-acts">
            <button class="tf-btn tf-primary" onclick={() => (stepIndex = 1)}>Next</button>
          </div>
        {:else if stepIndex === 1}
          {#if source}
            <p class="tf-fr-lead">
              <b>{source.display_name}</b> is configured. knobas has not synced it yet.
            </p>
            <div class="tf-fr-acts">
              <button class="tf-btn" onclick={() => (adding = true)}>Add another</button>
              <button class="tf-btn tf-primary" onclick={() => (stepIndex = 2)}>Next</button>
              {@render later()}
            </div>
          {:else}
            <p class="tf-fr-lead">Add the first source.</p>
            <div class="tf-fr-mods">
              {#if demo}
                <!--
                  First, because it is what a person opening a demo build came
                  for. Absent outside the demo profile: P13 keeps demo data and a
                  real corpus in separate profiles, so the button that would mix
                  them is not drawn rather than drawn and refused.
                -->
                <button class="tf-fr-mod" disabled={starting} onclick={() => void loadDemo()}>
                  <span class="tf-fr-name">Load the Tidewater dataset</span>
                  <!--
                    Both halves, since #440: the button loads the Tidewater work
                    *and* imports `testenv/estate.json`, so a subtitle
                    that enumerates one of them undercounts what the press does.
                    The name keeps naming the work, which is what a person
                    opening a demo build came for.
                  -->
                  <small>33 fixture items · a 23-asset estate · no network, no credential</small>
                </button>
              {/if}
              <button class="tf-fr-mod" onclick={() => (adding = true)}>
                <span class="tf-fr-name">Connect a real system</span>
                <!--
                  The roles knobas has a surface for (CONTEXT.md, *Role*), not
                  a list of adapters: it stays true when an external adapter
                  is installed, and needs no read (#1401).
                -->
                <small>A ticket system, code host, CI, wiki or monitoring</small>
              </button>
            </div>
            {#if failure}
              <p class="tf-fail tf-fr-fail">{failure}</p>
            {/if}
            <!--
              Restore on a clean machine (#1469). `restore_backup` restores only
              into a knobas that holds no data, and a source is data, so this is
              said here, before one is added, and only here: with a source
              configured the restore would be refused. The button records
              completion (a `setting` row, which the restore replaces) and lands
              where *Restore from a file…* and *Import…* list the folder, so the
              wizard has no second list of archives.
            -->
            <p class="tf-fr-note">
              Already have a backup or a share export? Restore it before adding a source: knobas restores only into a
              database that holds no data yet.
            </p>
            <div class="tf-fr-acts">
              {@render later()}
              <button class="tf-btn tf-ghost" disabled={finishing} onclick={() => void finish("backup")}>
                Restore from a backup in Settings
              </button>
            </div>
          {/if}
        {:else if stepIndex === 2}
          <p class="tf-fr-lead">
            knobas will read everything <b>{source?.display_name}</b> will give it. The first run is
            the slow one; after this it only asks for what changed.
          </p>

          {#if phase === null}
            <div class="tf-fr-acts">
              <button class="tf-btn tf-primary" disabled={starting} onclick={() => void sync()}>
                Start the first sync
              </button>
            </div>
          {:else}
            <div class="tf-fr-progress">
              <!--
                Indeterminate while it runs unless the run reports a walk with a
                total (#1379): `items` grows and nothing knows where it stops,
                and a bar that invented a total would be a lie that moves.

                The `finished` branch of the first element's ternaries is dead
                (#156): `phase` and `stepIndex` are assigned in the same channel
                callback, so step 2 never renders with `finished` true. It is
                left rather than simplified for the same reason as
                `itemsReading` above: it is the same one question about the same
                surface, and it is Björn's. The walk (#1379) is the one way this
                bar fills.
              -->
              <!--
                Two elements rather than one with an `undefined` value: a
                `<progress>` that was once given a value stays determinate when
                the value is taken away again.
              -->
              {#if finished || walkOf === undefined}
                <progress max={finished ? 1 : undefined} value={finished ? 1 : undefined}></progress>
              {:else}
                <progress max={walkOf} value={walk?.walked}></progress>
              {/if}
              <p class="tf-fr-reading">
                <b>{failed ? "failed" : phase}</b>
                · {walkReading}{itemsReading} · {(elapsed / 1000).toFixed(1)} s
              </p>
            </div>

            {#if failure}
              <!-- Text: this line came from a source system (gotcha 7). -->
              <p class="tf-fail tf-fr-fail">{failure}</p>
            {/if}

            <div class="tf-fr-acts">
              <!--
                Failure only. A run that ended `finished` is on the DONE panel by
                the time this renders (#156), so the *Finish* branch that used to
                sit here was unreachable — and an unreachable branch on this exact
                surface is what three pieces of work tripped over.
              -->
              {#if failed}
                <button class="tf-btn tf-primary" disabled={starting} onclick={() => void sync()}>Retry</button>
                <button class="tf-btn" disabled={finishing} onclick={() => void finish("tickets")}>
                  Skip for now
                </button>
              {/if}
            </div>
          {/if}
        {:else}
          <p class="tf-fr-lead">
            <!--
              The success is claimed because the run's ending said so; the count
              is only claimed when there is one (#137).
            -->
            knobas mirrored {#if mirrored === null}your items{:else}<b>{mirrored}</b> items{/if}. Press
            <kbd>⌘K</kbd> to search everything you just synced.
          </p>
          <div class="tf-fr-acts">
            <button class="tf-btn tf-primary" disabled={finishing} onclick={() => void finish("tickets")}>
              Finish
            </button>
          </div>
        {/if}
      </div>
    </div>
  </main>

  {#if adding}
    <TfAddSource
      {ports}
      {layers}
      onclose={() => (adding = false)}
      onsaved={(added) => {
        adding = false;
        source = added;
        stepIndex = 2;
        // `add_source` emits no `source:health`, and the shell's seed already
        // ran against a database that had no sources in it. Without this the
        // wizard finishes onto a shell that has never heard of the source it
        // just configured: no monogram in the status bar, and — on the
        // *Skip* path, where no sync follows to change `auth_state` — nothing
        // that would ever tell it.
        health.patch(added.health);
      }}
    />
  {/if}
</div>

<!--
  *Set up later in Settings* (#1399): the way out of the Source step. It
  records completion exactly as *Finish* does, so the next launch opens the
  shell, and lands on Settings › Sources, where adding a source lives.
-->
{#snippet later()}
  <button class="tf-btn tf-ghost" disabled={finishing} onclick={() => void finish("sources")}>
    Set up later in Settings
  </button>
{/snippet}
