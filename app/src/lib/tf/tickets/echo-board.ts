/**
 * A ticket's move drawn over the board's read (ticket-edit decisions, section 11): the **echo** (CONTEXT.md) laid over
 * every board answer before it is drawn, so every reader of the board agrees -- the columns, the lanes, their counts,
 * the list -- and a read that lands between the move and the refresh changes nothing drawn.
 *
 * For each card with a status echo: its status becomes the echo's, it leaves its column for the column of that status
 * (at the top, newest first, as the source will sort it), and both columns' totals move by one; laned, its lane's
 * cells move the same way. With a view's own columns the card stays in the board's status columns, which the view's
 * columns then gather. A status no column of the board holds is a column of its own at the board's end, which a view's
 * column gathers and, without one, draws last, past every column drawn.
 *
 * Answers the same object when no echo touches the board, so nothing draws again.
 */
import type { MiniBoard, MiniBoardCard, MiniBoardColumn } from "../../ipc/entity";
import { sameStatus } from "./drop";

/** The echoed status of an entity, or `null` where it has none. */
export type StatusOf = (entityId: string) => string | null;

export function withStatusEchoes(board: MiniBoard, statusOf: StatusOf): MiniBoard {
  const moves = new Map<string, { card: MiniBoardCard; to: string }>();
  for (const column of board.columns) {
    for (const card of column.cards) {
      const to = statusOf(card.entity_id);
      if (to !== null && !sameStatus(to, card.status)) moves.set(card.entity_id, { card, to });
    }
  }
  if (moves.size === 0) return board;
  const columns: MiniBoardColumn[] = board.columns.map((column) => {
    const kept = column.cards.filter((card) => !moves.has(card.entity_id));
    return kept.length === column.cards.length ? column : { ...column, cards: kept, total: Math.max(0, column.total - (column.cards.length - kept.length)) };
  });
  const statusAt = new Map<string, string | null>();
  for (const { card, to } of moves.values()) {
    let at = columns.findIndex((column) => sameStatus(column.status, to));
    if (at < 0) {
      columns.push({ status: to, cards: [], total: 0 });
      at = columns.length - 1;
    }
    const target = columns[at]!;
    columns[at] = { ...target, cards: [{ ...card, status: target.status }, ...target.cards], total: target.total + 1 };
    statusAt.set(card.entity_id, target.status);
  }
  if (board.lanes.length === 0) return { ...board, columns };
  const lanes = board.lanes.map((lane) => {
    let touched = false;
    const cells = lane.columns.map((cell) => {
      const ids = cell.ids.filter((id) => !moves.has(id));
      if (ids.length === cell.ids.length) return cell;
      touched = true;
      return { ...cell, ids, total: Math.max(0, cell.total - (cell.ids.length - ids.length)) };
    });
    const moved = lane.columns.flatMap((cell) => cell.ids.filter((id) => moves.has(id)));
    if (!touched) return lane;
    for (const id of moved) {
      const to = statusAt.get(id) ?? null;
      let at = cells.findIndex((cell) => cell.status === to);
      if (at < 0) {
        cells.push({ status: to, ids: [], total: 0 });
        at = cells.length - 1;
      }
      const cell = cells[at]!;
      cells[at] = { ...cell, ids: [id, ...cell.ids], total: cell.total + 1 };
    }
    return { ...lane, columns: cells };
  });
  return { ...board, columns, lanes };
}

/** The list's rows with each echoed status drawn in its card; the same array when no echo touches them. */
export function withStatusRows(rows: MiniBoardCard[], statusOf: StatusOf): MiniBoardCard[] {
  let changed = false;
  const drawn = rows.map((card) => {
    const to = statusOf(card.entity_id);
    if (to === null || sameStatus(to, card.status)) return card;
    changed = true;
    return { ...card, status: to };
  });
  return changed ? drawn : rows;
}
