<!--
  The top strip — **M1 members only.**

  The context switcher and its tabs (one built-in room until task 18 can list
  the configured sources), the launcher field, the sync monograms (task 18; the group is empty
  until there are sources to put in it), the gear, and settings.

  The **inbox count** joined them in M2 (#45): spec §2 puts it in the strip and
  story 18 is that the number is how a reader knows there is something without
  opening it. It is a button, because a count nobody can act on is chrome — and
  it is *absent* at zero rather than drawn as `0`, for the reason the rest of
  this comment gives.

  The **timer** joined them in M3 (#278): spec §2 puts one global timer in the
  strip, and story 7 is that a reader always knows what the clock is on. It is
  drawn **only while something is running**, the same rule as the inbox count
  and for the same reason — a permanent empty slot is one the eye keeps
  checking, and ⌘T is how a timer starts from anywhere. Its elapsed reading
  flaps (story 16), which is spec §2's rule for a flap literally: a value that
  changes while you watch.

  **Today** joined them in M3.1 (#279): spec §2's day button, opening the day
  review on the current date. Unlike the inbox count and the timer it is
  **always drawn**, because it is a destination rather than a reading — the
  same rule the sources and settings buttons follow. It carries today's date at
  the moment it is pressed, so the address names the day rather than meaning
  "whenever this was opened".

  **Assets** joined them in M4.0 (#428): spec §2's Assets button, opening the
  Tree. Always drawn, the rule *Today* and *Standup* follow — it is a
  destination rather than a reading. It carries **no count**: the open-alert
  badge spec §2 puts on it is M4.1's, and a badge that could only ever read
  zero would be a number nobody could act on, which is the reason the inbox
  count is absent at zero rather than drawn as `0`.
-->
<script lang="ts">
  import { untrack } from "svelte";

  import { inbox as sharedInbox, type Inbox } from "../inbox/inbox.svelte";
  import { getAsset as realGetAsset, type AssetDetail } from "../ipc/assets";
  import type { AuthState } from "../ipc/sources";
  import ContextTabs from "./ContextTabs.svelte";
  import Flap from "./Flap.svelte";
  import Monogram from "./Monogram.svelte";
  import { sourceMonogram } from "./monogram";
  import { builtinContexts, type RoomContext } from "./contexts";
  import { health as sharedHealth, isActionable, type Health } from "./health.svelte";
  import { latestRead } from "./latest-read";
  import { hashFor, type Router } from "./router.svelte";
  import { timer as sharedTimer, type Timer } from "./timer.svelte";
  import { assetTargetId, targetReading } from "./timer";
  import { dayKey } from "../time/day";

  let {
    router,
    onsearch,
    contexts = builtinContexts([]),
    health = sharedHealth,
    inbox = sharedInbox,
    timer = sharedTimer,
    ontimer,
    asset,
  }: {
    router: Router;
    onsearch: () => void;
    /**
     * The live timer store the slot draws.
     *
     * A prop with the shell's singleton as its default, the shape `health`
     * uses: the strip is correct whatever a caller passes, and a test can hand
     * it a store with no Tauri bridge behind it.
     */
    timer?: Timer;
    /**
     * Stop the running timer — what the slot's button does, and the same verb
     * ⌘T performs while one is running.
     *
     * The strip does not stop it itself. Stopping opens the worklog draft in
     * #280, and the draft is the shell's; a strip that called the store
     * directly would be a second place that decision was made. `undefined`
     * leaves the slot a plain reading with no button, which is what a caller
     * with nothing to do about it gets.
     *
     * **Required, and deliberately not defaulted** (the rule #238 set and
     * `shell/contexts.ts`'s `switcherContexts` records): this prop is the
     * whole of the join between the strip's slot and the shell's stop, and an
     * optional one can be dropped from a call site, type-check clean, and
     * leave a permanently disabled button that no test fails on. A caller with
     * nothing to do about the timer says so by passing `undefined`.
     */
    ontimer: (() => void) | undefined;
    /** The rooms the switcher offers. Defaults to *All work* alone. */
    contexts?: RoomContext[];
    /**
     * The live inbox store the count is read from.
     *
     * **Read, never counted here.** `inbox.count` is the backend's own
     * statement counted, and it excludes snoozed items because the number
     * means "needs me now" (#45, story 19). A badge derived from the length of
     * whatever list this side happens to be holding would be a second
     * definition of that, and snoozing is the first thing it would get wrong.
     */
    inbox?: Inbox;
    /**
     * The live `source:health` store the cluster draws.
     *
     * A prop with the shell's singleton as its default, so the strip is
     * correct whatever a caller passes and a test can hand it a store with no
     * Tauri bridge behind it.
     */
    health?: Health;
    /**
     * Read one asset, so the slot can name an asset target (#437).
     *
     * A port with the real command as its default, the shape `timer` and
     * `health` have: the strip is correct whatever a caller passes, and a test
     * can hand it an estate with no Tauri bridge behind it.
     *
     * **Called for an asset target and for nothing else.** Every other target
     * is already readable off its own id — `jira:PAY-231` reads `PAY-231`, the
     * string the reader would have typed — and asking the estate about one
     * would be a `not_found` per ticket.
     */
    asset?: ((assetId: string) => Promise<AssetDetail>) | undefined;
  } = $props();

  // svelte-ignore state_referenced_locally
  // Read once, at init: production omits this prop, and a bridge swapped
  // mid-life would leave the name on screen read through one estate and
  // re-read through another.
  const readAsset = asset ?? realGetAsset;

  const onSources = $derived(router.route.view === "sources");
  const onInbox = $derived(router.route.view === "inbox");
  /**
   * §14's settings surface (#69). A second labelled button rather than a menu
   * behind the gear: there are two destinations, and a two-item menu costs a
   * click to say what a second button says by being there.
   */
  const onSettings = $derived(router.route.view === "settings");
  /** §2's *Today*: the day review, on the day it is pressed (#279). */
  const onTime = $derived(router.route.view === "time");
  /** *Standup*: the digest at its own address (#288). */
  const onStandup = $derived(router.route.view === "standup");
  /**
   * §2's *Assets*: the estate (#428).
   *
   * True for `#/asset/<id>` as well as `#/assets/tree` — they are one view,
   * so the button reads as current when the reader is standing in either.
   */
  const onAssets = $derived(router.route.view === "assets");

  /** The asset the clock is on, or `null` for every other target (#437). */
  const onAsset = $derived(assetTargetId(timer.current?.target ?? null));

  /**
   * That asset once the estate has answered for it, keyed by the id it was
   * read for.
   *
   * The id travels with the name so the slot can refuse a name that belongs to
   * the *previous* target: a stop and a start on another asset are two events
   * a beat apart, and a slot that drew whatever the last read produced would
   * name the machine the reader has just stopped working on.
   */
  let named = $state<{ id: string; name: string; monogram: string } | null>(null);
  const readAssetLatest = latestRead<AssetDetail>();

  /**
   * Read the asset the clock is on, and **try again until it is named**.
   *
   * Tracked on `timer.current` as well as on the id, and that is the retry:
   * the store re-reads the timer on every activity line and every heartbeat
   * (`timer.svelte.ts`), so each of those is a fresh attempt at a name that is
   * still missing. Without it, the commonest case would be the permanently
   * unnamed one — `get_asset` rejects `not_ready` for the whole of bring-up,
   * which is exactly when a relaunch-restored timer is first drawn, and a
   * single attempt would leave the reader looking at a uuid for the rest of
   * the session.
   *
   * The read is skipped once the name is in hand, so a beat every thirty
   * seconds does not cost a round trip for an answer the slot already has.
   * `untrack` on that check, because reading `named` in an effect that writes
   * it is a loop for no gain: the value it is checked against was written by
   * this same effect.
   */
  $effect(() => {
    const id = onAsset;
    timer.current;
    if (id === null) {
      named = null;
      return;
    }
    if (untrack(() => named)?.id === id) return;
    void readAssetLatest(() => readAsset(id), {
      ok: (detail) => {
        named = { id, name: detail.asset.name, monogram: detail.asset.monogram };
      },
      fail: () => {
        // **Nothing is said, and the slot is not dropped.** Spec §2's promise
        // is that a reader always knows the clock is running; a name knobas
        // could not fetch is not worth trading that for, and the id's key is
        // still a true thing to draw. The next beat tries again.
        named = null;
      },
    });
  });

  /** The asset's name and chip, or `null` — see {@link named} for the key. */
  const heldAsset = $derived(named !== null && named.id === onAsset ? named : null);

  /**
   * What the slot calls the thing the clock is on.
   *
   * The asset's name where the estate has answered, and `targetReading`'s key
   * everywhere else — including an asset whose read has not landed yet, which
   * is a uuid and is still better than an empty slot.
   */
  const timerReading = $derived(
    timer.current === null ? "" : (heldAsset?.name ?? targetReading(timer.current.target)),
  );

  /**
   * How each state reads in the cluster's tooltip.
   *
   * Total over `AuthState` for the same reason `isActionable`'s table is: a
   * variant added on the Rust side has to fail `svelte-check` here rather than
   * fall through to a fallback nobody notices.
   */
  const STATE_WORD: Record<AuthState, string> = {
    ok: "ok",
    unauthorized: "credential rejected",
    unreachable: "server unreachable",
    missing_secret: "no credential stored",
    unknown: "not checked yet",
  };

  /**
   * `401`, and only for `unauthorized`.
   *
   * Spec §2 highlights the one state a person must resolve themselves: the
   * scheduler backs off and retries an `unreachable` source on its own (P7)
   * and will never retry a rejected credential. Labelling both would make the
   * loud reading mean "something is off somewhere", which is not a call to
   * action.
   */
  const unauthorized = $derived(health.all.filter((source) => source.state === "unauthorized"));

  /**
   * The whole list, in the tooltip.
   *
   * 44 px of strip cannot hold five source names and their states, and the
   * mockup put them here for the same reason (`signal-miller.html:2299`).
   */
  const tooltip = $derived(
    health.all
      .map((source) => `${source.source_id}: ${STATE_WORD[source.state]}`)
      .join("\n"),
  );
</script>

<header class="topbar">
  <ContextTabs {router} {contexts} />

  <button class="searchfield" onclick={onsearch} title="Search everything and act in place (⌘K)">
    <svg viewBox="0 0 16 16" aria-hidden="true">
      <circle cx="7" cy="7" r="4.5" />
      <path d="M10.5 10.5 14 14" />
    </svg>
    <span class="ph">Search</span>
    <kbd>⌘K</kbd>
  </button>

  <span class="spacer"></span>

  <!--
    Absent at zero, deliberately. An empty inbox is the state a person should
    be able to stop thinking about, and a permanent `0` in the strip is a slot
    the eye keeps checking. The address stays reachable either way.
  -->
  {#if inbox.count > 0}
    <button
      class="tb-btn {onInbox ? 'on' : ''}"
      aria-current={onInbox ? "page" : undefined}
      aria-label="Inbox: {inbox.count} needing you"
      title="Inbox — {inbox.count} needing you now (snoozed items are not counted)"
      onclick={() => router.go("#/inbox")}
    >
      <svg viewBox="0 0 16 16" aria-hidden="true">
        <path d="M1.8 8.5h3l1 2h4.4l1-2h3M1.8 8.5 3.6 3h8.8l1.8 5.5v4a1 1 0 0 1-1 1H2.8a1 1 0 0 1-1-1z" />
      </svg>
      <span class="k">{inbox.count}</span>
    </button>
  {/if}

  <button
    class="tb-btn {onTime ? 'on' : ''}"
    aria-current={onTime ? "page" : undefined}
    title="Today — the day's blocks, editable"
    onclick={() => router.go(hashFor({ view: "time", day: dayKey(new Date()) }))}
  >
    <svg viewBox="0 0 16 16" aria-hidden="true">
      <rect x="2" y="3" width="12" height="11" rx="1" />
      <path d="M2 6.5h12M5.5 1.8v2.4M10.5 1.8v2.4" />
    </svg>
    <span class="k">Today</span>
  </button>

  <!--
    *Standup* (#288). Beside *Today* because they are the same kind of thing —
    a surface about the day, one click from anywhere — and a digest nothing
    opens is a digest nobody reads. The address carries no date: the digest is
    this morning's standup, defined against today.
  -->
  <button
    class="tb-btn {onStandup ? 'on' : ''}"
    aria-current={onStandup ? "page" : undefined}
    title="Standup — yesterday, today and blockers"
    onclick={() => router.go(hashFor({ view: "standup" }))}
  >
    <svg viewBox="0 0 16 16" aria-hidden="true">
      <path d="M2.5 4h11M2.5 8h7M2.5 12h9" />
    </svg>
    <span class="k">Standup</span>
  </button>

  <!--
    *Assets* (#428). Beside *Standup* because it is the same kind of thing — a
    destination one click from anywhere. The address is the Tree tab, which is
    what the view opens on; a reader who was last on an asset gets the Tree at
    the top rather than back where they were, which is the same rule *Today*
    follows in carrying the date it was pressed on.
  -->
  <button
    class="tb-btn {onAssets ? 'on' : ''}"
    aria-current={onAssets ? "page" : undefined}
    title="Assets — the estate as a tree"
    onclick={() => router.go(hashFor({ view: "assets", tab: "tree", assetId: null }))}
  >
    <svg viewBox="0 0 16 16" aria-hidden="true">
      <rect x="2" y="2.5" width="12" height="4" rx="1" />
      <rect x="2" y="9.5" width="12" height="4" rx="1" />
      <path d="M4.5 4.5h.01M4.5 11.5h.01" />
    </svg>
    <span class="k">Assets</span>
  </button>

  <!--
    Absent when nothing is running, deliberately — the inbox count's rule.
    ⌘T starts a timer from anywhere, so an empty slot would buy nothing and
    cost a permanent place for the eye to check.
  -->
  {#if timer.current}
    <button
      class="tb-btn timer run"
      aria-label="Timing {timerReading} — {timer.elapsed}"
      title="Timing {timerReading}. Stop it with ⌘T."
      onclick={() => ontimer?.()}
      disabled={ontimer === undefined}
    >
      <!--
        `.timer`, `.pulse` and `.ctx` are the mockup's own classes, ported into
        `app.css` with the rest of the sheet and dormant until now: this is the
        slot they were written for.
      -->
      <span class="pulse" aria-hidden="true"></span>
      <!--
        The asset's own chip — `VM`, `CT`, `DB` — and the same one the Tree's
        columns put on the row, because it is the asset's monogram and not the
        kind's: on this surface the question is *which machine*, and `AS` would
        answer *an asset*, which the name already says. Drawn only for an asset,
        which is the one target whose name is not in its id.
      -->
      {#if heldAsset}
        <span class="mg" title={heldAsset.id}>{heldAsset.monogram}</span>
      {/if}
      <span class="ctx">{timerReading}</span>
      <!--
        A flap: the archetypal value that changes while you watch (spec §2,
        story 16). `Flap` honours `prefers-reduced-motion` itself, so a reader
        who asked for stillness gets the number without the leaves.
      -->
      <Flap value={timer.elapsed ?? "0:00"} width="s" label="time on this timer" />
    </button>
  {/if}

  {#if health.all.length > 0}
    <button
      class="sync"
      title={tooltip}
      aria-label="Source credential health"
      onclick={() => router.go("#/sources")}
    >
      {#each health.all as source (source.source_id)}
        <Monogram
          text={sourceMonogram(source.source_id)}
          tone={isActionable(source.state) ? "err" : "ok"}
          label="{source.source_id}: {STATE_WORD[source.state]}"
        />
      {/each}
      {#if unauthorized.length > 0}
        <span class="err-txt">
          <span class="lbl">credential rejected</span> 401
        </span>
      {/if}
    </button>
  {/if}

  <button
    class="tb-btn {onSources ? 'on' : ''}"
    aria-label="Sources"
    aria-current={onSources ? "page" : undefined}
    onclick={() => router.go("#/sources")}
  >
    <svg viewBox="0 0 16 16" aria-hidden="true">
      <circle cx="8" cy="8" r="2.2" />
      <path
        d="M8 1.5v2M8 12.5v2M1.5 8h2M12.5 8h2M3.4 3.4l1.4 1.4M11.2 11.2l1.4 1.4M3.4 12.6l1.4-1.4M11.2 4.8l1.4-1.4"
      />
    </svg>
  </button>

  <!-- Sliders, not a second gear: two identical icons say the two surfaces are
       the same place. -->
  <button
    class="tb-btn {onSettings ? 'on' : ''}"
    aria-label="Settings"
    aria-current={onSettings ? "page" : undefined}
    onclick={() => router.go("#/settings")}
  >
    <svg viewBox="0 0 16 16" aria-hidden="true">
      <path d="M2 4.5h12M2 8h12M2 11.5h12" />
      <circle cx="5.5" cy="4.5" r="1.6" />
      <circle cx="10" cy="8" r="1.6" />
      <circle cx="6.5" cy="11.5" r="1.6" />
    </svg>
  </button>
</header>
