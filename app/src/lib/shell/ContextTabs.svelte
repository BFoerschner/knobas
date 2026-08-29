<!--
  The context switcher and its tabs (`signal-miller.html:2283-2289`).

  M1's contexts are read-only and derived — see `contexts.ts` for why. What
  carries over unchanged is the *shape*: the current room's name opening a
  popover over the whole list, then one tab each.

  Navigation is `router.go`, never a local `selected`, because the address is
  the state (spec §2): a room is a place, and a place has to survive a reload
  and be pasteable into a message.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../ipc";
  import { createContext } from "../ipc/entity";
  import type { RoomContext } from "./contexts";
  import { contextById } from "./contexts";
  import { openFreshContext } from "./contexts.svelte";
  import type { Router } from "./router.svelte";
  import { push } from "./toasts.svelte";

  let { router, contexts }: { router: Router; contexts: RoomContext[] } = $props();

  let open = $state(false);

  /** Whether the *new* tab is an input right now. */
  let naming = $state(false);
  /** The ad-hoc label being typed. */
  let label = $state("");
  /** True while the create is in flight, so Enter cannot double-fire. */
  let creating = $state(false);

  /** Mint the ad-hoc context and go there (#47). */
  async function create() {
    const title = label.trim();
    if (title === "" || creating) return;
    creating = true;
    try {
      const row = await createContext(title);
      naming = false;
      label = "";
      await openFreshContext(row, (hash) => router.go(hash));
    } catch (rejection) {
      push({ text: `Could not create the context: ${ipcErrorMessage(rejection)}`, tone: "err" });
    } finally {
      creating = false;
    }
  }

  /** The room the reader is in, or the one they will return to. */
  const current = $derived(contextById(router.ctx, contexts));
  /** Whether a tab may be `on` at all — `#/sources` highlights none of them. */
  const inRoom = $derived(router.route.view === "room");

  function go(id: string) {
    open = false;
    router.go(`#/ctx/${id}`);
  }

  $effect(() => {
    if (!open) return;
    // A pointer press anywhere else dismisses it. On `pointerdown` rather than
    // `click` so a press that starts outside closes before the element under
    // it is activated, and captured so a handler that stops propagation
    // cannot leave the popover stranded open.
    const dismiss = (event: PointerEvent) => {
      if (event.target instanceof Node && switcher?.contains(event.target)) return;
      open = false;
    };
    window.addEventListener("pointerdown", dismiss, true);
    return () => window.removeEventListener("pointerdown", dismiss, true);
  });

  let switcher = $state<HTMLDivElement | null>(null);
</script>

<div class="ctx-switch" bind:this={switcher}>
  <button
    class="ctx-name"
    aria-haspopup="true"
    aria-expanded={open}
    title="Switch context"
    onclick={() => (open = !open)}
  >
    <span class="nm">{current.label}</span>
    <svg viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke-width="1.5">
      <path d="M4 6l4 4 4-4" />
    </svg>
  </button>

  {#if open}
    <!--
      Escape closes the popover and stops there: rung 1 of the ladder in
      `keys.ts`. Without the `stopPropagation` one keypress would both close
      this and unwind the detail slide-over behind it.
    -->
    <div
      class="pop ctx-pop"
      role="menu"
      tabindex="-1"
      onkeydown={(event) => {
        if (event.key !== "Escape") return;
        event.preventDefault();
        event.stopPropagation();
        open = false;
      }}
    >
      <div class="lab">Contexts</div>
      {#each contexts as context (context.id)}
        <button
          class="it {inRoom && context.id === router.ctx ? 'on' : ''}"
          role="menuitem"
          onclick={() => go(context.id)}
        >
          <span>{context.label}</span>
          <span class="k">{context.kindWord}</span>
        </button>
      {/each}
    </div>
  {/if}
</div>

<div class="tabs">
  {#each contexts as context (context.id)}
    <button
      class="tab {inRoom && context.id === router.ctx ? 'on' : ''}"
      aria-current={inRoom && context.id === router.ctx ? "page" : undefined}
      onclick={() => go(context.id)}>{context.label}</button
    >
  {/each}
  {#if naming}
    <!--
      The tab becomes the input: an ad-hoc context is a label and nothing
      else (spec §7), so there is nothing a dialog would add. Escape backs
      out; blur backs out too unless a create is already in flight.
    -->
    <!-- svelte-ignore a11y_autofocus -->
    <input
      class="tab new-name"
      autofocus
      aria-label="New context label"
      placeholder="Context label…"
      disabled={creating}
      bind:value={label}
      onkeydown={(event) => {
        if (event.key === "Enter") void create();
        if (event.key === "Escape") {
          event.stopPropagation();
          naming = false;
          label = "";
        }
      }}
      onblur={() => {
        if (!creating) naming = false;
      }}
    />
  {:else}
    <button class="tab new" title="New ad-hoc context" onclick={() => (naming = true)}>
      <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M8 3v10M3 8h10" /></svg>
      new
    </button>
  {/if}
</div>
