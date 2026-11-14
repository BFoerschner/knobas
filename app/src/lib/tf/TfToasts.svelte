<!--
  Toasts in the ticket-first shell (#809, R-1.8): the same stack the rooms UI
  draws (`shell/toasts.svelte.ts`), so every `push` anywhere in the app lands
  here, drawn as prototype-6 draws one: ink on the page, the words of the
  button that caused it, and at most its actions as underlined words.

  Text only: a toast often carries a source's own error message (gotcha 7).
-->
<script lang="ts">
  import { dismiss, hold, release, toasts, type ToastAction } from "../shell/toasts.svelte";
  import TfKeyed from "./TfKeyed.svelte";
  import { modals } from "./modals.svelte";

  /**
   * What held focus before a toast took it. A pressed toast goes away with the button in it, and a dialog under it
   * keeps `Tab` only while focus is inside it, so focus goes back there.
   */
  let before: HTMLElement | null = null;
  let stack = $state<HTMLElement | null>(null);

  function press(id: number, action: ToastAction | null) {
    action?.run();
    dismiss(id);
    const active = document.activeElement;
    if (before?.isConnected && (active === null || active === document.body || stack?.contains(active))) before.focus({ preventScroll: true });
    before = null;
  }
</script>

<div
  class="tf-toasts"
  class:tf-over-modal={modals.open > 0}
  role="status"
  aria-live="polite"
  bind:this={stack}
  onfocusin={(event) => {
    if (event.relatedTarget instanceof HTMLElement && !stack?.contains(event.relatedTarget)) before = event.relatedTarget;
  }}
>
  {#each toasts.items as toast (toast.id)}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="tf-toast {toast.tone === 'err' ? 'tf-err' : ''}"
      onmouseenter={() => hold(toast.id, "hover")}
      onmouseleave={() => release(toast.id, "hover")}
      onfocusin={() => hold(toast.id, "focus")}
      onfocusout={(event) => {
        if (!(event.relatedTarget instanceof Node && event.currentTarget.contains(event.relatedTarget))) release(toast.id, "focus");
      }}
    >
      <span><TfKeyed text={toast.text} /></span>
      {#each [...(toast.action ? [toast.action] : []), ...(toast.more ?? [])] as action, index (index)}
        <button onclick={() => press(toast.id, action)}>{action.label}</button>
      {/each}
      <button class="tf-x" aria-label="Dismiss" onclick={() => press(toast.id, null)}>✕</button>
    </div>
  {/each}
</div>
