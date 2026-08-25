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
  import ContextTabs from "./ContextTabs.svelte";
  import { builtinContexts } from "./contexts";
  import type { Router } from "./router.svelte";

  let {
    router,
    onsearch,
    sources = [],
  }: {
    router: Router;
    onsearch: () => void;
    /**
     * The configured sources, one room each.
     *
     * Empty until task 18 can list them — stream F owns `listSources`, and
     * declaring its shape here would be inventing another stream's interface.
     * An empty list is not a placeholder: it is the truth for a knobas that
     * has synced nothing.
     */
    sources?: { id: string; label: string }[];
  } = $props();

  const contexts = $derived(builtinContexts(sources));
  const onSources = $derived(router.route.view === "sources");
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

  <!-- Task 18 fills this with one monogram per configured source. -->
  <span class="sync"></span>

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
