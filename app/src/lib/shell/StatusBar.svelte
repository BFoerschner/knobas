<!--
  The status bar (`signal-miller.html:2304-2310`).

  Spec §2 wants *"DB size, FTS freshness, sync cadence, counts, pending writes,
  user, clock"* plus the latest-change one-liner. What is here is what M1 can
  actually answer, and everything else is an em dash rather than a
  plausible-looking zero — a status bar that says `0 items` when it simply has
  not asked is worse than one that says it does not know.

  * **DB size and counts** come from stream F's `dbStats` — task 19.
  * **Sync cadence** comes from `syncStatus().next_run_at` — task 19.
  * **Latest change** is this task: seeded from `recentActivity(1)`, moved by
    `activity:new`, coalesced.
  * **Pending writes** is a constant `0`, and honestly so: the write queue is
    M2, so there is nothing that could be pending.
  * **The user** is omitted. There is no identity model until M2, and an
    invented username is worse than an empty slot.
-->
<script lang="ts">
  import { listen } from "@tauri-apps/api/event";

  import { EVENTS } from "../ipc";
  import { recentActivity, type ActivityRow } from "../ipc/entity";
  import type { Lifecycle } from "./lifecycle.svelte";
  import { createLatestChange } from "./latest-change.svelte";
  import { ago } from "./time";

  let { lifecycle }: { lifecycle: Lifecycle } = $props();

  const latest = createLatestChange();

  let now = $state(new Date());

  $effect(() => {
    // Inside the effect, with a teardown — not at module scope. A module-level
    // interval survives hot reload and every remount, so a dev session ends up
    // with a dozen of them ticking against a component that is long gone.
    const timer = setInterval(() => {
      now = new Date();
    }, 1000);
    return () => clearInterval(timer);
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
    void listen<ActivityRow>(EVENTS.activityNew, (event) => latest.push(event.payload))
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

  <!-- Task 19: `dbStats()` and `syncStatus()` are stream F's, and not merged. -->
  <span title="Database size — arrives with the sources view (task 19)">postgres knobas · —</span>
  <span title="Entity and mirror-row counts — arrive with the sources view (task 19)">
    — entities · — items
  </span>
  <span title="Sync cadence — arrives with the scheduler (task 19)">sync every — · next in —</span>

  {#if latest.current}
    <span class="latest" title="The newest line in the activity log">
      <b>{latest.current.verb}</b>
      {latest.current.entity_id ?? ""}
      · {latestWho} · {ago(latest.current.at, now)}
    </span>
  {/if}

  <span class="spacer"></span>
  <!--
    A real zero, not a placeholder: M1 has no write queue at all, so nothing
    can be pending. It renders so the slot exists where M2 will put a number
    that moves.
  -->
  <span class="pend" title="Queued write-backs — the write queue arrives in M2">
    pending writes 0
  </span>
  <span>{clock}</span>
</footer>

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
</style>
