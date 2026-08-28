<!--
  The top strip — **M1 members only.**

  The context switcher and its tabs (one built-in room until task 18 can list
  the configured sources), the launcher field, the sync monograms (task 18; the group is empty
  until there are sources to put in it), and the gear.

  The timer, inbox, Assets and Today/Day buttons of spec §2 are M2/M3/M4 and
  are deliberately absent. Reserving a slot for a button that cannot work is
  how a shell fills up with dead chrome, and a disabled control teaches the
  reader nothing except that the app is unfinished.
-->
<script lang="ts">
  import type { AuthState } from "../ipc/sources";
  import ContextTabs from "./ContextTabs.svelte";
  import Monogram from "./Monogram.svelte";
  import { builtinContexts, type RoomContext } from "./contexts";
  import { health as sharedHealth, isActionable, type Health } from "./health.svelte";
  import type { Router } from "./router.svelte";

  let {
    router,
    onsearch,
    contexts = builtinContexts([]),
    health = sharedHealth,
  }: {
    router: Router;
    onsearch: () => void;
    /** The rooms the switcher offers. Defaults to *All work* alone. */
    contexts?: RoomContext[];
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

  {#if health.all.length > 0}
    <button
      class="sync"
      title={tooltip}
      aria-label="Source credential health"
      onclick={() => router.go("#/sources")}
    >
      {#each health.all as source (source.source_id)}
        <Monogram
          text={source.source_id.slice(0, 2).toUpperCase()}
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
</header>
