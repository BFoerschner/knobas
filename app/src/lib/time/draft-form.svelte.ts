/**
 * The worklog draft's form: what is ticked, whether the comment is the
 * reader's, and what is sent (#280).
 *
 * One class for both drafts, the rooms UI's `WorklogDraft.svelte` and the new
 * shell's `TfWorklogDraft.svelte` (#826), so the rules below are written once
 * while the two shells live side by side. The reasoning for each rule is in
 * `WorklogDraft.svelte`'s header.
 *
 * **Read once, at construction.** A draft is one afternoon and a dialog is
 * opened for one; a draft swapped mid-life would silently discard what the
 * reader had ticked. The shell closes and reopens.
 */
import type { Draft, LoggedWork } from "../ipc/time";
import { atClock, clockOf } from "./draft";
import { keyOf, minutesLabel } from "./format";

export class DraftForm {
  readonly draft: Draft;

  /** The ticked candidates, by id. Reassigned, never mutated, so it re-renders. */
  ticked = $state(new Set<string>());
  /** Whether the reader has typed in the comment, which the boxes then leave alone. */
  edited = $state(false);
  typed = $state("");
  /** Minutes, because that is the unit a person corrects a clock in. */
  minutes = $state(0);
  /** When the work began, as the time field shows it. */
  began = $state("");

  constructor(draft: Draft) {
    this.draft = draft;
    this.ticked = new Set(draft.candidates.map((c) => c.id));
    this.typed = draft.comment;
    this.minutes = Math.max(0, Math.floor(draft.seconds / 60));
    this.began = clockOf(draft.started_at);
  }

  /** An untouched field sends the block's own start, to the second. */
  get startedAt(): string {
    return this.began === clockOf(this.draft.started_at)
      ? this.draft.started_at
      : atClock(this.draft.started_at, this.began);
  }

  /** The comment as the ticks make it: the backend's bullets, joined. */
  get generated(): string {
    return this.draft.candidates
      .filter((c) => this.ticked.has(c.id))
      .map((c) => c.bullet)
      .join("\n");
  }

  get comment(): string {
    return this.edited ? this.typed : this.generated;
  }

  /**
   * Whole minutes, in seconds: what the button says is what is sent, floored
   * from the blocks' sum and never rounded up (CONTEXT.md, *Timesheet*). The
   * leftover seconds stay unlogged.
   */
  get seconds(): number {
    return Math.max(0, Math.floor(this.minutes)) * 60;
  }

  /** `2h 30m`, `45m`: the reading on the button. */
  get reading(): string {
    return minutesLabel(this.minutes);
  }

  /** `PAY-231`: the half after the first colon, as everywhere else. */
  get key(): string {
    return keyOf(this.draft.entity_id);
  }

  toggle(id: string) {
    const next = new Set(this.ticked);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    this.ticked = next;
  }

  /** The reader typed in the comment: it is theirs from now on. */
  takeOver(text: string) {
    this.edited = true;
    this.typed = text;
  }

  /** What `log_work` is sent. */
  get work(): LoggedWork {
    return { startedAt: this.startedAt, seconds: this.seconds, comment: this.comment };
  }
}
