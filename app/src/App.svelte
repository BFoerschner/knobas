<script lang="ts">
  import { onMount, untrack } from "svelte";

  import InboxView from "./lib/inbox/InboxView.svelte";
  import { inbox } from "./lib/inbox/inbox.svelte";
  import { Launcher } from "./lib/launcher";
  import Booting from "./lib/shell/Booting.svelte";
  import Room from "./lib/shell/Room.svelte";
  import Shell from "./lib/shell/Shell.svelte";
  import Toast from "./lib/shell/Toast.svelte";
  import { ALL_CONTEXT, switcherContexts, type RoomContext } from "./lib/shell/contexts";
  import { contexts as storedContexts } from "./lib/shell/contexts.svelte";
  import { startFollowingMerges } from "./lib/shell/follow-merges";
  import { health } from "./lib/shell/health.svelte";
  import { kindRegistry } from "./lib/shell/kind-registry.svelte";
  import { installKeys } from "./lib/shell/keys";
  import { lifecycle } from "./lib/shell/lifecycle.svelte";
  import { projects } from "./lib/shell/projects.svelte";
  import { router } from "./lib/shell/router.svelte";
  import { timer } from "./lib/shell/timer.svelte";
  import { canBeTarget } from "./lib/shell/timer";
  import TimerPicker from "./lib/shell/TimerPicker.svelte";
  import { push } from "./lib/shell/toasts.svelte";
  import SettingsView from "./lib/settings/SettingsView.svelte";
  import FirstRun from "./lib/sources/FirstRun.svelte";
  import SourcesView from "./lib/sources/SourcesView.svelte";
  import StartWork from "./lib/start-work/StartWork.svelte";
  import { ipcErrorMessage } from "./lib/ipc";
  import { linkTo } from "./lib/detail/links.svelte";

  /**
   * The rooms the switcher offers: *All work*, the stored contexts (#47), one
   * room per configured source, and one per project a corpus shows (#209).
   *
   * The sources come from the live `source:health` store rather than from a
   * second `list_sources` call — the store already knows every source id, it
   * is kept current by `source:health`, and one fact with one home is what
   * keeps the tab strip from disagreeing with the sources view about which
   * sources exist. The projects come from their own store for the same reason,
   * and from a census rather than from any room's read: a room's scan is a
   * window over the newest items, so a quiet project would silently have no
   * room.
   *
   * The source label is the source id. `display_name` lives on
   * `SourceSummary`, which this store does not carry; a tab reading `jira-eu`
   * is honest and is the word the address `#/ctx/src:jira-eu` uses. A project
   * is labelled by the source's own name for it, and by its key where the
   * source gave none knobas could read.
   */
  const contexts = $derived(
    switcherContexts(
      storedContexts.all,
      health.all.map((source) => ({ id: source.source_id, label: source.source_id })),
      projects.all,
    ),
  );

  /**
   * The room the reader last stood in, as the switcher's list last resolved
   * it (#241): the address, and the room it named on that list, or `null`
   * where it named none. `null` as a whole only until the first room view.
   *
   * It survives a non-room view on purpose (#257): the sources view is where
   * a source is removed from, and the reader who removes one comes *back* --
   * `back()` goes to the room they left. A memory cleared on the way out has
   * nothing to compare that return against, and the dead address would be
   * arrived at silently, by identity, as if opened cold.
   *
   * Plain state rather than a rune, on purpose: it is the effect below's
   * memory of its own previous run, and a rune here would make that run
   * depend on itself.
   */
  let standing: { ctx: string; room: RoomContext | null } | null = null;

  /**
   * A room that stops existing under the reader says so, and hands the
   * address to *All work* (#241).
   *
   * Only derived rooms can vanish -- a source removed or disabled, a project
   * the census stopped showing -- and both arrive here through the same list
   * rebuild, which is why the detection is this one effect over the assembled
   * list rather than a check in the room view and another in the tab strip.
   *
   * The trigger is the **transition**, not the state: the address resolved on
   * the previous room view and falls back on this one. A dead address opened
   * cold never resolved, so it keeps #209's silent fallback by identity; a
   * stored context just made, or a project appearing mid-session, resolves
   * *after* a moment of not resolving, which is the other direction and no
   * news.
   *
   * "Previous room view", not "previous run": a non-room view returns early
   * and leaves the memory as it was, so the room can also vanish while the
   * reader is elsewhere and be announced on their return (#257). The guard
   * keeps the rest silent -- a different room on return (`before.ctx !==
   * ctx`), the same room still there (`room !== null`), one that never
   * resolved (`before.room === null`), and nothing to return to (`before ===
   * null`).
   *
   * The address is replaced, not pushed: the dead one must not be one step
   * back. A detail open over the vanished room stays open -- the route keeps
   * its detail and only the room moves.
   *
   * `$effect.pre` rather than `$effect`: the rewrite then lands before the
   * frame is drawn, so the list changing and the address moving are one
   * paint rather than a fallback frame followed by a corrected one.
   *
   * The two side effects run untracked so this depends on exactly the two
   * things it reads above, the route and the list. `push` reads the toast
   * stack's length before it writes (that is what `Array.prototype.push` on
   * a `$state` proxy does), and a tracked read there makes every toast --
   * and every toast's dismissal six seconds later -- a reason to run this
   * again.
   */
  $effect.pre(() => {
    const route = router.route;
    if (route.view !== "room") return;
    const ctx = route.ctx;
    const room = contexts.find((candidate) => candidate.id === ctx) ?? null;
    const before = standing;
    standing = { ctx, room };
    if (before === null || before.ctx !== ctx || before.room === null || room !== null) return;

    const gone = before.room;
    untrack(() => {
      push({ text: `${gone.label} is no longer a room. Showing ${ALL_CONTEXT.label}.` });
      router.replace({ ...route, ctx: ALL_CONTEXT.id });
    });
  });

  /**
   * Whether the ⌘K overlay is up.
   *
   * `bind:`-ed because the launcher registers the hotkey itself, so the flag
   * moves from either side. Stream D's task 22 owns this file's shell; the
   * mount here is the two lines that integration is.
   */
  let launcherOpen = $state(false);

  /**
   * The mounted room, for Escape's rung 4 (#250).
   *
   * The maximised tile is the room's own state and stays there; the ladder in
   * `keys.ts` needs one question answered, and `bind:this` is the handle for
   * asking it. `null` whenever no room is mounted -- a non-room view, the
   * wizard -- and then there is nothing to restore, which is the honest
   * answer: rung 3 has already taken the key by then anyway.
   */
  let room = $state<ReturnType<typeof Room> | null>(null);

  /**
   * The entity the slide-over has open, as the launcher's `Tab` chain names it.
   *
   * Read off the address rather than off the detail component: the address is
   * what says an entity is open, and it is the shell's to read (spec §2). The
   * label is the id's key — the same half `Detail.svelte` puts in its header —
   * because the title is the detail's read and is not known here.
   *
   * `undefined` while no detail is open, which is what makes *Link to…* absent
   * from the chain rather than offered against nothing.
   */
  const openEntity = $derived.by(() => {
    const route = router.route;
    if (route.view !== "room" || !route.detail) return undefined;
    const id = route.detail.entityId;
    return { entityId: id, label: id.slice(id.indexOf(":") + 1) };
  });

  /**
   * **What is in front of the reader**, by the rule spec #272 states for both
   * the heartbeat and ⌘T: *the open detail, else the room's anchor, else
   * none* (#278).
   *
   * Read off the address and the resolved room, for the same reason
   * `openEntity` is: the address is what says what is open, and it is the
   * shell's to read. `null` is a legal answer and not a missing one — it is
   * what makes ⌘T open the picker rather than start on nothing, and what
   * passive attribution (#281) will record as an unattributed gap.
   *
   * Everything is put through `canBeTarget` before it leaves here. A promoted
   * context's anchor is a ticket or an epic, so it always passes today; the
   * guard is what stops a room shape that changes later quietly making a
   * context the thing the clock runs on.
   */
  const foreground = $derived.by(() => {
    const route = router.route;
    if (route.view !== "room") return null;
    const open = route.detail?.entityId;
    if (open && canBeTarget({ entityId: open })) {
      return { kind: "entity", entity_id: open } as const;
    }
    const anchor = contexts.find((candidate) => candidate.id === route.ctx)?.anchorId;
    if (anchor && canBeTarget({ entityId: anchor })) {
      return { kind: "entity", entity_id: anchor } as const;
    }
    return null;
  });

  // The store holds the answer rather than a function that computes it: the
  // heartbeat fires from an interval, outside any reactive scope, and a
  // callback closing over runes read there would be read untracked.
  $effect(() => {
    timer.foreground = foreground;
  });

  /** Whether ⌘T's picker is up (#278, story 9). */
  let pickerOpen = $state(false);

  /**
   * Say why a timer command refused, in the backend's own words.
   *
   * One helper for the three of them: `start_timer` refuses a stored context,
   * a blank label and a second timer, and each of those is a sentence the
   * reader can act on — so there is nothing per-caller to add, and three
   * copies of the same arrow would be three chances for one of them to start
   * swallowing.
   */
  function complain(error: unknown) {
    push({ text: ipcErrorMessage(error), tone: "err" });
  }

  /**
   * ⌘T, and the strip's timer slot: one verb with three outcomes, all three
   * decided by `timer.press()` — see its documentation for why the rule is
   * there and not in `keys.ts`.
   *
   * The one thing decided here is `"pick"`, because opening a dialog is the
   * shell's. A refusal is a toast: `start_timer` rejects a stored context, a
   * blank label and a second timer, and every one of those is a sentence the
   * reader can act on.
   */
  function toggleTimer() {
    void timer
      .press()
      .then((outcome) => {
        if (outcome === "pick") pickerOpen = true;
      })
      .catch(complain);
  }

  /**
   * The launcher's *Start timer* row: **stop what is running, then start**
   * (#278, story 11).
   *
   * The **order** is `timer.switchTo`'s and not this function's — the store
   * owns it, with its own witness — because it is a rule about the clock
   * rather than a step in the shell's glue. What is here is the sentence the
   * reader sees. #280 turns the closed block it answers with into a worklog
   * draft.
   *
   * The launcher stays open behind this deliberately — it closes on its own
   * `Enter`, and a chain action is not a navigation.
   */
  function startTimerOn(entityId: string, title: string) {
    void timer
      .switchTo({ kind: "entity", entity_id: entityId })
      .then(() => {
        push({ text: `Timing ${title}.` });
      })
      .catch(complain);
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
    /**
     * Unmounted before the bridge was ready.
     *
     * Everything below the dynamic import runs on a later tick, and the window
     * can be gone by then — in a test that mounts and unmounts, and in a real
     * session that closes during bring-up. Without this the teardown runs
     * *first* and the subscriptions are installed *after* it, which is a leak
     * per mount rather than a subscription per window: `lifecycle.start()`
     * opens with `stopped = false`, so a `stop()` that has already happened is
     * simply undone.
     */
    let disposed = false;
    let stopHealth: (() => void) | undefined;
    let stopMerges: (() => void) | undefined;
    let stopInbox: (() => void) | undefined;
    let stopContexts: (() => void) | undefined;
    let stopProjects: (() => void) | undefined;
    let stopTimer: (() => void) | undefined;

    void (async () => {
      // Dev only, and behind `import.meta.env.DEV` so Rollup folds the branch
      // away and the fixture never reaches a bundle. `?fake-ipc` opts in;
      // without it even a dev build talks to the real backend.
      //
      // Awaited *before* anything subscribes, and that ordering is
      // load-bearing under `?fake-ipc`: a dynamic import resolves on a later
      // tick, so a `listen` issued first would run against a
      // `window.__TAURI_INTERNALS__` the fixture has not defined yet. A real
      // Tauri window injects its internals before any script runs, which is
      // why this only ever breaks in browser QA — the worst place for a
      // difference to hide.
      if (import.meta.env.DEV) {
        const dev = await import("./lib/shell/dev/fake-tauri");
        dev.installIfRequested();
      }
      if (disposed) return;

      // Subscribed for the whole session, not per component: the top strip,
      // the sources view and the launcher all draw this, and three independent
      // fetches is three chances for them to disagree (#27). The *seed* is the
      // effect below — `credential_health` cannot answer until the database is
      // up. Behind the same await as the lifecycle, for the same reason: it is
      // a `listen`, and a `listen` before the fixture is a listen into nothing.
      stopHealth = health.start();
      // The reverse direction's trigger (#44): every sync ending runs the
      // follow-merges pass, and a moved ticket is announced. Session-wide for
      // the same reason the health store is -- a merged pull request does not
      // care which view is open.
      stopMerges = startFollowingMerges();
      // The inbox moves when the mirror moves and when the reader answers
      // something, so it subscribes to the two events that already say so
      // rather than getting a channel of its own. Subscribing here, seeding
      // below: `inbox_items` goes through the sync engine's state and answers
      // `not_ready` for the whole of bring-up.
      stopInbox = inbox.start();
      // The stored contexts (#47): same split as health — subscribe now,
      // seed once the database can answer.
      stopContexts = storedContexts.start();
      // The projects a corpus shows (#209): the same split again. Its event is
      // a sync run ending, because a project room appears when the first item
      // carrying it syncs.
      stopProjects = projects.start();
      // The clock, the heartbeat and the activity subscription (#278). Behind
      // the same await as the four above, and for the same reason: `begin()`
      // issues a `listen` *synchronously*, and a `listen` before the fixture
      // is a listen into nothing. The tick and the beat are intervals and do
      // not care, but the subscription is how the strip learns about a start
      // or a stop this window did not make, and losing it under `?fake-ipc`
      // would be invisible in browser QA -- the shell would simply stop
      // noticing.
      //
      // Nothing is waiting on the tick: the first `refresh` comes from the
      // `lifecycle.ready` effect below, which fires long after this.
      stopTimer = timer.begin();
      // Once, at shell start: `list_adapters` is static per build and answers
      // before the database is up, so there is nothing to poll and nothing to
      // tear down.
      void kindRegistry.load();

      await lifecycle.start();
    })();

    const stopRouter = router.start();
    const stopKeys = installKeys(router, {
      openLauncher,
      restoreTile: () => room?.restoreTile() ?? false,
      toggleTimer,
    });

    return () => {
      disposed = true;
      stopTimer?.();
      stopHealth?.();
      stopMerges?.();
      stopInbox?.();
      stopContexts?.();
      stopProjects?.();
      stopKeys();
      stopRouter();
      lifecycle.stop();
    };
  });

  /**
   * Seed credential health the moment the database can answer, and never
   * before.
   *
   * `credential_health` goes through `crate::sources::state()`, which rejects
   * with `not_ready` for the whole of bring-up. Seeding at mount therefore
   * threw its one reading away, and because the scheduler emits `source:health`
   * *on a change only*, a steady install where every source is `ok` got no
   * second chance: the strip's monograms, the per-source room tabs and the
   * launcher's row complaints stayed empty for the session. README's rule is
   * the one that was broken — "no data call before `db.state === 'ready'`; the
   * shell gates on the lifecycle rather than catching and ignoring".
   *
   * An effect rather than a one-shot because `retry()` can take the window
   * from `failed` back to `ready`, and that is a database the store has still
   * never read.
   */
  $effect(() => {
    if (lifecycle.ready) void health.reseed();
  });

  /**
   * Seed the inbox the moment the database can answer, and never before.
   *
   * The same rule and the same shape as the health seed above: `inbox_items`
   * reaches the sync engine's state for the adapter descriptors, which is
   * managed only once the database is up, so a read at mount would throw its
   * one answer away — and the strip's count would stay at zero for the session
   * unless a sync happened to finish.
   */
  $effect(() => {
    if (lifecycle.ready) void inbox.refresh();
  });

  /**
   * Seed the stored contexts the moment the database can answer — the same
   * rule and the same shape as the two seeds above: `list_contexts` rejects
   * with `not_ready` for the whole of bring-up, and `contexts:changed` only
   * fires on a mutation, so a fresh session would otherwise show no stored
   * rooms until the first one is made.
   */
  $effect(() => {
    if (lifecycle.ready) void storedContexts.reseed();
  });

  /**
   * Seed the project rooms the moment the database can answer — the same rule
   * and the same shape as the three seeds above. `list_projects` rejects with
   * `not_ready` for the whole of bring-up, and its event only fires when a
   * sync run ends, so without this a session that syncs nothing new would show
   * no project rooms for whatever is already mirrored.
   */
  $effect(() => {
    if (lifecycle.ready) void projects.reseed();
  });

  /**
   * Seed the timer the moment the database can answer — the same rule and the
   * same shape as the four seeds above. `current_timer` rejects with
   * `not_ready` for the whole of bring-up, and the activity signal only fires
   * on a mutation, so without this a session that starts with a timer already
   * running would show nothing in the strip until the next thing happened.
   *
   * This is also the read the relaunch sweep runs *before*: by the time the
   * database says `ready`, a timer that outlived the last process has already
   * been closed, so what comes back here is never a clock that ran all night.
   */
  $effect(() => {
    if (lifecycle.ready) void timer.refresh();
  });

  /**
   * The top strip's search field, and `installKeys`' own ⌘K binding.
   *
   * The launcher binds ⌘K too — that is what makes mounting it the whole
   * integration — and the two converge: this sets the flag, the component
   * toggles it, and both land on "open" from a closed box.
   */
  function openLauncher() {
    launcherOpen = true;
  }

  /**
   * §14a's landing: *"land in the launcher"*.
   *
   * The room first, then the overlay — a launcher over a blank pane is the
   * same empty window it was before, and the sentence's point is that the
   * reader can immediately search what they just synced.
   */
  function onFirstRunDone() {
    completedFirstRun = true;
    // The wizard has changed the set of sources — it added one, or `demo_load`
    // registered `mock` — and neither `add_source` nor `demo_load` emits
    // `source:health`. The seed below ran against a database that had no
    // sources in it at all, so without this the shell lands on a room whose
    // strip, tabs and launcher complaints have never heard of what was just
    // configured. `Skip for now` is the path where nothing else would ever
    // tell it.
    void health.reseed();
    // ...and `demo_load` has just written a corpus with projects in it (#207),
    // which the seed above ran too early to see. Since #240 the demo load
    // ends with the `sync:state` its run owes, and the projects store
    // re-lists on that, so this is no longer the only thing standing between
    // a demo load and the project rooms -- the wizard's route form can be
    // left by a room tab, the launcher or an address bar without ever coming
    // through here. Kept because the *Finish* path needs no event to be
    // right, and because the health half above still has no event behind it.
    void projects.reseed();
    router.go("#/ctx/all");
    launcherOpen = true;
  }

  /**
   * Whether *this session* has finished the wizard.
   *
   * `AppStatus.first_run` is the durable answer, written by
   * `complete_first_run` and read on the next launch. It is **not** re-read
   * here: `lifecycle` stops polling once the database is ready, so within one
   * session `status.first_run` keeps whatever it said at boot. This flag is
   * that session's answer, and without it every route change after the wizard
   * would put the reader straight back into it.
   */
  let completedFirstRun = $state(false);

  /**
   * The §14a wizard takes over the whole window when there is nothing to show.
   *
   * Not a route the reader has to find: a first run has no rooms, no sources
   * and no corpus, so a shell drawn around an empty room with a wizard
   * somewhere behind `#/first-run` is a window that says nothing about what to
   * do next. `#/first-run` still addresses it, so a person can walk back
   * through it on purpose.
   */
  const firstRun = $derived(
    !completedFirstRun && (lifecycle.status?.first_run ?? false),
  );

  async function onRetry() {
    try {
      await lifecycle.retry();
    } catch (error) {
      push({ text: `Could not restart the database: ${ipcErrorMessage(error)}`, tone: "err" });
    }
  }
