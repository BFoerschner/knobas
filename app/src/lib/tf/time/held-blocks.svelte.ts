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
import { heldWrite, push } from "../../shell/toasts.svelte";

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
export function holdBlock(id: number, text: string, write: () => Promise<void>, after: () => unknown, ms: number): void {
  held = [...held, { id, sent: false }];
  const release = () => (held = held.filter((hold) => hold.id !== id));
  heldWrite(
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

/** Each test starts with nothing held. */
export function resetHeldBlocks(): void {
  held = [];
}
