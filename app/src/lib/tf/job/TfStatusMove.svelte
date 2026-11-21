<!--
  The ticket page's status, as a control (ticket-edit decisions, section 13; ADR-0054). Björn: "inside the ticket, you
  can just change the status on the field after clicking them". The chip as it reads and a small chevron: a press, or
  `m` anywhere on the page outside a field, opens a menu of the statuses the ticket can reach from where it stands, in
  the source's order. A pick moves it (`moveTicket`): the chip shows the new status at once, with *Moving, queued*
  then *Moved, sent* beside it, gone when the page's read shows the status.

  - Opened before the source answered, the menu says *Asking ⟨source⟩…* and takes its items when they land.
  - Nowhere to go: one disabled line, *⟨Source⟩ offers no move from ⟨status⟩.*
  - A failed read: the statuses the source was seen to use under *⟨Source⟩ did not answer; it decides when the move is
    sent.*; with none, that line and *Try again*.
  - A source that offers no moves, or a credential that may not: the chip plain, with the reason as its title. The
    chevron's room is held, unseen, so nothing moves when the grants answer.
  - A refused or held move: the status goes back, and a note stands under the row, full width, in the source's words,
    with *Open write queue* and `×`.
-->
<script lang="ts">
  import StatusMenu from "../tickets/StatusMenu.svelte";
  import TfStateChip from "../TfStateChip.svelte";
  import type { Echo } from "../queued.svelte";
  import type { Reach } from "../tickets/reach.svelte";
  import type { TicketNote } from "../tickets/moves.svelte";
  import { openWriteQueue } from "../../shell/write-queue.svelte";

  let {
    status,
    ticketKey,
    source,
    reach,
    asking = false,
    moves,
    echo = null,
    note = null,
    open = $bindable(false),
    onmove,
    onretry,
    ondismiss,
  }: {
    /** The status the page's read says. */
    status: string | null;
    /** The ticket's key, which names the menu. */
    ticketKey: string;
    /** The source's display name. */
    source: string;
    /** What the window holds of where the ticket can go. */
    reach: Reach | null;
    /** Whether a read of it is out. */
    asking?: boolean;
    /** Whether a move can be offered at all, and why not. */
    moves: { can: boolean; why: string | null };
    /** The move's echo, drawn in place of the read's status while it stands. */
    echo?: Echo | null;
    note?: TicketNote | null;
    open?: boolean;
    onmove: (status: string) => void;
    /** *Try again* after a failed read. */
    onretry: () => void;
    /** The note's `×`: its write discarded. */
    ondismiss: () => void;
  } = $props();

  let button = $state<HTMLButtonElement | null>(null);

  const shown = $derived(echo?.text ?? status);
  const sent = $derived(echo?.state === "sent");

</script>

<button
  type="button"
  class="tf-statebtn"
  bind:this={button}
  aria-haspopup={moves.can ? "menu" : undefined}
  aria-expanded={moves.can ? open : undefined}
  aria-disabled={moves.can ? undefined : "true"}
  aria-label={moves.can ? `Status: ${shown ?? "No status"}. Change it (M)` : undefined}
  title={moves.can ? "Change the status (M)" : (moves.why ?? undefined)}
  onclick={() => {
    if (moves.can) open = !open;
  }}
  ><TfStateChip text={shown ?? "No status"} /><span class="tf-chev" aria-hidden="true"></span></button
>
<!-- Both words stacked, the one that stands shown: the chip keeps the longer one's width, so nothing moves as it turns *sent*. -->
{#if echo !== null}<span class="tf-chip tf-swrite" class:tf-info={!sent}
    ><span class="tf-wwords"><span class:tf-won={!sent} aria-hidden={sent ? "true" : undefined}>Moving, queued</span><span class:tf-won={sent} aria-hidden={sent ? undefined : "true"}>Moved, sent</span></span></span
  >{/if}
{#if note}
  <p class="tf-fact-note" class:tf-held={note.tone === "warn"} role="status">
    <span>{note.lead} {note.words}</span>
    <button type="button" onclick={openWriteQueue}>Open write queue</button>
    <button type="button" aria-label="Discard the move" title="Discard the move" onclick={ondismiss}>×</button>
  </p>
{/if}
<StatusMenu bind:open opener={button} ticketKey={ticketKey} status={shown} {source} {reach} {asking} onpick={onmove} {onretry} />
