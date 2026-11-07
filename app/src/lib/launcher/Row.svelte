<!--
  One selectable line of the launcher (#2421, spec #2419): three cells like a
  table. The kind's monogram in the state's tone, a key cell a person reads
  (`cells.ts` decides it per kind), and the title. No second line, no
  excerpt, no path, no state chip and no provenance: the state is the
  monogram's colour, and what a second line said is in the title or not
  worth the row's height.

  Results, the board's recents and the sheet's pinned row all draw this, so
  "a row" is one thing everywhere.

  **Everything here is text.** `title` and `cell` are raw source text,
  whatever somebody typed into a ticket, `<script>` included (roadmap §4
  gotcha 7).
-->
<script lang="ts">
  import { kindTag } from "../assets/tree";
  import { aimed } from "./aim";
  import type { Tone } from "../ipc/entity";

  let {
    monogram,
    tone = null,
    kindName,
    cell,
    mono = false,
    title,
    chord = null,
    source = null,
    id,
    selected,
    onopen,
    onhover,
  }: {
    monogram: string;
    /** The state's tone, which colours the monogram; `null` for a row with no state. */
    tone?: Tone | null;
    /** What the monogram stands for, with the state's word when it has one: *Ticket: Done*. */
    kindName: string;
    /** The key cell. */
    cell: string;
    /** Whether the key cell is an identifier, drawn in the mono face. */
    mono?: boolean;
    title: string;
    /** The chord bound to the row (#2429): a saved list's, which opens it from anywhere. */
    chord?: string | null;
    /** The source's name, where another row in the list has this row's key (#1476): two tickets, not one drawn twice. */
    source?: string | null;
    /** The option's element id, which the listbox names while it is active (#1411). */
    id: string;
    selected: boolean;
    onopen: () => void;
    onhover: () => void;
  } = $props();
</script>

<button
  {id}
  class="tf-ln-res"
  class:tf-ln-on={selected}
  role="option"
  aria-selected={selected}
  onclick={onopen}
  onpointermove={aimed(onhover)}
>
  <span class="tf-ln-mg" data-tone={tone ?? undefined} title={kindName} aria-label={kindName}>{kindTag(monogram)}</span>
  <span class="tf-ln-k" class:tf-ln-mono={mono}>{cell}</span>
  <span class="tf-ln-t">{title}</span>
  {#if source}<span class="tf-ln-src" title={source}>{source}</span>{/if}
  {#if chord}<kbd class="tf-ln-chord" title="Its key">{chord}</kbd>{/if}
</button>
