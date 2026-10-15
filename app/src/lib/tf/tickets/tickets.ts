/**
 * The Tickets view's rules (#810, requirements §3), pure: what a column
 * draws, what a card's corner says, what the preview's button reads, and
 * what *Around this ticket* counts.
 */
import type { EntityPage, MiniBoardCard, MiniBoardColumn } from "../../ipc/entity";
import { kindLabel, kindSingular, sectionOf } from "../../shell/kinds";

/** How many cards a column draws before *Show N more* (R-3.4). */
export const COLUMN_CAP = 8;
/** How many a *Done* column draws (R-3.4). */
export const DONE_CAP = 3;

/** Whether a column is the *Done* one, however its source cases it. */
export function isDone(status: string | null): boolean {
  return status !== null && status.toLowerCase() === "done";
}

/** What a column's header says: its status in the source's words, or *No status*. */
export function columnName(status: string | null): string {
  return status ?? "No status";
}

/**
 * The cards a column draws (R-3.4): up to eight, three for *Done*, all of
 * them once expanded, and every match while the filter has words (the read
 * is uncapped then).
 */
export function drawnCards(column: MiniBoardColumn, expanded: boolean, typing: boolean): MiniBoardCard[] {
  if (expanded || typing) return column.cards;
  return column.cards.slice(0, isDone(column.status) ? DONE_CAP : COLUMN_CAP);
}

/** What the column's *Show N more* button says, or `null` when it draws every card. */
export function moreLine(column: MiniBoardColumn, drawn: number): string | null {
  const rest = column.total - drawn;
  if (rest <= 0) return null;
  return isDone(column.status) ? `Show ${rest} more done` : `Show ${rest} more`;
}

/**
 * The initials in a card's corner: the first letters of an account's first
 * two parts (`mara.lindqvist` is *ML*), or its first two letters, or *–* for
 * nobody.
 */
export function initials(assignee: string | null): string {
  if (assignee === null) return "–";
  const parts = assignee.split(/[.\s_@-]+/).filter(Boolean);
  if (parts.length >= 2) return (parts[0]![0]! + parts[1]![0]!).toUpperCase();
  return (parts[0] ?? assignee).slice(0, 2).toUpperCase();
}

/**
 * When a ticket changed, as prototype-6 writes it: *today 11:48*,
 * *yesterday 16:05*, else the date. *never* for a source that never said.
 */
export function when(iso: string | null, now: Date = new Date()): string {
  if (iso === null) return "never";
  const at = new Date(iso);
  const day = (d: Date) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
  const time = `${pad(at.getHours())}:${pad(at.getMinutes())}`;
  if (day(at) === day(now)) return `today ${time}`;
  const yesterday = new Date(now);
  yesterday.setDate(now.getDate() - 1);
  if (day(at) === day(yesterday)) return `yesterday ${time}`;
  return day(at);
}

function pad(n: number): string {
  return String(n).padStart(2, "0");
}

/**
 * The preview's one primary button and the line under it (R-3.9), by the
 * ticket's state as prototype-6 decides it: *Start work* on a To Do ticket of
 * mine, *Open job* on a done one or somebody else's, *Resume work* on the
 * rest of mine. Where it goes is the job's address; #816 and #818 wire what
 * happens there.
 */
export function previewAction(
  card: Pick<MiniBoardCard, "assignee" | "mine">,
  status: string | null,
  timerOnIt: boolean,
): { label: string; note: string } {
  const lower = status?.toLowerCase() ?? "";
  if (lower === "to do" && card.mine) {
    return {
      label: "Start work",
      note: "Creates a branch and PR, moves it to In Progress, starts the timer. You review each step.",
    };
  }
  if (lower === "done") return { label: "Open job", note: "Done. Opening shows what was attached." };
  if (!card.mine) {
    return { label: "Open job", note: card.assignee ? `Assigned to ${card.assignee}.` : "Unassigned." };
  }
  return {
    label: "Resume work",
    note: timerOnIt ? "Your timer is running on this ticket." : "Starts the timer on this ticket.",
  };
}

/** One line of *Around this ticket*: a section and what it holds. */
export interface AroundLine {
  section: string;
  /** "7 builds" */
  count: string;
  /** "1 failing or down", or `null` when nothing is. */
  failing: string | null;
}

/**
 * *Around this ticket* (R-3.9): one line per section of what the ticket's
 * context holds, from the tally a `latest_per_parent` room read counts — so
 * a build is its configuration's latest — grouped the way the room's tiles
 * group kinds (`sectionOf`). Tickets are the view's own and not a section
 * here. A section of one kind says that kind's plural; a mixed one (Code)
 * says *items*. Tone `err` is what fails or is down.
 */
export function aroundLines(tally: EntityPage["tally"]): AroundLine[] {
  const sections = new Map<string, { label: string; order: number; kinds: Set<string>; n: number; bad: number }>();
  for (const line of tally ?? []) {
    if (line.kind === "ticket" || line.count === 0) continue;
    const section = sectionOf(line.kind);
    const held = sections.get(section.id) ?? { label: section.label, order: section.order, kinds: new Set(), n: 0, bad: 0 };
    held.kinds.add(line.kind);
    held.n += line.count;
    if (line.tone === "err") held.bad += line.count;
    sections.set(section.id, held);
  }
  return [...sections.values()]
    .sort((a, b) => a.order - b.order)
    .map((held) => {
      const only = held.kinds.size === 1 ? [...held.kinds][0]! : null;
      const one = held.n === 1;
      const noun = (
        only === null ? (one ? "item" : "items") : one ? kindSingular(only) : kindLabel(only)
      ).toLowerCase();
      return {
        section: held.label,
        count: `${held.n} ${noun}`,
        failing: held.bad > 0 ? `${held.bad} failing or down` : null,
      };
    });
}
