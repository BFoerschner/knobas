<!--
  A command line on one line that never runs past its box (the renderers-columns pass, 2026-11-20; design system, *Paths*).
  Its program reads as a path does in `TfPath`: the program's start gives way first, all of it if need be, and its file
  stays; then its arguments, in the muted ink, are cut at their end. The program's file is cut only when it alone is wider
  than the box. The whole command is its title, and all of it is in the text, quotes too, so a selection copies it as typed.

  `keepEnd` (#2518, an *Open in…* line and Settings › Checkouts' table): the arguments keep the folder or file their last
  path ends in (`argsParts`), which says which checkout or file the command opens: what follows it gives way first, cut
  at its end, then what comes before it, down to an ellipsis, and only then the program's folders. Arguments with no path
  in them are cut at their end, as without it.
-->
<script lang="ts">
  import { argsParts, commandParts } from "./path";

  let { command, class: className = "", keepEnd = false }: { command: string; class?: string; keepEnd?: boolean } = $props();

  const parts = $derived(commandParts(command));
  const args = $derived(keepEnd ? argsParts(parts.rest) : { head: parts.rest, tail: "", after: "" });
</script>

<span class="tf-cmd tf-mono {className}" class:tf-cmd-ends={keepEnd} title={command}
  >{#if parts.head !== ""}<span class="tf-cmd-head">{parts.head}</span>{/if}<span class="tf-cmd-file">{parts.tail}</span
  >{#if args.head !== ""}<span class="tf-cmd-args">{" "}{args.head}</span>{/if}{#if args.tail !== ""}<span class="tf-cmd-end">{args.tail}</span>{/if}{#if args.after !== ""}<span
      class="tf-cmd-after">{args.after}</span
    >{/if}</span
>