</script>

{#if lifecycle.ready && firstRun}
  <FirstRun demo={lifecycle.status?.demo ?? false} onfinish={onFirstRunDone} />
{:else if lifecycle.ready}
  <Shell {router} {lifecycle} {contexts} onsearch={openLauncher} {timer} ontimer={toggleTimer}>
    {#snippet main()}
      {#if router.route.view === "unknown"}
        <!--
          A parsed address for a surface a later milestone owns. Saying which
          one beats a blank pane, and beats pretending the word was an entity
          kind and 404ing on it.
        -->
        <div class="empty">
          <p><span class="mono">{router.route.hash}</span> arrives in a later milestone.</p>
          <button class="btn" onclick={() => router.back()}>Back to the room</button>
        </div>
      {:else if router.route.view === "inbox"}
        <InboxView {router} />
      {:else if router.route.view === "sources"}
        <SourcesView />
      {:else if router.route.view === "settings"}
        <SettingsView />
      {:else if router.route.view === "first-run"}
        <FirstRun demo={lifecycle.status?.demo ?? false} onfinish={onFirstRunDone} />
      {:else if router.route.view === "start-work"}
        <!--
          Keyed on the ticket. A flow's state *is* its address, and this branch
          stays selected when the address moves from one ticket to another --
          so without the key Svelte would keep the mounted component and hand it
          new props, leaving one ticket's steps on screen under another's
          heading.
        -->
        {#key router.route.key}
          <StartWork
            entityId={router.route.key}
            onnavigate={(hash) => router.go(hash)}
            onclose={() => router.back()}
          />
        {/key}
      {:else}
        <Room bind:this={room} {router} {contexts} />
      {/if}
    {/snippet}
  </Shell>
  <!--
    `sources` is supplied, and supplying it is the point: the launcher's rows
    and chips then read the live `source:health` store instead of the copy the
    board fetches once per opening. An empty list here means *watching, nothing
    to complain about* — which is why the prop's contract distinguishes it from
    `undefined` (#36).
  -->
  <Launcher
    bind:open={launcherOpen}
    sources={health.all}
    {openEntity}
    onlink={(targetId, targetTitle) => {
      // Guarded because the prop outlives one keystroke: the chain is only
      // built while something is open, and this is where that stops being an
      // assumption.
      if (openEntity) void linkTo(openEntity.entityId, targetId, targetTitle);
    }}
    ontimer={startTimerOn}
    onnavigate={(hash) => router.go(hash)}
    onclose={() => {}}
  />
  {#if pickerOpen}
    <TimerPicker onpick={startFromPicker} onclose={() => (pickerOpen = false)} />
  {/if}
{:else}
  <Booting
    db={lifecycle.db}
    version={lifecycle.status?.app_version ?? ""}
    demo={lifecycle.status?.demo ?? false}
    onretry={() => void onRetry()}
  />
{/if}
<Toast />
