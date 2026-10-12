<!--
  A text block (ADR-0020): a source's prose -- a commit message, a pull
  request's description. Drawn as text even where the source writes Markdown
  (`markdown: true`), by the rule `NoteBody` follows: interpolate, never render
  (rule 5, gotcha 7).
-->
<script lang="ts">
  let { text }: { text: string; markdown: boolean } = $props();
</script>

<p class="tb">{text}</p>

<style>
  .tb {
    font-size: 12.5px;
    line-height: 1.55;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
</style>
