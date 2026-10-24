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
  did: a block that stays local is still on the Day tab, still in the
  timesheet, still exportable. Nothing about closing this dialog loses the
  afternoon — what it declines is only sending it to Jira.

  It draws as a ticket-first dialog (`TfDialog`, #1432), mounted in the shell's
  `dialogs` so `.tf`'s rules reach it, and is rung 1 of the `Esc` ladder.
-->
<script lang="ts">
  import { minutesLabel } from "./format";
  import {
    updateBlock as realUpdateBlock,
    worklogDraft as realWorklogDraft,
    type AdHocBlock,
    type Block,
    type Draft,
  } from "../ipc/time";
  import type { SuggestionRule } from "../ipc/time";
  import { targetReading } from "../shell/timer";
  import { layers as sharedLayers, type Layers } from "../tf/layers.svelte";
  import TfDialog from "../tf/settings/TfDialog.svelte";
  import { minutesBetween, readerDay } from "./day";
  import { localDay } from "./draft";

  let {
    block,
    offer,
    onclose,
    onlog,
    updateBlock = realUpdateBlock,
    worklogDraft = realWorklogDraft,
    layers = sharedLayers,
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
    /** The shell's layer stack, which `Esc` closes from; a test builds its own. */
    layers?: Layers;
  } = $props();

  const suggestion = $derived(offer.suggestion);

  /** `PAY-231` — the suggestion read the way every timer target is. */
  const key = $derived(suggestion ? targetReading({ kind: "entity", entity_id: suggestion.entity_id }) : "");

  /** What the block itself was on — a key, or the label as it was typed. */
  const on = $derived(targetReading(block.target));

  /**
   * **Why this ticket**, in one sentence per rule.
   *
   * The `switch` is exhaustive **because the `default` arm assigns the rule to
   * `never`**: a fourth rule added to the backend fails `svelte-check` right
   * here, rather than reaching this dialog as a suggestion with no reason under
   * it. A `default: return ""` would compile for ever and draw exactly that.
   */
  function reasonFor(rule: SuggestionRule): string {
    switch (rule) {
      case "linked_to_target":
        return `You linked it to ${on}.`;
      // The job's ticket, which is what the reader was in: the context behind
      // a job is its hidden working set and is never named (#1427).
      case "context_anchor":
        return `The timer was started while you were in ${key}.`;
      case "last_logged":
        return "It is the last ticket you logged time to on this day.";
      default: {
        const unhandled: never = rule;
        throw new Error(`no reason is written for the ${String(unhandled)} rule`);
      }
    }
  }

  const because = $derived(suggestion ? reasonFor(suggestion.rule) : "");

  /** `45m`, `2h 15m`: how long the block was, as every other page writes a length. */
  const length = $derived(minutesLabel(minutesBetween(block.started_at, block.ended_at)));

  let sending = $state(false);
  /**
   * Why the reader is still looking at this dialog: a refusal in the backend's
   * own words, or the one non-failure that leaves them here — a block that
   * moved with nothing left to log on it.
   */
  let problem = $state<string | null>(null);

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
   * draft's own interval field, and after that the Day tab's.
   */
  function logToTicket() {
    if (!suggestion || sending) return;
    sending = true;
    problem = null;
    void updateBlock(block.id, block.started_at, block.ended_at, {
      kind: "entity",
      entity_id: suggestion.entity_id,
    })
      .then(() =>
        worklogDraft(suggestion.entity_id, readerDay(localDay(new Date(block.started_at)))),
      )
      .then((draft) => {
        if (!draft) {
          // The move happened; there is simply nothing to log. Said rather
          // than closed silently, because the block is on the ticket now and
          // a reader told nothing would believe neither had happened.
          problem = `This block is on ${key} now, but there is nothing left to log on it today.`;
          sending = false;
          return;
        }
        onlog(draft);
        onclose();
      })
      .catch((error: unknown) => {
        // Never swallowed: the block is still where it was, and a reader who
        // is not told will close this believing the time is on the ticket.
        problem = error instanceof Error ? error.message : String(error);
        sending = false;
      });
  }
</script>

<TfDialog title="Log an ad-hoc block" subtitle="{length} on {on}" {onclose} {layers}>
  {#snippet body()}
    {#if suggestion}
      <p>This time has no ticket. knobas suggests:</p>
      <div class="tf-tm-sug">
        <span class="tf-mono tf-dim">{key}</span>
        <span class="tf-tm-sug-t">{suggestion.title ?? key}</span>
        <span class="tf-dim tf-tm-sug-why">{because}</span>
      </div>
    {:else}
      <p>This time has no ticket, and knobas has nothing to suggest. Keep it local and assign one later on the Day tab.</p>
    {/if}

    {#if problem}
      <p class="tf-fail" role="alert">{problem}</p>
    {/if}
  {/snippet}

  {#snippet footer()}
    <button
      type="button"
      class="tf-btn"
      class:tf-primary={!suggestion}
      class:tf-ghost={!!suggestion}
      onclick={onclose}
    >
      Keep local
    </button>
    {#if suggestion}
      <button type="button" class="tf-btn tf-primary" disabled={sending} onclick={logToTicket}>
        Log to {key}…
      </button>
    {/if}
  {/snippet}
</TfDialog>

