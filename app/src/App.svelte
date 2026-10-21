<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { listen } from "@tauri-apps/api/event";

  import { alerts } from "./lib/assets/alerts.svelte";
  import { inbox } from "./lib/inbox/inbox.svelte";
  import { notifications } from "./lib/inbox/notify.svelte";
  import { Launcher } from "./lib/launcher";
  import Booting from "./lib/shell/Booting.svelte";
  import { startFollowingMerges } from "./lib/shell/follow-merges";
  import { health } from "./lib/shell/health.svelte";
  import { kindRegistry } from "./lib/shell/kind-registry.svelte";
  import { lifecycle } from "./lib/shell/lifecycle.svelte";
  import { timer } from "./lib/shell/timer.svelte";
  import { targetReading } from "./lib/shell/timer";
  import { openWriteQueue } from "./lib/shell/write-queue.svelte";
  import TimerPicker from "./lib/shell/TimerPicker.svelte";
  import { push } from "./lib/shell/toasts.svelte";
  import { followWrite } from "./lib/shell/follow-write";
  import AdHocBlockDialog from "./lib/time/AdHocBlock.svelte";
  import FirstRun from "./lib/sources/FirstRun.svelte";
  import { EVENTS, ipcErrorMessage } from "./lib/ipc";
  import { adHocBlock, worklogDraft, type AdHocBlock, type Block, type Worklog } from "./lib/ipc/time";
  import { recordCaptureContext } from "./lib/ipc/entity";
  import { entityAddress } from "./lib/tf/address";
  import { HOME, homeIfBlank } from "./lib/tf/home";
  import { currentJob } from "./lib/tf/job/current.svelte";
  import { foregroundOf } from "./lib/tf/foreground";
  import { parseTfHash } from "./lib/tf/route";
  import TfShell from "./lib/tf/TfShell.svelte";
  import TfWorklogDraft from "./lib/tf/TfWorklogDraft.svelte";
  import { createDraftFlow } from "./lib/tf/drafting.svelte";
  import { offsetMinutes } from "./lib/time/draft";
  import { hoursAndMinutes } from "./lib/time/draft-form.svelte";

  /**
   * The window draws the ticket-first shell, and nothing else (#881): the
   * rooms UI is gone. The shell owns its keyboard and its address; this file
   * keeps what is the whole app's: the bring-up, the session-wide stores, the
   * launcher, the timer verb and the dialogs it opens.
   *
   * A window opened on no address lands on Tickets, at `#/tickets`.
   */
  homeIfBlank(location);

  /** Go to an address the launcher or the capture window names. */
  function navigate(hash: string) {
    location.hash = hash;
  }

  /** Whether the ⌘K overlay is up; the launcher binds ⌘K itself. */
  let launcherOpen = $state(false);

  /** The address, read again on every change; the shell owns what it draws. */
  let hash = $state(location.hash);

  /**
   * What is in front of the reader (#1102): the entity page on screen, else
   * the asset the Tree selects, else the job on screen (R-1.4), else nothing,
   * by `foregroundOf`'s reading of `CONTEXT.md`'s rule. A page with none of
   * them leaves ⌘T asking. The store holds the answer because the heartbeat
   * reads it from an interval, outside any reactive scope.
   */
  const foregroundId = $derived(foregroundOf(parseTfHash(hash), currentJob.entityId)?.entity_id ?? null);
  // Keyed on the id, so a tab of the same page (`#/pr/<id>/files`) changes nothing downstream.
  const foreground = $derived(foregroundId === null ? null : ({ kind: "entity", entity_id: foregroundId } as const));

  $effect(() => {
    timer.foreground = foreground;
  });

  /**
   * What a capture will attach (#503): the foreground, pushed to the backend
   * so the capture window can read it. There is no stored context to stand in
   * any more, so that half is `null`. A rejected record is swallowed: it can
   * only fail when there is no bridge at all.
   */
  $effect(() => {
    void recordCaptureContext(null, foreground?.entity_id ?? null).catch(() => {});
  });

  /** Whether ⌘T's picker is up (#278). */
  let pickerOpen = $state(false);

  /** The ad-hoc block dialog a stop opened, with its block, or `null` (#281). */
  let adHoc = $state<{ block: Block; offer: AdHocBlock } | null>(null);

  /** Bumped whenever a draft or a dialog changes a day's blocks (#283). */
  let timeRevision = $state(0);

  /** Say why a command refused, in the backend's own words. */
  function complain(error: unknown) {
    push({ text: ipcErrorMessage(error), tone: "err" });
  }

  /**
   * What a stop opens, and what the draft's answers do to the clock (#826,
   * R-13.1 to R-13.3): *Keep running*, *Stop, log later*, and a switch that
   * waits for the running ticket's draft.
   */
  const drafts = createDraftFlow({
    timer,
    // Wrapped, so the bridge is read when a stop happens rather than at mount.
    adHocBlock: (id, when) => adHocBlock(id, when),
    worklogDraft: (entityId, when) => worklogDraft(entityId, when),
    onadhoc: (block, offer) => (adHoc = { block, offer }),
    push,
    complain,
    onday: () => (location.hash = "#/time"),
  });

  /**
   * The Day tab's *Log…* on an unlogged block (#827): the draft for that
   * ticket and day. `null` is "nothing left to log there", which is told.
   */
  function draftFor(entityId: string, day: string) {
    void worklogDraft(entityId, { day, offsetMinutes: offsetMinutes(new Date(`${day}T12:00:00`)) })
      .then((draft) => {
        if (draft === null) push({ text: "Nothing to log on that ticket that day." });
        else drafts.show(draft);
      })
      .catch(complain);
  }

  /**
   * A worklog now exists: one toast in the button's words (R-1.8), and one
   * followed write. Queued, not sent (#775).
   */
  function onWorklogged(logged: Worklog) {
    timeRevision += 1;
    const key = targetReading({ kind: "entity", entity_id: logged.entity_id });
    push({
      text: `Queued: log ${hoursAndMinutes(Math.round(logged.seconds / 60))} to ${key}.`,
      action: { label: "Show queue", run: openWriteQueue },
    });
    if (logged.write_queue_id !== null) followWrite({ id: logged.write_queue_id }, { name: "Worklog" });
  }

  /**
   * ⌘T and the timer chip. One draft at a time (#826); a running clock opens
   * its draft and stops on the answer (#896); otherwise `timer.press()`
   * starts on the foreground or asks with the picker.
   */
  function toggleTimer() {
    if (drafts.busy) return drafts.refuse();
    if (timer.current) return drafts.stop();
    void timer
      .press()
      .then((pressed) => {
        if (pressed.did === "pick") pickerOpen = true;
      })
      .catch(complain);
  }

  /** *Start timer* on a ticket: the running ticket's draft first (R-13.2). */
  function startTimerOn(entityId: string, title: string) {
    void drafts.switchTo({ kind: "entity", entity_id: entityId }, title);
  }

  /** Start on what the picker chose, and close it only if that worked. */
  function startFromPicker(target: Parameters<typeof timer.start>[0]) {
    void timer
      .start(target)
      .then(() => {
        pickerOpen = false;
      })
      .catch(complain);
  }

  onMount(() => {
    // Unmounted before the bridge was ready: everything after the dynamic
    // import runs a tick later, and must not subscribe into a dead window.
    let disposed = false;
    const stops: (() => void)[] = [];

    void (async () => {
      // Dev only, folded away in a bundle. Awaited before anything subscribes:
      // a `listen` issued first would run against a bridge `?fake-ipc` has not
      // installed yet.
      if (import.meta.env.DEV) {
        const dev = await import("./lib/shell/dev/fake-tauri");
        dev.installIfRequested();
      }
      if (disposed) return;

      // Subscribed for the whole session; each is seeded below once the
      // database can answer.
      stops.push(health.start());
      stops.push(startFollowingMerges());
      stops.push(inbox.start());
      stops.push(alerts.start());
      stops.push(timer.begin());
      stops.push(notifications.start());
      // The capture window's *Open in knobas* (#503). Not awaited: an awaited
      // listen here would push the lifecycle's own subscription past a
      // teardown (measured 2026-09-08, `residue`). A note has no view of its
      // own while notes hide (D10), so it opens Settings › Notes and capture
      // (#1085); the payload is the note's id.
      void listen<string>(EVENTS.captureOpenNote, (event) => {
        navigate(entityAddress("note", event.payload));
      }).then((off) => {
        if (disposed) off();
        else stops.push(off);
      });
      // Static per build, and answers before the database is up.
      void kindRegistry.load();

      await lifecycle.start();
    })();

    return () => {
      disposed = true;
      for (const stop of stops) stop();
      lifecycle.stop();
    };
  });

  // Seed each session store the moment the database can answer, and never
  // before: every one of these reads rejects with `not_ready` during bring-up,
  // and their events fire on a change only. One effect each, so what one seed
  // happens to read never re-runs another.
  $effect(() => {
    if (lifecycle.ready) void health.reseed();
  });
  $effect(() => {
    if (lifecycle.ready) void inbox.refresh();
  });
  $effect(() => {
    if (lifecycle.ready) void alerts.refresh();
  });
  $effect(() => {
    if (lifecycle.ready) void timer.refresh();
  });
  $effect(() => {
    if (lifecycle.ready) void notifications.reseed();
  });

  /**
   * Hand the inbox's stream to the notifier (#290), once the inbox has
   * answered: primed against the empty stream it starts with, the notifier
   * would take the whole backlog for news.
   */
  $effect(() => {
    const stream = inbox.stream;
    const answered = inbox.answered;
    untrack(() => {
      if (answered) notifications.saw(stream);
    });
  });

  function openLauncher() {
    launcherOpen = true;
  }

  /**
   * Whether this session has finished the §14a wizard. `first_run` is read
   * once at boot, so without this every change after the wizard would put the
   * reader back into it.
   */
  let completedFirstRun = $state(false);
  const firstRun = $derived(!completedFirstRun && (lifecycle.status?.first_run ?? false));

  /**
   * §14a's landing: Tickets, with the launcher open over it. A reader who set
   * up later (#1399) lands on Settings › Sources instead, where adding a
   * source lives, with nothing over it.
   */
  function onFirstRunDone(landing: "tickets" | "sources") {
    completedFirstRun = true;
    // The wizard added a source, and neither `add_source` nor `demo_load`
    // emits `source:health`.
    void health.reseed();
    if (landing === "sources") {
      location.hash = "#/settings/sources";
      return;
    }
    location.hash = HOME;
    launcherOpen = true;
  }

  async function onRetry() {
    try {
      await lifecycle.retry();
    } catch (error) {
      push({ text: `Could not restart the database: ${ipcErrorMessage(error)}`, tone: "err" });
    }
  }
