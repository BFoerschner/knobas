<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { listen } from "@tauri-apps/api/event";

  import AssetsView from "./lib/assets/AssetsView.svelte";
  import MonitorsView from "./lib/assets/MonitorsView.svelte";
  import InboxView from "./lib/inbox/InboxView.svelte";
  import { alerts } from "./lib/assets/alerts.svelte";
  import { inbox } from "./lib/inbox/inbox.svelte";
  import { notifications } from "./lib/inbox/notify.svelte";
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
  import { sourceKinds } from "./lib/shell/source-kinds.svelte";
  import { router } from "./lib/shell/router.svelte";
  import { timer } from "./lib/shell/timer.svelte";
  import { canBeTarget, roomForeground } from "./lib/shell/timer";
  import TimerPicker from "./lib/shell/TimerPicker.svelte";
  import { push } from "./lib/shell/toasts.svelte";
  import { localDay, offsetMinutes } from "./lib/time/draft";
  import AdHocBlockDialog from "./lib/time/AdHocBlock.svelte";
  import WorklogDraft from "./lib/time/WorklogDraft.svelte";
  import SettingsView from "./lib/settings/SettingsView.svelte";
  import FirstRun from "./lib/sources/FirstRun.svelte";
  import SourcesView from "./lib/sources/SourcesView.svelte";
  import StartWork from "./lib/start-work/StartWork.svelte";
  import StandupView from "./lib/standup/StandupView.svelte";
  import DayReview from "./lib/time/DayReview.svelte";
  import WeekTimesheet from "./lib/time/WeekTimesheet.svelte";
  import { ipcErrorMessage } from "./lib/ipc";
  import {
    adHocBlock,
    worklogDraft,
    type AdHocBlock,
    type Block,
    type Draft,
  } from "./lib/ipc/time";
  import { addToContext, linkTo } from "./lib/detail/links.svelte";
  import { EVENTS } from "./lib/ipc";
  import { recordCaptureContext } from "./lib/ipc/entity";

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
      health.all.map((source) => ({
        id: source.source_id,
        label: source.source_id,
        adapterKind: sourceKinds.of(source.source_id),
      })),
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
   * **The room the reader is standing in**, resolved, or `null` when they are
   * not in one.
   *
   * One lookup for the two questions below, which read different fields of it:
   * the foreground rule wants its *anchor* (what the clock runs on when nothing
   * is open) and the timer's room wants its *stored context* (where the reader
   * was standing). Two `find`s over the same list would be two chances for one
   * of them to go on reading a room the address has left.
   */
  const standingIn = $derived.by(() => {
    const route = router.route;
    if (route.view !== "room") return null;
    return contexts.find((candidate) => candidate.id === route.ctx) ?? null;
  });

  /**
   * **What is in front of the reader**, by the rule spec #272 states for both
   * the heartbeat and ⌘T: *the open detail, else the Assets pane's asset, else
   * the room's anchor, else none* (#278, and the middle rung #437).
   *
   * Read off the address and the resolved room, for the same reason
   * `openEntity` is: the address is what says what is open, and it is the
   * shell's to read. `null` is a legal answer and not a missing one — it is
   * what makes ⌘T open the picker rather than start on nothing, and what
   * passive attribution (#282) records as an unattributed gap.
   *
   * **The Assets view is not a room, so its rung is its own branch.** The
   * pane's asset is `#/asset/<id>`'s id and nothing else: `#/assets/tree` is
   * the same surface with nothing selected, and the view has no anchor to fall
   * back to — story 47 asks for *the asset open in the pane*, and a browse of
   * the estate with an empty pane is honestly nothing in front of the reader.
   * That is also why this cannot be folded into the room branch: the two views
   * answer the question from different halves of the address.
   *
   * Everything is put through `canBeTarget` before it leaves here. A promoted
   * context's anchor is a ticket or an epic, so it always passes today; the
   * guard is what stops a room shape that changes later quietly making a
   * context the thing the clock runs on.
   *
   * The **room** ladder itself is `timer.ts`'s `roomForeground` since #502,
   * which is the other reader of it: a note born from *New note* carries a
   * `captured-from` link to the foreground, and it has to be the same answer
   * the heartbeat would send at that instant or knobas' two records of what
   * the reader was on disagree about one minute.
   */
  const foreground = $derived.by(() => {
    const route = router.route;
    if (route.view === "assets") {
      const held = route.assetId;
      return held && canBeTarget({ entityId: held })
        ? ({ kind: "entity", entity_id: held } as const)
        : null;
    }
    if (route.view !== "room") return null;
    const front = roomForeground(route.detail?.entityId, standingIn?.anchorId);
    return front === null ? null : ({ kind: "entity", entity_id: front } as const);
  });

  // The store holds the answer rather than a function that computes it: the
  // heartbeat fires from an interval, outside any reactive scope, and a
  // callback closing over runes read there would be read untracked.
  $effect(() => {
    timer.foreground = foreground;
  });

  /**
   * **The stored context the reader is standing in**, which every start
   * records on the block it opens (#281).
   *
   * `filter.context` and not the room's own id: *All work*, a source room and
   * a project room are derived rooms with no `ctx:` row behind them, and each
   * of them carries `null` there (`contexts.ts`). A room id passed straight
   * through would send `src:jira` as a context, which the backend refuses.
   *
   * Not the anchor, which is what `foreground` reads: the anchor is *what the
   * clock runs on* when nothing is open, and this is *where the reader was
   * standing*. The same room supplies both, and they answer different
   * questions.
   */
  const roomContext = $derived(standingIn?.filter.context ?? null);

  /**
   * **The room the launcher's *Add to context* row acts on**, or `undefined`.
   *
   * One value for the row's label and for the write, because they name the
   * same room and two derivations of it are two chances to disagree. `null`
   * `roomContext` is every *derived* room — *All work*, a source, a project —
   * which has no `ctx:` entity to link to, and that is the row's absence
   * rather than a write with one end.
   */
  const launcherContext = $derived(
    roomContext === null
      ? undefined
      : { ctxId: roomContext, label: standingIn?.label ?? "this context" },
  );

  $effect(() => {
    timer.roomContext = roomContext;
  });

  /**
   * **What a capture will attach** (#503) — the same two values the timer store
   * is given just above, pushed to the backend so the capture window can read
   * them.
   *
   * The capture window is a webview of its own with no shell in it: it cannot
   * work out which room the reader was standing in or what was in front of
   * them, so this is how it is told. What is sent is `roomContext` and
   * `foreground` **unchanged** — not a third derivation of either — because the
   * property the deputy's ruling of 2026-09-08 on #502 binds is that what a
   * capture attaches equals what the heartbeat would send at that instant. Two
   * spellings of the ladder would make that a coincidence; there is one, and it
   * is `timer.ts`'s `roomForeground`, read once above.
   *
   * A rejected record is swallowed, and the reason is that there is nothing to
   * say: `record_capture_context` takes no pool and reads nothing, so it
   * cannot answer `not_ready` and has no failure of its own — what is left is
   * the bridge not being there at all, which is browser QA under `?fake-ipc`
   * before the fixture is installed. The effect runs again on the next change,
   * and a toast about a note nobody is writing yet would be the shell shouting
   * about its own plumbing.
   */
  $effect(() => {
    void recordCaptureContext(roomContext, foreground?.entity_id ?? null).catch(() => {});
  });

  /** Whether ⌘T's picker is up (#278, story 9). */
  let pickerOpen = $state(false);

  /** The worklog draft a stop opened, or `null` (#280). */
  let worklog = $state<Draft | null>(null);

  /**
   * The ad-hoc block dialog a stop opened, with the block it is about, or
   * `null` (#281).
   *
   * The block travels beside the offer because the dialog draws it — how long
   * it was, what it was on — and re-targets it by id. Reading it back off the
   * day would be a second read of a row the stop already handed over.
   */
  let adHoc = $state<{ block: Block; offer: AdHocBlock } | null>(null);

  /**
   * Bumped whenever the time view writes anything (#283).
   *
   * The day strip and the week timesheet share one address and one screen, and
   * an edit on either changes what the other draws — assigning a block moves
   * the week's unlogged total, and *Log all* makes the strip's blocks
   * read-only. Neither owns the other, and the time commands deliberately
   * write no activity line, so the shell holds one counter between them: each
   * view calls `onchanged` after a successful write and re-reads when the
   * counter moves. Spec #272 story 44 is the reason: "assigning a block and
   * watching the week's unlogged total change is one glance".
   */
  let timeRevision = $state(0);

  /**
   * **Every stop on an entity asks for a draft, and `null` is the ordinary
   * answer** (#280).
   *
   * Whether a worklog can go anywhere is the backend's decision, read off the
   * source's declared write ops — so the shell asks and opens the draft when
   * it gets one. A list of kinds here would be the hardcoded per-adapter table
   * §3a exists to prevent, and would go stale the day an adapter starts taking
   * worklogs.
   *
   * The **day is the one the block started on**, which is the rule the backend
   * files a block under (`UNLOGGED_BLOCKS` narrows on `started_at`). A stop at
   * 00:10 closes an afternoon that belongs to yesterday, and asking for today
   * would answer `null` — the reader would get nothing, with no way to tell
   * why.
   *
   * A failed read is a toast, not silence: the reader pressed stop expecting a
   * draft, and the blocks are still there to log by hand from the day review.
   */
  function draftWorklog(closed: Block | null) {
    if (!closed || closed.target.kind !== "entity") return;
    const on = closed.target.entity_id;
    void worklogDraft(on, {
      day: localDay(new Date(closed.started_at)),
      offsetMinutes: offsetMinutes(),
    })
      .then((draft) => {
        worklog = draft;
      })
      .catch(complain);
  }

  /**
   * **What a stop opens**, decided by the backend and not by a list of kinds
   * here (#281).
   *
   * `ad_hoc_block` is asked first, and its `null` is the answer that says *the
   * block is on a ticket* — so the worklog draft is what opens. Anything else
   * is a page, a note, a repo or a label, and the ad-hoc dialog opens on it,
   * with or without a suggestion inside.
   *
   * The shell could not make this decision itself without keeping a table of
   * which kinds take worklogs, which is the per-adapter table §3a exists to
   * prevent: the day an adapter starts taking them, the table is wrong and
   * nothing says so.
   *
   * A failed read is a toast and nothing else: the block exists either way,
   * and the day review is where it can still be given a ticket by hand.
   */
  function offerOn(closed: Block | null) {
    if (!closed) return;
    const day = { day: localDay(new Date(closed.started_at)), offsetMinutes: offsetMinutes() };
    void adHocBlock(closed.id, day)
      .then((offer) => {
        if (offer) {
          adHoc = { block: closed, offer };
          return;
        }
        draftWorklog(closed);
      })
      .catch(complain);
  }

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
      .then((pressed) => {
        if (pressed.did === "pick") pickerOpen = true;
        if (pressed.did === "stopped") offerOn(pressed.closed);
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
      .then((closed) => {
        push({ text: `Timing ${title}.` });
        // The block the switch closed is a day's work on the *previous*
        // target, and it opens whichever dialog that target calls for: a
        // switch is a stop, and a stop that quietly discarded the offer to log
        // would make the launcher row the one way to lose an afternoon.
        offerOn(closed);
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
    let stopAlerts: (() => void) | undefined;
    let stopContexts: (() => void) | undefined;
    let stopProjects: (() => void) | undefined;
    let stopSourceKinds: (() => void) | undefined;
    let stopTimer: (() => void) | undefined;
    let stopNotify: (() => void) | undefined;
    let stopCapture: (() => void) | undefined;

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
      // The estate's open alerts (#444): the same split again. They move when
      // a sync run reconciles them and when the reader acks one, which are the
      // two events the inbox already subscribes to -- so there is no
      // `alert:*` channel, for the reason `alerts.svelte.ts` records.
      stopAlerts = alerts.start();
      // The stored contexts (#47): same split as health — subscribe now,
      // seed once the database can answer.
      stopContexts = storedContexts.start();
      // The projects a corpus shows (#209): the same split again. Its event is
      // a sync run ending, because a project room appears when the first item
      // carrying it syncs.
      stopProjects = projects.start();
      // Which adapter each source runs (#285), which is what a project room's
      // chip is worded from. Its event is `source:health`, because an adapter
      // kind is immutable per source and only the *set* of sources can move.
      stopSourceKinds = sourceKinds.start();
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
      // The notification listener's click channel (#290, fed since #339 by
      // knobas' own `notification:clicked` event). Behind the same await as
      // everything above it, and for the same reason: `listen` is an
      // `invoke`, and under `?fake-ipc` one issued before the fixture is one
      // into nothing. A rejected subscription is swallowed by the store:
      // notifications still fire, they simply have no door.
      stopNotify = notifications.start();
      // The capture window's *Open in knobas* button (#503). Behind the same
      // await as everything above it and for the same reason: `listen` is an
      // `invoke`, and under `?fake-ipc` one issued before the fixture is one
      // into nothing. The address is the kind-agnostic alias, because a note
      // written in another window is one this shell has never drawn and knows
      // no kind for -- `#/entity/<id>` is what that alias is for.
      //
      // **Not `await`ed, and that is the whole of it.** Every line in this
      // block is synchronous after the one `await` at its head, and an
      // `await listen(...)` here would push everything below it a tick later
      // — so a shell unmounted in that window runs its teardown *first*, and
      // only then is this subscription stored and the lifecycle's own
      // `db:state` installed, each with nothing left to stop it.
      // Measured on 2026-09-08: `residue`'s *App leaves nothing behind after
      // it has been used* went red under a loaded gate with exactly those
      // **two** listeners left, this one and `db:state`.
      //
      // The `disposed` check inside is the other half: `listen` resolves a
      // tick later whatever this line does, so a subscription that lands after
      // the teardown has to be dropped rather than stored.
      void listen<string>(EVENTS.captureOpenNote, (event) => {
        router.go(`#/entity/${event.payload}`);
      }).then((off) => {
        if (disposed) off();
        else stopCapture = off;
      });
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
      stopCapture?.();
      stopNotify?.();
      stopTimer?.();
      stopHealth?.();
      stopMerges?.();
      stopInbox?.();
      stopAlerts?.();
      stopContexts?.();
      stopProjects?.();
      stopSourceKinds?.();
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
   * Seed the open alerts the moment the database can answer — the same rule
   * and the same shape as the seeds above. `open_alerts` rejects with
   * `not_ready` for the whole of bring-up, and alerts only move when a sync
   * run finishes, so a strip seeded at mount would draw no badge until the
   * next run — which on a one-minute interval is a minute of an estate that is
   * already down looking well.
   */
  $effect(() => {
    if (lifecycle.ready) void alerts.refresh();
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
   * Seed the adapter kinds the moment the database can answer (#285) — the
   * same rule and the same shape as the seeds above. `list_sources` rejects
   * with `not_ready` for the whole of bring-up, and `source:health` fires on a
   * change only, so a steady install would otherwise chip every Confluence
   * space room *project* for the session.
   */
  $effect(() => {
    if (lifecycle.ready) void sourceKinds.reseed();
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
   * Seed which kinds may notify the moment the database can answer (#290) —
   * the same rule and the same shape as the seeds above. `notification_kinds`
   * rejects with `not_ready` for the whole of bring-up, and nothing emits an
   * event when a setting is written, so a read at mount would leave the store
   * holding *no kinds* for the session and every notification would be
   * silently gated off.
   */
  $effect(() => {
    if (lifecycle.ready) void notifications.reseed();
  });

  /**
   * **Hand the inbox's stream to the notifier**, which is the whole of the
   * wire between the two (#290).
   *
   * There is no `inbox:*` event and there is deliberately not going to be one
   * (`inbox.svelte.ts` records why), so the change signal the notifier listens
   * to *is* this store moving: the inbox already re-reads on `activity:new`
   * and on every finished sync, and this effect runs when the answer changes.
   *
   * **Gated on `inbox.answered`, and that is the load-bearing half.** The
   * store holds an empty stream until its first read comes back, and a
   * notifier primed against that would take the reader's whole backlog for
   * news — a burst of notifications about nothing new, landing the moment
   * knobas opens.
   *
   * The call runs untracked: `saw` reads the enabled kinds, and a tracked read
   * there would make switching a kind on in settings a reason to walk the
   * stream again.
   */
  $effect(() => {
    const stream = inbox.stream;
    const answered = inbox.answered;
    untrack(() => {
      if (answered) notifications.saw(stream);
    });
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
    // ...and the wizard has just configured the sources those projects belong
    // to, which is the only thing that decides whether their rooms are chipped
    // *space* or *project* (#285).
    void sourceKinds.reseed();
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
      {:else if router.route.view === "time"}
        <!--
          The day review (#279). `day` is `null` for the bare `#/time`, which
          the view resolves against its own clock -- `parseHash` is pure and
          must not read one.
        -->
        <!--
          The day strip and the week under it (#279, #283) — one address, and
          one counter between them. Story 44 wants assigning a block and
          watching the week's unlogged total change to be *one glance*: each
          view bumps `timeRevision` when it writes, and each re-reads when it
          moves. The shell holds the counter because the two are siblings and
          neither owns the other; the time commands deliberately write no
          activity line of their own, so there is no signal to listen to.
        -->
        <DayReview
          {router}
          day={router.route.day}
          revision={timeRevision}
          onchanged={() => (timeRevision += 1)}
        />
        <WeekTimesheet
          day={router.route.day}
          revision={timeRevision}
          onchanged={() => (timeRevision += 1)}
        />
      {:else if router.route.view === "assets"}
        <!--
          The Assets view (#428): the estate as Miller columns with a fixed
          pane. One branch for `#/assets/tree` and `#/asset/<id>` alike — they
          are the same surface, one of them with a selection — so the view
          reads the address itself rather than being handed an id.

          The **tab** is the one thing read here rather than inside, because
          the two tabs are two components (#448): the roster draws no columns,
          no pane and no search box, and one component covering both would
          load the whole estate to show a list of the mirror. What makes them
          one view to a reader is the tab strip, which both draw from
          `AssetsTabs`.
        -->
        {#if router.route.tab === "monitors"}
          <MonitorsView {router} />
        {:else}
          <AssetsView {router} />
        {/if}
      {:else if router.route.view === "standup"}
        <!--
          The standup digest (#288): three lists at their own address, drawn
          for today. The address carries no date -- a digest is this morning's
          standup, and `#/standup/<date>` is the standup *protocol*'s, which is
          a note per date and a different surface.
        -->
        <StandupView {router} />
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
    context={launcherContext}
    oncontext={(targetId, targetTitle) => {
      // Guarded for `onlink`'s reason: the prop outlives one keystroke, and
      // this is where "the reader is standing in a stored context" stops being
      // an assumption. The same value the chain was built from, so the row's
      // label and the toast's cannot name two different rooms.
      const room = launcherContext;
      if (room) void addToContext(room.ctxId, targetId, targetTitle, room.label);
    }}
    ontimer={startTimerOn}
    onnavigate={(hash) => router.go(hash)}
    onclose={() => {}}
  />
  {#if pickerOpen}
    <TimerPicker onpick={startFromPicker} onclose={() => (pickerOpen = false)} />
  {/if}
  {#if adHoc}
    <!--
      *Log to a ticket…* re-targets the block before it opens the draft, and a
      re-targeted block moves a row of the week from one target to another — so
      it bumps the counter for the same reason an edit on the strip does.
    -->
    <AdHocBlockDialog
      block={adHoc.block}
      offer={adHoc.offer}
      onclose={() => (adHoc = null)}
      onlog={(draft) => {
        timeRevision += 1;
        worklog = draft;
      }}
    />
  {/if}
  {#if worklog}
    <!--
      **A worklog made here counts as a time write too** (#283). The draft is
      the ordinary way one gets made — every stop on a ticket offers it — and
      it moves the same two numbers *Log all* moves: the week's `logged` column,
      and the strip's blocks, which have just become read-only. Without the
      bump the reader logs an afternoon and watches the week go on calling it
      unlogged, which is the staleness story 44 is about, arriving through the
      other door.
    -->
    <WorklogDraft
      draft={worklog}
      onclose={() => (worklog = null)}
      onlogged={(logged) => {
        timeRevision += 1;
        push({
          text: `Logged ${Math.round(logged.seconds / 60)}m to ${logged.entity_id.slice(
            logged.entity_id.indexOf(":") + 1,
          )}.`,
        });
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
<Toast />
