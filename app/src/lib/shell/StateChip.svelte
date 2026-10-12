<!--
  An item's state, as the adapter declared it (#666): a pull request's
  `open` / `merged` / `closed`, a build's `running` or `FAILURE`.

  Quiet by construction, as the Signal sheet asks: mono text in a hairline
  frame, the tone carried by the text and its frame and never by a fill —
  except `err`, which is the one state a reader is scanning a room for and
  gets a faint wash of its colour. The tone is the adapter's word for what the
  value means; which colour that is stays here, in the shell.

  `text` is source text (gotcha 7): interpolated, never rendered as markup.
-->
<script lang="ts">
  import type { StateChip } from "../ipc/entity";

  let { state }: { state: StateChip } = $props();
</script>

<span class="st" data-tone={state.tone}>{state.text}</span>

<style>
  .st {
    display: inline-flex;
    align-items: center;
    flex: none;
    height: 16px;
    padding: 0 5px;
    border: 1px solid color-mix(in srgb, currentColor 45%, transparent);
    border-radius: 2px;
    font: 500 10px/1 var(--mono);
    letter-spacing: 0.02em;
    white-space: nowrap;
    vertical-align: 1px;
    color: var(--muted);
  }
  .st[data-tone="ok"] {
    color: var(--ok);
  }
  .st[data-tone="warn"] {
    color: var(--amber);
  }
  .st[data-tone="err"] {
    color: var(--fail);
    background: color-mix(in srgb, var(--fail) 10%, transparent);
  }
  .st[data-tone="info"] {
    color: var(--link);
  }
  .st[data-tone="muted"] {
    color: var(--faint);
    border-color: var(--hair2);
  }
</style>
