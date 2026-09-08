<!--
  The capture window (issue #503, spec #491 stories 40–43): the note editor in
  miniature, over whatever is in front.

  Everything with a decision in it is `capture.svelte.ts` — when the note is
  created, what its title is, what each exit does. What is here is the surface:
  one text area, one line of status, one button, and the two keys.

  **Its own document, not a route in the shell.** `capture.html` mounts this and
  nothing else — no context switcher, no health subscription, no timer, no inbox
  poll — because the shortcut's promise is that a thought costs one keystroke,
  and booting the whole frontend over somebody else's window to write two lines
  is not that. That is also why the styles here are local rather than the
  shell's tiles: this window has no chrome to match.
-->
<script lang="ts">
  import { onMount } from "svelte";

  import { createCapture, type CapturePorts } from "./capture.svelte";

  let {
    ports,
  }: {
    /**
     * The bridge, injectable so a test needs no Tauri — and `close` is one of
     * them rather than a prop of its own, so there is one door into this
     * window's IO and no way to override half of it.
     */
    ports?: Partial<CapturePorts>;
  } = $props();

  // svelte-ignore state_referenced_locally
  const capture = createCapture(ports);

  let box = $state<HTMLTextAreaElement | null>(null);

  // The window is opened by a keystroke and the next keystroke is the note, so
  // the caret has to be here before the reader can notice it was not.
  onMount(() => box?.focus());

  /**
   * Escape and ⌘Enter, which do the same thing: close with what was typed
   * saved. Ctrl+Enter beside ⌘Enter so the window behaves on the two platforms
   * knobas builds for and has not been run on.
   */
  function onKeydown(event: KeyboardEvent) {
    if (event.key === "Escape" || (event.key === "Enter" && (event.metaKey || event.ctrlKey))) {
      event.preventDefault();
      void capture.finish();
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<main class="cap">
  <textarea
    bind:this={box}
    class="inp"
    aria-label="Capture"
    placeholder="Type. Escape or ⌘Enter to keep it."
    value={capture.text}
    oninput={(event) => void capture.typed(event.currentTarget.value)}
  ></textarea>

  <div class="foot">
    {#if capture.failure}
      <p class="fail">{capture.failure}</p>
    {:else if capture.noteId}
      <p class="sub">Saved as a note.</p>
    {:else}
      <p class="sub">Nothing is kept until you type.</p>
    {/if}
    <button
      class="btn"
      aria-label="Open in knobas"
      disabled={capture.noteId === null || capture.busy}
      onclick={() => void capture.openInMain()}
    >
      Open in knobas
    </button>
  </div>
</main>

<style>
  .cap {
    display: grid;
    grid-template-rows: 1fr auto;
    gap: 8px;
    height: 100vh;
    box-sizing: border-box;
    padding: 10px;
    background: var(--bg);
    color: var(--text);
  }

  .inp {
    width: 100%;
    height: 100%;
    box-sizing: border-box;
    resize: none;
    font: inherit;
    padding: 8px;
    color: var(--text);
    background: var(--panel);
    border: 1px solid var(--hair2);
    border-radius: 2px;
  }

  .foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    font-size: 11px;
  }

  .sub {
    color: var(--muted);
  }

  .fail {
    color: var(--fail);
  }
</style>