</script>

<svelte:window onhashchange={() => (hash = location.hash)} />

{#if lifecycle.ready && firstRun}
  <FirstRun demo={lifecycle.status?.demo ?? false} onfinish={onFirstRunDone} />
{:else if lifecycle.ready}
  <TfShell
    {lifecycle}
    onsearch={openLauncher}
    ontimer={toggleTimer}
    onstarttimer={startTimerOn}
    onlog={draftFor}
    {timeRevision}
    ontimechanged={() => (timeRevision += 1)}
  >
    {#snippet dialogs()}
      {#if drafts.open}
        {#key drafts.open}
          <TfWorklogDraft
            draft={drafts.open.draft}
            stopped={drafts.open.running !== null}
            onbeforelog={() => drafts.stopClock()}
            onoutcome={(outcome) => drafts.settle(outcome)}
            onlogged={onWorklogged}
          />
        {/key}
      {/if}
    {/snippet}
  </TfShell>
  <!--
    `sources` is supplied so the launcher's rows and chips read the live
    `source:health` store; an empty list means *watching, nothing to complain
    about* (#36). There is no stored context to add to any more.
  -->
  <Launcher
    bind:open={launcherOpen}
    sources={health.all}
    sourcesRead={{ phase: health.phase, error: health.error, retry: () => void health.reseed() }}
    context={undefined}
    oncontext={undefined}
    ontimer={startTimerOn}
    onnavigate={navigate}
    onclose={() => {}}
  />
  {#if pickerOpen}
    <TimerPicker onpick={startFromPicker} onclose={() => (pickerOpen = false)} />
  {/if}
  {#if adHoc}
    <!-- *Log to a ticket…* re-targets the block, which moves a row of the week. -->
    <AdHocBlockDialog
      block={adHoc.block}
      offer={adHoc.offer}
      onclose={() => (adHoc = null)}
      onlog={(draft) => {
        timeRevision += 1;
        drafts.show(draft);
      }}
    />
  {/if}
{:else}
  <Booting
    db={lifecycle.db}
    version={lifecycle.status?.app_version ?? ""}
    demo={lifecycle.status?.demo ?? false}
    onretry={() => void onRetry()}
  />
{/if}
