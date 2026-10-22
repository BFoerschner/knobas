/**
 * The decisions taken in the Proposed links view (#835, R-6.4), kept for the
 * session and written to the mirror once the toast's *Undo* has had its time.
 *
 * **Why the write waits.** *Undo* has to put a proposal back in the queue, and
 * no command can: an accepted proposal withdrawn by `unlink` is tombstoned, and
 * a dismissed one is remembered on purpose (`dismiss_suggestion`). So a
 * decision is held here for {@link UNDO_MS}, the toast's own life, and only
 * then sent; *Undo* inside that window cancels a write that never happened.
 * A window closed inside those seconds loses the decision, and the proposal
 * is simply there again next time (a ruling of #835).
 *
 * **What is written.** *Link them* is `accept_suggestion` when the relation is
 * the one proposed. A relation the reader changed is drawn by `create_link`
 * under that relation, and the proposal is then dismissed, because
 * `accept_suggestion` confirms the row as it stands and no command relabels a
 * link. The two are separate writes; a pair already linked under the chosen
 * relation counts as drawn, so a retry after a failed dismissal converges.
 * *Not related* is `dismiss_suggestion`. *Decide later* writes nothing:
 * the proposal waits in the mirror, and the session remembers the choice.
 *
 * Module-level, as `toasts.svelte.ts` is: decisions outlive the view, so
 * leaving it and coming back finds the queue as it was left.
 */
import { ipcErrorMessage, isIpcError } from "../../ipc";
import {
  acceptSuggestion,
  createLink,
  dismissSuggestion,
  type LinkRow,
  type SuggestionEntry,
} from "../../ipc/entity";
import type { Decided, Verdict } from "./review";

/** The commands a decision is written with. */
export interface DecisionPorts {
  acceptSuggestion: (linkId: string) => Promise<void>;
  dismissSuggestion: (linkId: string) => Promise<void>;
  createLink: (fromId: string, toId: string, relation?: string, note?: string) => Promise<LinkRow>;
}

/** How long *Undo* is offered, and so how long a write waits: the toast's life. */
export const UNDO_MS = 6500;

export interface Decisions {
  /** The verdict per link id this session. */
  readonly verdicts: Decided;
  /** Every proposal decided this session, so *Decided* lists one the mirror no longer returns. */
  readonly entries: readonly SuggestionEntry[];
  /**
   * Decide. `onfail` hears why a held write was refused; the decision is then
   * undone, so the proposal is back in the queue.
   */
  decide(entry: SuggestionEntry, verdict: Verdict, relation: string, onfail: (message: string) => void): void;
  /** Take a decision back. A write still held is cancelled. */
  undo(id: string): void;
  /** Tests only: cancel every held write and forget the verdicts and entries (#1743). */
  reset(): void;
}

export function createDecisions(
  // Wrapped, not referenced, so importing the shell touches no command until
  // a decision is written (a suite that mocks `ipc/entity` without these
  // still loads it), as `job/ports.ts` does.
  ports: DecisionPorts = {
    acceptSuggestion: (linkId) => acceptSuggestion(linkId),
    dismissSuggestion: (linkId) => dismissSuggestion(linkId),
    createLink: (fromId, toId, relation, note) => createLink(fromId, toId, relation, note),
  },
  delay: number = UNDO_MS,
): Decisions {
  const state = $state<{ verdicts: Decided; entries: SuggestionEntry[] }>({ verdicts: {}, entries: [] });
  const held = new Map<string, ReturnType<typeof setTimeout>>();

  async function write(entry: SuggestionEntry, verdict: Verdict, relation: string) {
    const { link } = entry;
    if (verdict === "no") return ports.dismissSuggestion(link.id);
    if (relation === link.relation) return ports.acceptSuggestion(link.id);
    // `conflict` is the pair already linked under that relation: the state this
    // write wants. It is what a retry meets when the dismissal below failed
    // after the link landed, so the retry finishes the job instead of sticking.
    try {
      await ports.createLink(link.from_id, link.to_id, relation);
    } catch (cause) {
      if (!isIpcError(cause) || cause.code !== "conflict") throw cause;
    }
    await ports.dismissSuggestion(link.id);
  }

  function undo(id: string) {
    const timer = held.get(id);
    if (timer !== undefined) clearTimeout(timer);
    held.delete(id);
    delete state.verdicts[id];
  }

  return {
    get verdicts() {
      return state.verdicts;
    },
    get entries() {
      return state.entries;
    },
    decide(entry, verdict, relation, onfail) {
      const id = entry.link.id;
      undo(id);
      state.verdicts[id] = verdict;
      if (!state.entries.some((known) => known.link.id === id)) state.entries.push(entry);
      if (verdict === "skip") return;
      const timer = setTimeout(() => {
        if (held.get(id) !== timer) return;
        held.delete(id);
        write(entry, verdict, relation).catch((cause: unknown) => {
          delete state.verdicts[id];
          onfail(`${verdict === "yes" ? "Could not link it" : "Could not dismiss it"}: ${ipcErrorMessage(cause)}`);
        });
      }, delay);
      held.set(id, timer);
    },
    undo,
    reset() {
      for (const timer of held.values()) clearTimeout(timer);
      held.clear();
      state.verdicts = {};
      state.entries = [];
    },
  };
}

/** The session's decisions. Tests build their own. */
export const decisions = createDecisions();
