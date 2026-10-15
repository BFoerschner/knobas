<!--
  Toasts in the ticket-first shell (#809, R-1.8): the same stack the rooms UI
  draws (`shell/toasts.svelte.ts`), so every `push` anywhere in the app lands
  here, drawn as prototype-6 draws one: ink on the page, the words of the
  button that caused it, and at most its actions as underlined words.

  Text only: a toast often carries a source's own error message (gotcha 7).
-->
<script lang="ts">
  import { dismiss, toasts } from "../shell/toasts.svelte";
</script>

<div class="tf-toasts" role="status" aria-live="polite">
  {#each toasts.items as toast (toast.id)}
    <div class="tf-toast {toast.tone === 'err' ? 'tf-err' : ''}">
      <span>{toast.text}</span>
      {#each [...(toast.action ? [toast.action] : []), ...(toast.more ?? [])] as action (action.label)}
        <button
          onclick={() => {
            action.run();
            dismiss(toast.id);
          }}>{action.label}</button
        >
      {/each}
      <button class="tf-x" aria-label="Dismiss" onclick={() => dismiss(toast.id)}>✕</button>
    </div>
  {/each}
</div>
