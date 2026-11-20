<!--
  One selectable line of the launcher (#2421, spec #2419): three cells like a
  table. The kind's monogram in the state's tone, a key cell a person reads
  (`cells.ts` decides it per kind), and the title. No path, no state chip and
  no provenance: the state is the monogram's colour.

  **One second line, only where the name is no explanation** (the
  everything-search decisions, 2026-11-20, D10): a row *Everything* found by
  its text, and not by its name, says where under its title, the place and
  then the passage with the words marked (`whyOf` in `cells.ts`). It comes
  with the answer, so the row's height is known before it paints.

  Results, the board's recents and the sheet's pinned row all draw this, so
  "a row" is one thing everywhere.

  **Everything here is text.** `title` and `cell` are raw source text,
  whatever somebody typed into a ticket, `<script>` included (roadmap §4
  gotcha 7).
-->
<script lang="ts">
  import { kindTag } from "../assets/tree";
  import { aimed } from "./aim";
  import { chordLabel } from "../shell/platform";
  import { shortKey } from "../tf/tickets/tickets";
  import { menuThing, type Thing } from "../tf/context/thing";
  import type { Tone } from "../ipc/entity";
  import { markTitle } from "./marks";
  import type { Why } from "./cells";

  let {
    monogram,
    tone = null,
    kindName,
    cell,
    mono = false,
    title,
    marks = [],
    why = null,
    chord = null,
    source = null,
    id,
    selected,
    onopen,
    onhover,
    thing = () => null,
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
    /** The title's matched letters, pairs of start and end in UTF-16 code units of `title` (`marks.ts`). */
    marks?: readonly number[];
    /** Where the words are, for a row its name does not explain (D10): a second line under the title. */
    why?: Why | null;
    /** The chord bound to the row (#2429): a saved list's, which opens it from anywhere. */
    chord?: string | null;
    /** The source's name, where another row in the list has this row's key (#1476): two tickets, not one drawn twice. */
    source?: string | null;
    /** The option's element id, which the listbox names while it is active (#1411). */
    id: string;
    selected: boolean;
    onopen: () => void;
    onhover: () => void;
    /** What the row is to a right-click (2026-11-16), read when one comes: an entity's address names it. */
    thing?: () => Thing | null;
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
  {@attach menuThing(() => thing())}
>
  <span class="tf-ln-mg" data-tone={tone ?? undefined} title={kindName} aria-label={kindName}>{kindTag(monogram)}</span>
  <!-- A forge's issue key is told apart by its repository and number, which the cell's ellipsis cut: ten Gitea issues
       read *tidewater/pay…* on the live run (2026-11-09). Shortened as a ticket card shortens it, the whole on hover. -->
  <span class="tf-ln-k" class:tf-ln-mono={mono} title={mono && shortKey(cell) !== cell ? cell : undefined}>{mono ? shortKey(cell) : cell}</span>
  <span class="tf-ln-t" {@attach markTitle(() => ({ title, marks }))}>{title}</span>
  {#if source}<span class="tf-ln-src" title={source}>{source}</span>{/if}
  {#if chord}<kbd class="tf-ln-chord" title="Its key">{chordLabel(chord)}</kbd>{/if}
  {#if why}
    <!-- A real `<mark>`: the line's text changes with every answer, so there is no node for the Highlight API to keep. -->
    <span class="tf-ln-why" class:tf-ln-code={why.code}>
      {#if why.path}
        <!-- A path as `TfPath` draws one: the folders give way, the file's name stays whole, the whole path on hover. -->
        <span class="tf-ln-why-t tf-ln-why-path" title={why.segments.map((seg) => seg.text).join("")}
          >{#if why.path.head.length > 0}<span class="tf-path-head"
              >{#each why.path.head as seg, i (i)}{#if seg.hit}<mark>{seg.text}</mark>{:else}{seg.text}{/if}{/each}</span
            >{/if}<span class="tf-path-tail">{#each why.path.tail as seg, i (i)}{#if seg.hit}<mark>{seg.text}</mark>{:else}{seg.text}{/if}{/each}</span
          ></span
        >
      {:else}
        <span class="tf-ln-why-t"
          >{#if why.place}<span class="tf-ln-why-at">{why.place}</span>{/if}{#each why.segments as seg, i (i)}{#if seg.hit}<mark>{seg.text}</mark>{:else}{seg.text}{/if}{/each}</span
        >
      {/if}
      {#if why.count}<span class="tf-ln-why-n">{why.count}</span>{/if}
    </span>
  {/if}
</button>
