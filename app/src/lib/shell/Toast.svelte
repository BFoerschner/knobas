<!--
  The toast stack, rendered into the mockup's `#toasts` corner.

  Text only: a toast routinely carries an `IpcError.message`, which is a string
  a source system produced (gotcha 7). It is interpolated, never parsed.
-->
<script lang="ts">
  import { dismiss, toasts } from "./toasts.svelte";
</script>

<div id="toasts" role="status" aria-live="polite">
  {#each toasts.items as toast (toast.id)}
    <div class="toast {toast.tone === 'err' ? 'err' : ''}">
      <span>{toast.text}</span>
      {#if toast.action}
        <button
          class="btn sm"
          onclick={() => {
            toast.action?.run();
            dismiss(toast.id);
          }}>{toast.action.label}</button
        >
      {/if}
      <button class="x" aria-label="Dismiss" onclick={() => dismiss(toast.id)}>
        <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4 4l8 8M12 4l-8 8" /></svg>
      </button>
    </div>
  {/each}
</div>
