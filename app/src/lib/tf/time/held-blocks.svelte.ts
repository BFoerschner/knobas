/**
 * **Blocks a press took and has not written yet** (#1429, F-339, F-356). A write with no reverse command (`dismiss_block`)
 * or one a block's marks can't come back from (`delete_block`): the block leaves the day at once and the write is held for
 * the toast's life, then sent, so *Undo* cancels a write that never happened and the block keeps its id, its context and
 * its worklog mark (the #835 pattern).
 *
 * Here and not in `TfDay`, because the toast outlives the view: a Day opened again during the toast has to hide what the
 * one before it hid. Closing the window inside those seconds sends nothing, and the block is simply there next time.
 */
import { ipcErrorMessage } from "../../ipc";
import { amend, heldWrite, push, toasts } from "../../shell/toasts.svelte";

interface Hold {
  id: number;
  /** The write has landed, so the next read, which no longer has the block, lets it go. */
  sent: boolean;
}

let held = $state<Hold[]>([]);

export function isHeld(id: number): boolean {
  return held.some((hold) => hold.id === id);
}

/** A read that began after those writes landed holds what they did, so the day no longer needs to hide them. */
export function forgetSent(): void {
  held = held.filter((hold) => !hold.sent);
}

/** Hide block `id` and send `write` when the toast saying `text` ends, unless its *Undo* was pressed. `after` runs once the write has answered. */
export function holdBlock(id: number, text: string, write: () => Promise<void>, after: () => unknown, ms: number): number {
  held = [...held, { id, sent: false }];
  const release = () => (held = held.filter((hold) => hold.id !== id));
  return heldWrite(
    text,
    () => {
      void write()
        .then(
          () => (held = held.map((hold) => (hold.id === id ? { ...hold, sent: true } : hold))),
          (error: unknown) => {
            release();
            push({ text: ipcErrorMessage(error), tone: "err" });
          },
        )
        .then(after);
    },
    release,
    { ms },
  );
}

/** The live toast of *Dismiss*es and how many rows it stands for. */
let dismissals: { toast: number; rows: number } | null = null;

/**
 * {@link holdBlock} for *Dismiss*. A press while the toast stands is that toast again, its one *Undo* taking back all of them,
 * so it says how many (F-365).
 */
export function holdDismissal(id: number, write: () => Promise<void>, after: () => unknown, ms: number): void {
  const counted = dismissals;
  const live = counted === null ? undefined : toasts.items.find((toast) => toast.id === counted.toast);
  const rows = counted !== null && live !== undefined ? counted.rows + 1 : 1;
  const text = rows === 1 ? "Dismissed. knobas will not offer it again." : `Dismissed ${rows} rows. knobas will not offer them again.`;
  // The words first, so the press below finds a toast already saying them and adds to it.
  if (live?.action !== undefined) amend(live.id, { text, action: live.action });
  dismissals = { toast: holdBlock(id, text, write, after, ms), rows };
}

/** Each test starts with nothing held. */
export function resetHeldBlocks(): void {
  held = [];
  dismissals = null;
}
