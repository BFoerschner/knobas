<!--
  What stopping the timer on a page, a note, a repo or an ad-hoc label opens
  (#281, spec #272 "Worklog draft and ad-hoc block").

  The block has nowhere to be logged, so knobas offers one ticket and **says
  which rule produced it**. Two ways out, and they are the whole surface:
  *Log to a ticket…* moves the block onto that ticket and hands the reader
  #280's worklog draft; *Keep local* closes and writes nothing at all.

  ## Why the reason is on screen

  A suggestion whose reason is hidden can only be trusted or ignored, and the
  three rules are not equally strong: a link the reader drew is nearly always
  right, while "the last ticket you logged to today" is a guess that is often
  right and sometimes badly wrong. The sentence is what lets them tell those
  apart in the second it takes to read it — so the backend hands over the
  *rule*, and the wording lives here, beside the layout it has to fit.

  ## Why *Keep local* is not a cancel

  It is the default when no rule fired, and it is a real answer even when one
  did: a block that stays local is still on the day review, still in the
  timesheet, still exportable. Nothing about closing this dialog loses the
  afternoon — what it declines is only sending it to Jira.
-->
<script lang="ts">
  import {
    updateBlock as realUpdateBlock,
    worklogDraft as realWorklogDraft,
    type AdHocBlock,
    type Block,
    type Draft,
  } from "../ipc/time";
  import Modal from "../shell/Modal.svelte";
  import { targetReading } from "../shell/timer";
  import { localDay, offsetMinutes } from "./draft";

  let {
    block,
    offer,
    onclose,
    onlog,
    updateBlock = realUpdateBlock,
    worklogDraft = realWorklogDraft,
  }: {
    /** The block that just closed. */
    block: Block;
    /** What the backend answered `ad_hoc_block` with, for this block. */
    offer: AdHocBlock;
    /** *Keep local*, Escape, and what a finished log does last. */
    onclose: () => void;
    /**
     * The draft to open, once the block has been moved onto the ticket.
     *
     * The shell renders it, for the reason the picker hands back a target
     * rather than opening one: a component that mounted the next dialog would
     * own a piece of the shell's stack.
     */
    onlog: (draft: Draft) => void;
    /** The bridge, injectable so a test needs no Tauri. */
    updateBlock?: typeof realUpdateBlock;
    worklogDraft?: typeof realWorklogDraft;
  } = $props();

  const suggestion = $derived(offer.suggestion);

  /** `PAY-231` — the half after the first colon, as everywhere else. */
  const key = $derived(
    suggestion ? suggestion.entity_id.slice(suggestion.entity_id.indexOf(":") + 1) : "",
  );

  /** What the block itself was on — a key, or the label as it was typed. */
  const on = $derived(targetReading(block.target));

  /**
   * **Why this ticket**, in one sentence per rule.
   *
   * The `switch` is exhaustive over `SuggestionRule`, so a rule added to the
   * backend fails `svelte-check` here rather than drawing a suggestion with no
   * reason under it.
   */
  const because = $derived.by(() => {
    switch (suggestion?.rule) {
      case "linked_to_target":
        return `You linked it to ${on}.`;
      case "context_anchor":
        return "It anchors the room this block ran in.";
      case "last_logged":
        return "It is the last ticket you logged time to on this day.";
      default:
        return "";
    }
  });

  /** `45m`, `2h 30m` — how long the block was. */
  const length = $derived.by(() => {
    const minutes = Math.round(
      (new Date(block.ended_at).getTime() - new Date(block.started_at).getTime()) / 60_000,
    );
    const whole = Math.floor(minutes / 60);
    const rest = minutes % 60;
    if (whole === 0) return `${rest}m`;
    return rest === 0 ? `${whole}h` : `${whole}h ${rest}m`;
  });

  let sending = $state(false);
  /** What went wrong, in the backend's own words. */
  let failed = $state<string | null>(null);

  /**
   * *Log to a ticket…*: move the block onto the ticket, then open the draft.
   *
   * **The re-target comes first, and it is the whole reason this is two
   * calls.** #280's draft is built from the day's unlogged blocks *on that
   * ticket* and re-derives them again when the reader logs — a draft opened
   * before the move would not contain this block, and the afternoon it is
   * about would be the one thing missing from it.
   *
   * The times are the block's own, unchanged: the reader is saying what this
   * stretch was *about*, not when it happened. Correcting the minutes is the
   * draft's own interval field, and after that the day review's.
   */
  function logToTicket() {
    if (!suggestion || sending) return;
    sending = true;
    failed = null;
    void updateBlock(block.id, block.started_at, block.ended_at, {
      kind: "entity",
      entity_id: suggestion.entity_id,
    })
      .then(() =>
        worklogDraft(suggestion.entity_id, {
          day: localDay(new Date(block.started_at)),
          offsetMinutes: offsetMinutes(),
        }),
      )
      .then((draft) => {
        if (!draft) {
          // The move happened; there is simply nothing to log. Said rather
          // than closed silently, because the block is on the ticket now and
          // a reader told nothing would believe neither had happened.
          failed = `This block is on ${key} now, but there is nothing left to log on it today.`;
          sending = false;
          return;
        }
        onlog(draft);
        onclose();
      })
      .catch((error: unknown) => {
        // Never swallowed: the block is still where it was, and a reader who
        // is not told will close this believing the time is on the ticket.
        failed = error instanceof Error ? error.message : String(error);
        sending = false;
      });
  }
</script>

<Modal title="Log an ad-hoc block" subtitle="{length} on {on}" {onclose}>
  {#snippet body()}
    {#if suggestion}
      <p class="lab">This time has no ticket. knobas suggests:</p>
      <div class="sug">
        <span class="k">{key}</span>
        <span class="t">{suggestion.title ?? suggestion.entity_id}</span>
        <span class="why">{because}</span>
      </div>
    {:else}
      <p class="lab">
        This time has no ticket, and knobas has nothing to suggest — no link, no
        room, and nothing logged today. Keep it local, or give it a ticket from
        the day review.
      </p>
    {/if}

    {#if failed}
      <p class="fail">{failed}</p>
    {/if}
  {/snippet}

  {#snippet footer()}
    <span class="spacer"></span>
    <button class="btn" class:pri={!suggestion} class:ghost={!!suggestion} type="button" onclick={onclose}>
      Keep local
    </button>
    {#if suggestion}
      <button class="btn pri" type="button" disabled={sending} onclick={logToTicket}>
        Log to {key}…
      </button>
    {/if}
  {/snippet}
</Modal>

<style>
  .sug {
    display: grid;
    gap: 3px;
    margin-top: 10px;
    padding: 10px;
    border: 1px solid var(--hair);
    border-radius: 3px;
    background: var(--raised);
  }

  .sug .k {
    font: 500 11px var(--mono);
    color: var(--faint);
    letter-spacing: 0.04em;
  }

  .sug .t {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .sug .why {
    color: var(--faint);
    font-size: 11px;
  }

  .fail {
    margin-top: 10px;
    color: var(--fail);
    font-size: 12px;
  }
</style>
