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

  The timer, Assets and Today/Day buttons of spec §2 are M3/M4 and are
  deliberately absent. Reserving a slot for a button that cannot work is how a
  shell fills up with dead chrome, and a disabled control teaches the reader
  nothing except that the app is unfinished.
-->
<script lang="ts">
  import { inbox as sharedInbox, type Inbox } from "../inbox/inbox.svelte";
  import type { AuthState } from "../ipc/sources";
  import ContextTabs from "./ContextTabs.svelte";
  import Monogram from "./Monogram.svelte";
  import { sourceMonogram } from "./monogram";
  import { builtinContexts, type RoomContext } from "./contexts";
  import { health as sharedHealth, isActionable, type Health } from "./health.svelte";
  import type { Router } from "./router.svelte";

  let {
    router,
    onsearch,
    contexts = builtinContexts([]),
    health = sharedHealth,
    inbox = sharedInbox,
  }: {
    router: Router;
    onsearch: () => void;
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
  } = $props();

  const onSources = $derived(router.route.view === "sources");
  const onInbox = $derived(router.route.view === "inbox");
  /**
   * §14's settings surface (#69). A second labelled button rather than a menu
   * behind the gear: there are two destinations, and a two-item menu costs a
   * click to say what a second button says by being there.
   */
  const onSettings = $derived(router.route.view === "settings");

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
