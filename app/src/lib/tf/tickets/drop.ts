/**
 * Whether a column takes a held card, and why not (ticket-edit decisions, section 6): one pure function, so the
 * pointer, the keys, the menus and the tests cannot disagree about where a ticket can go. The board draws its answer
 * (`data-drop`), the carry walks it and the drop obeys it.
 *
 * The rules, first match wins; statuses compare trimmed and ignoring ASCII case, as the adapters do:
 *
 * 1. the column the card stands in: `own`, a drop puts it back;
 * 2. another lane: on a board laned by assignee or priority, the lane's value must be one the ticket's field takes
 *    (`lane`, section 26): `no` in the source's words where it is not, `ask` while that is not known; then the card's
 *    own status there takes it as is (the edit alone), and any other column answers by the rules below. A board laned
 *    by anything else: `no`, knobas does not change that;
 * 3. a source that declares no `transition`: `no`;
 * 4. a credential that may not move it: `no`, in the source's words;
 * 5. the *No status* column: `no`;
 * 6. nothing known of where it can go: `ask`, with the column's statuses (the source decides at the send);
 * 7. only the statuses its source was seen to use: `ask` with those of the column it uses, else `no`;
 * 8. the column's statuses the ticket can reach: `yes`, with them;
 * 9. none: `no`.
 */
import type { Reach } from "./reach.svelte";

export type Drop =
  /** The column the card stands in: a drop puts it back. */
  | { kind: "own" }
  /** The statuses of this column the ticket can reach, in the column's order. */
  | { kind: "yes"; statuses: string[] }
  /** Not known yet: a drop queues the move and the source decides. */
  | { kind: "ask"; statuses: string[] }
  | { kind: "no"; why: string };

export interface DropColumn {
  name: string;
  /** With no view, the column's own status; a view's own column, its statuses; a Jira view's, the listed ones. */
  statuses: readonly string[];
  /** The column the card stands in. */
  own: boolean;
  /** In the card's own lane (always, on a board without lanes). */
  sameLane: boolean;
  /** Its lane's key, `""` for the lane of tickets with none (and on a board without lanes). */
  lane?: string;
}

/** Whether a lane's value is one the ticket's field takes: known `yes`, not yet known `ask`, or `no` and why. */
export type LaneTakes = { kind: "yes" } | { kind: "ask" } | { kind: "no"; why: string };

export interface DropFacts {
  /** The source's display name. */
  source: string;
  /** Whether its adapter declares `transition`. */
  moves: boolean;
  /** `refused_writes`' reason for the move, or `null`. */
  refused: string | null;
  /** Another lane's answer, on a laned board (section 26); left out, another lane takes no drop. */
  lane?: (key: string) => LaneTakes;
}

/** How the adapters compare a status's name. */
export function sameStatus(a: string | null | undefined, b: string | null | undefined): boolean {
  return a != null && b != null && a.trim().toLowerCase() === b.trim().toLowerCase();
}

/** *A or B*, *A, B or C*. */
export function either(names: readonly string[]): string {
  if (names.length <= 1) return names[0] ?? "";
  return `${names.slice(0, -1).join(", ")} or ${names.at(-1)}`;
}

export function dropOn(ticket: { status: string | null }, column: DropColumn, reach: Reach | null, facts: DropFacts): Drop {
  if (column.own) return { kind: "own" };
  if (!column.sameLane) {
    const lane = facts.lane?.(column.lane ?? "") ?? { kind: "no", why: "A move changes the status. Drop it in its own lane." };
    if (lane.kind === "no") return lane;
    const own = column.statuses.find((status) => sameStatus(status, ticket.status));
    if (own !== undefined) return { kind: lane.kind, statuses: [own] };
    const there = dropOn(ticket, { ...column, sameLane: true }, reach, facts);
    return there.kind === "yes" && lane.kind === "ask" ? { kind: "ask", statuses: there.statuses } : there;
  }
  if (!facts.moves) return { kind: "no", why: `${facts.source} tickets cannot be moved from knobas: its adapter offers no moves.` };
  if (facts.refused !== null) return { kind: "no", why: facts.refused };
  const statuses = column.statuses.filter((status) => status.trim() !== "");
  if (statuses.length === 0) return { kind: "no", why: "A ticket cannot be moved to no status." };
  if (reach === null) return { kind: "ask", statuses: [...statuses] };
  const within = statuses.filter((status) => reach.statuses.some((known) => sameStatus(known, status)));
  if (reach.standing === "observed") {
    return within.length > 0 ? { kind: "ask", statuses: within } : { kind: "no", why: `${facts.source} has no status ${either(statuses)}.` };
  }
  if (within.length > 0) return { kind: "yes", statuses: within };
  return { kind: "no", why: `${facts.source} offers no move from ${ticket.status ?? "no status"} to ${either(statuses)}.` };
}
