<!--
  A command line on one line that never runs past its box (the renderers-columns pass, 2026-11-20; design system, *Paths*).
  Its program reads as a path does in `TfPath`: the program's start gives way first, all of it if need be, and its file
  stays; then its arguments, in the muted ink, are cut at their end. The program's file is cut only when it alone is wider
  than the box. The whole command is its title, and all of it is in the text, quotes too, so a selection copies it as typed.
-->
<script lang="ts">
  import { commandParts } from "./path";

  let { command, class: className = "" }: { command: string; class?: string } = $props();

  const parts = $derived(commandParts(command));
</script>

<span class="tf-cmd tf-mono {className}" title={command}
  >{#if parts.head !== ""}<span class="tf-cmd-head">{parts.head}</span>{/if}<span class="tf-cmd-file">{parts.tail}</span
  >{#if parts.rest !== ""}<span class="tf-cmd-args">{" "}{parts.rest}</span>{/if}</span
>
