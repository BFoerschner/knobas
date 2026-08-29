<!--
  The status bar (`signal-miller.html:2304-2310`).

  Spec §2 wants *"DB size, FTS freshness, sync cadence, counts, pending writes,
  user, clock"* plus the latest-change one-liner. What is here is what M1 can
  actually answer, and everything else is an em dash rather than a
  plausible-looking zero — a status bar that says `0 items` when it simply has
  not asked is worse than one that says it does not know.

  * **DB size and counts** come from `dbStats()`, re-read whenever a run
    finishes and every 60 s otherwise. Until it answers they are em dashes:
    `0 items` when the bar has simply not asked is worse than saying so.
  * **Sync cadence** is the *soonest* `next_run_at` across every source, in a
    flap because it moves while you watch. Counting down to the latest one
    would leave the bar reading nine minutes while a sync ran four minutes
    earlier.
  * **Latest change** is seeded from `recentActivity(1)`, moved by
    `activity:new`, coalesced.
  * **Pending writes** is the write queue (issue #42), and it is a *button*:
    the number is the only place a held write announces itself, so it has to be
    the way in. Two numbers, never one — "3 waiting" that quietly included a
    write needing a decision would be the badge telling the reader that a
    conflict resolves itself.
  * **The user** is omitted. There is no identity model until M2, and an
    invented username is worse than an empty slot.
-->
<script lang="ts">
  import { listen } from "@tauri-apps/api/event";

  import { EVENTS } from "../ipc";
  import { recentActivity, type ActivityRow } from "../ipc/entity";
  import { dbStats, syncStatus, type DbStats, type SourceSyncStatus } from "../ipc/sources";
  import { countdown, formatBytes, formatInterval } from "../sources/diagnostics";
  import Flap from "./Flap.svelte";
  import type { Lifecycle } from "./lifecycle.svelte";
  import { createLatestChange } from "./latest-change.svelte";
  import { ago } from "./time";
  import WriteQueue from "./WriteQueue.svelte";
  import { createWriteQueue, decisionsIn } from "./write-queue.svelte";

  let {
    lifecycle,
    now: fixedNow,
  }: {
    lifecycle: Lifecycle;
    /** Injectable clock, so the countdown is testable rather than waited for. */
    now?: Date;
  } = $props();

  const latest = createLatestChange();
  const queue = createWriteQueue();

  /** Whether the queue panel is open. */
  let queueOpen = $state(false);

  // svelte-ignore state_referenced_locally
  // Read once: a fixed clock is a test's decision and never changes after
  // mount, and the ticking one below is what production uses.
  let now = $state(fixedNow ?? new Date());

  /** How often the database numbers are re-read absent a sync transition. */
  const STATS_MS = 60_000;

  let stats = $state<DbStats | null>(null);
  /** The live per-source schedule, keyed by source id. */
  let schedule = $state<Record<string, SourceSyncStatus>>({});

  $effect(() => {
    // A fixed clock is a test's, and must not be overwritten a second later —
    // a countdown that jumped to the real time one tick in would make every
    // assertion about it a race.
    if (fixedNow) return;
    // Inside the effect, with a teardown — not at module scope. A module-level
    // interval survives hot reload and every remount, so a dev session ends up
    // with a dozen of them ticking against a component that is long gone.
    const timer = setInterval(() => {
      now = new Date();
    }, 1000);
    return () => clearInterval(timer);
  });

  /**
   * The database numbers and the schedule.
   *
   * Two independent reads with a `catch` each: `db_stats` failing must not
   * take the cadence down with it, and neither may take the window down.
   */
  $effect(() => {
    // House rule: nothing is asked before the database is up.
    if (!lifecycle.ready) return;

    let dead = false;
    let off: (() => void) | undefined;

    const read = () => {
      void dbStats()
        .then((next) => {
          if (!dead) stats = next;
        })
        .catch(() => {
          // Em dashes, not zeroes. A status bar cannot tell a real zero from a
          // question it never got to ask, so it says so.
          if (!dead) stats = null;
        });
      void syncStatus()
        .then((rows) => {
          if (dead) return;
          const next: Record<string, SourceSyncStatus> = {};
          for (const row of rows) next[row.source_id] = row;
          schedule = next;
        })
        .catch(() => {
          // Leave whatever the events have already said.
        });
    };

    // The event carries a whole `SourceSyncStatus`, so the schedule moves
    // without a round trip; only the *counts* need re-reading, and only when a
    // run has actually finished.
    void listen<SourceSyncStatus>(EVENTS.syncState, (event) => {
      if (dead) return;
      schedule = { ...schedule, [event.payload.source_id]: event.payload };
      if (!event.payload.running) {
        void dbStats()
          .then((next) => {
            if (!dead) stats = next;
          })
          .catch(() => {});
      }
    })
      .then((unlisten) => {
        if (dead) unlisten();
        else off = unlisten;
      })
      .catch(() => {});

    read();
    const timer = setInterval(read, STATS_MS);

    return () => {
      dead = true;
      off?.();
      clearInterval(timer);
    };
  });

  /**
   * The soonest scheduled run across every source.
   *
   * The *soonest*, not the latest and not any particular source's: the bar has
   * one slot and the question it answers is "when does anything happen next".
   */
  const nextRunAt = $derived.by(() => {
    const stamps = Object.values(schedule)
      .map((status) => status.next_run_at)
      .filter((stamp): stamp is string => stamp !== null)
      .map((stamp) => new Date(stamp))
      .filter((at) => !Number.isNaN(at.getTime()));
    if (stamps.length === 0) return null;
    return new Date(Math.min(...stamps.map((at) => at.getTime()))).toISOString();
  });

  /**
   * The cadence, when every source shares one.
   *
   * `sync_interval_secs` is per source and lives on `SourceSummary`, which this
   * bar does not read. What it *can* say from `SourceSyncStatus` is the gap
   * between the soonest next run and the finish it was derived from (P7:
   * interval = seconds after the previous run finished), and only when that is
   * unambiguous.
   */
  const cadence = $derived.by(() => {
    const rows = Object.values(schedule).filter(
      (status) => status.next_run_at !== null && status.last_finished_at !== null,
    );
    if (rows.length === 0) return null;
    const gaps = new Set(
      rows.map((status) =>
        Math.round(
          (new Date(status.next_run_at!).getTime() - new Date(status.last_finished_at!).getTime()) /
            1000,
        ),
      ),
    );
    return gaps.size === 1 ? formatInterval([...gaps][0]!) : null;
  });

  $effect(() => {
    // House rule: no data call before the database is up. `recent_activity`
    // rejects with `not_ready` during bring-up, and catching that to ignore it
    // would be the shell pretending it did not know.
    if (!lifecycle.ready) return;

    let dead = false;
    let off: (() => void) | undefined;

    // The listener first, then the seed: a line that arrives between the two
    // is then either delivered by the event or already in the seed, whereas
    // the other order has a window in which it is neither.
    // The queue moves on exactly the same signal, because every one of its
    // transitions writes an activity line -- which is why it needs no event of
    // its own (`commands::sources::pending_writes` says so in place).
    void listen<ActivityRow>(EVENTS.activityNew, (event) => {
      latest.push(event.payload);
      void queue.refresh();
    })
      .then((unlisten) => {
        // Unmounted while `listen` was in flight: `listen` is itself an
        // `invoke`, so it resolves a tick or more later.
        if (dead) unlisten();
        else off = unlisten;
      })
      .catch(() => {
        // A failed subscription is not a failed window: the seed below still
        // renders, and the line simply stops moving.
      });

    void queue.refresh();

    void recentActivity(1)
      .then((rows) => {
        const first = rows[0];
        if (!dead && first) latest.push(first);
      })
      .catch(() => {
        // Nothing to show is the ordinary state of a fresh corpus.
      });

    return () => {
      dead = true;
      off?.();
      latest.stop();
    };
  });

  const clock = $derived(
    now.toLocaleString(undefined, {
      weekday: "short",
      year: "numeric",
      month: "2-digit",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
    }),
  );

  /** Who caused the latest change, in the status bar's own shorthand. */
  const latestWho = $derived(
    latest.current === null
      ? ""
      : latest.current.actor === "user"
        ? "you"
        : latest.current.actor.replace(/^sync:/, ""),
  );
</script>

<footer class="statusbar">
  <span>knobas {lifecycle.status?.app_version ?? ""}</span>
  <span>profile {lifecycle.status?.demo ? "demo" : "default"}</span>

  <span title="The size of the knobas database on disk">
    postgres knobas · {stats ? formatBytes(stats.db_bytes) : "—"}
  </span>
  <span title="Entities in the index, and rows in the source mirror">
    {stats ? stats.entity_count : "—"} entities · {stats ? stats.item_count : "—"} items
  </span>
  <span title="How often sources sync, and when the next run is due">
    sync {cadence ?? "—"} · next in
    <!--
      A flap: this is the archetypal value that changes while you watch, which
      is spec §2's whole rule for when one is warranted.
    -->
    <Flap value={countdown(nextRunAt, now)} width="s" label="time until the next sync" />
  </span>

  {#if latest.current}
    <span class="latest" title="The newest line in the activity log">
      <b>{latest.current.verb}</b>
      {latest.current.entity_id ?? ""}
      · {latestWho} · {ago(latest.current.at, now)}
    </span>
  {/if}

  <span class="spacer"></span>
  <!--
    A button, because the badge is the only place a held write announces
    itself and there has to be a way in from it. The decisions are a separate
    span with its own colour rather than a larger total: story 18 is that a
    write needing an answer is distinguishable *at a glance*, and one number
    covering both is precisely the thing that hides it.
  -->
  <button
    class="pend"
    class:on={decisionsIn(queue.counts) > 0}
    onclick={() => (queueOpen = true)}
    title="Writes knobas still owes a source. Open to see what is queued."
  >
    pending writes {queue.counts.pending}
    {#if decisionsIn(queue.counts) > 0}
      <span class="decide">· {decisionsIn(queue.counts)} need you</span>
    {/if}
  </button>
  <span>{clock}</span>
</footer>

{#if queueOpen}
  <WriteQueue {queue} {now} onclose={() => (queueOpen = false)} />
{/if}

<style>
  .latest {
    /*
      The one variable-width member. It is allowed to be clipped rather than to
      push the clock off the end — `.statusbar` is `overflow: hidden`, and the
      readings either side of it are the fixed ones.
    */
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .latest b {
    font-weight: 500;
    color: var(--muted);
  }

  /*
    A button that reads as the rest of the bar: the status bar is a row of
    readings, and one of them being a control must not make it look like a
    toolbar. `app.css`'s own reset already gives a button `font: inherit`,
    `color: inherit` and no border, so only the layout is set here -- and the
    colour is left alone so that `.statusbar .pend.on`, the sheet's existing
    amber state, is what lifts the whole reading when a decision is owed.
  */
  .pend {
    display: inline-flex;
    gap: 5px;
    padding: 0;
    cursor: pointer;
  }
</style>
