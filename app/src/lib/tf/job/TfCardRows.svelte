<!--
  The sections table (#2401, `card-sections.ts`), as the ticket page's card and the
  Tickets preview's *Around this ticket* both draw it (F-109), so one ticket reads the
  same in both: a bordered list, one row a section with its count, its worst state and
  a chevron, opening the section's view narrowed to the ticket.
-->
<script lang="ts">
  import { stateWord } from "../state-word";
  import { toneClass } from "../tone";
  import { pressSection, type CardSection } from "./card-sections";

  let {
    rows,
    entityId,
    ticketKey,
  }: {
    rows: readonly CardSection[];
    /** The ticket a press narrows the section to. */
    entityId: string;
    /** What a row's title calls the ticket. */
    ticketKey: string;
  } = $props();
</script>

<div class="tf-tktable tf-tksecs">
  {#each rows as row (row.name)}
    <a
      href={row.href}
      title={row.opens === "subtasks" ? `Tickets under ${ticketKey}. ${row.words}` : `${row.name} view, this ticket only. ${row.words}`}
      onclick={() => pressSection(row, entityId)}
      ><span class="tf-tkname">{row.name}</span><span class="tf-n">{row.count}</span>{#if row.state !== null}<span
          class="tf-st"
          class:tf-asks={row.state.tone === "err" || row.state.tone === "warn"}
          ><span class="tf-dot {toneClass(row.state)}" aria-hidden="true"></span>{stateWord(row.state.text)}</span
        >{:else if row.note !== undefined}<span class="tf-st">{row.note}</span>{:else}<span></span>{/if}<span class="tf-chev" aria-hidden="true">›</span></a
    >
  {/each}
</div>
