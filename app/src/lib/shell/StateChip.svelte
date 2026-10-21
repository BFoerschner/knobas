<!--
  An item's state, as the adapter declared it (#666): a pull request's
  `open` / `merged` / `closed`, a build's `running` or `FAILURE`.

  The shell's chip (`.tf-chip` and its tone classes, `tf/css/chrome.css`),
  its colour the adapter's tone (#1422). The tone is the adapter's word for what the value means; which
  colour that is stays in the shell.

  `text` is source text (gotcha 7): interpolated, never rendered as markup.
-->
<script lang="ts">
  import type { StateChip } from "../ipc/entity";

  let { state }: { state: StateChip } = $props();
</script>

<span class="tf-chip tf-state tf-{state.tone}" data-tone={state.tone}>{state.text}</span>
