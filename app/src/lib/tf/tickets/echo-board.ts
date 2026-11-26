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

/**
 * A ticket's field edit drawn over the board's read (ticket-edit decisions, section 26), as a move is: the new
 * assignee, priority, summary or labels on its card at once. On a board laned by the field it changed (assignee or
 * priority), the card leaves its lane's cell for the same status's cell of the lane of its new value (a lane of its
 * own at the board's end where none is drawn), and both lanes' counts move by one. Lay it over the status echoes, so
 * a card moved and reassigned stands in its new column of its new lane.
 *
 * Answers the same object when no echo touches the board.
 */
export interface FieldEchoed {
  /** The assignee's account, `""` for nobody; its name as it reads beside it. */
  assignee?: { account: string; name: string };
  priority?: string;
  title?: string;
  /** Labels added (`+`) and removed (`-`), oldest first. */
  labels?: string[];
}

/** An entity's field echoes, or `null` where it has none. */
export type FieldsOf = (entityId: string) => FieldEchoed | null;

/** Which field a board's lanes are by, as the read names it. */
export type LanedBy = "epic" | "assignee" | "priority" | "issuetype" | "label" | null;

/** `card` with its echoed fields drawn, or the same card where they change nothing. */
export function withFields(card: MiniBoardCard, echoed: FieldEchoed | null): MiniBoardCard {
  if (echoed === null) return card;
  let drawn = card;
  const set = (patch: Partial<MiniBoardCard>) => (drawn = { ...drawn, ...patch });
  const { assignee, priority, title, labels } = echoed;
  if (assignee !== undefined && (assignee.account || null) !== card.assignee) {
    const { assignee_name: _name, ...rest } = drawn;
    drawn = assignee.account === "" ? { ...rest, assignee: null } : { ...rest, assignee: assignee.account, assignee_name: assignee.name };
  }
  if (priority !== undefined && !sameStatus(priority, card.priority)) set({ priority });
  if (title !== undefined && title !== card.title) set({ title });
  if (labels !== undefined && labels.length > 0) {
    const now = [...card.labels];
    for (const one of labels) {
      const name = one.slice(1);
      const at = now.indexOf(name);
      if (one.startsWith("+") && at < 0) now.push(name);
      if (one.startsWith("-") && at >= 0) now.splice(at, 1);
    }
    if (now.length !== card.labels.length || now.some((name, i) => name !== card.labels[i])) set({ labels: now });
  }
  return drawn;
}

/** The lane key a card's value stands in: the assignee's account or the priority, `""` for none. */
function laneKeyOf(card: MiniBoardCard, by: LanedBy): string | null {
  if (by === "assignee") return card.assignee ?? "";
  if (by === "priority") return card.priority ?? "";
  return null;
}

export function withFieldEchoes(board: MiniBoard, fieldsOf: FieldsOf, by: LanedBy = null): MiniBoard {
  const relaned = new Map<string, string>();
  let touched = false;
  const columns = board.columns.map((column) => {
    let changed = false;
    const cards = column.cards.map((card) => {
      const drawn = withFields(card, fieldsOf(card.entity_id));
      if (drawn === card) return card;
      changed = true;
      const was = laneKeyOf(card, by);
      const now = laneKeyOf(drawn, by);
      if (was !== null && now !== null && was !== now) relaned.set(card.entity_id, now);
      return drawn;
    });
    if (!changed) return column;
    touched = true;
    return { ...column, cards };
  });
  if (!touched) return board;
  if (relaned.size === 0 || board.lanes.length === 0) return { ...board, columns };
  const leaving = new Map<string, string | null>();
  const lanes = board.lanes.map((lane) => {
    let gone = 0;
    const cells = lane.columns.map((cell) => {
      const ids = cell.ids.filter((id) => !relaned.has(id) || relaned.get(id) === lane.key);
      if (ids.length === cell.ids.length) return cell;
      for (const id of cell.ids) if (!ids.includes(id)) leaving.set(id, cell.status);
      gone += cell.ids.length - ids.length;
      return { ...cell, ids, total: Math.max(0, cell.total - (cell.ids.length - ids.length)) };
    });
    return gone === 0 ? lane : { ...lane, count: Math.max(0, lane.count - gone), columns: cells };
  });
  for (const [id, status] of leaving) {
    const key = relaned.get(id)!;
    let at = lanes.findIndex((lane) => lane.key === key);
    if (at < 0) {
      lanes.push({ key, count: 0, columns: [] });
      at = lanes.length - 1;
    }
    const lane = lanes[at]!;
    const cells = [...lane.columns];
    let cell = cells.findIndex((one) => one.status === status);
    if (cell < 0) {
      // In the board's column order, as a lane's cells are.
      const rank = (of: string | null) => {
        const at = board.columns.findIndex((column) => column.status === of);
        return at < 0 ? board.columns.length : at;
      };
      cell = cells.findIndex((one) => rank(one.status) > rank(status));
      if (cell < 0) cell = cells.length;
      cells.splice(cell, 0, { status, ids: [], total: 0 });
    }
    const one = cells[cell]!;
    cells[cell] = { ...one, ids: [id, ...one.ids], total: one.total + 1 };
    lanes[at] = { ...lane, count: lane.count + 1, columns: cells };
  }
  return { ...board, columns, lanes };
}

/** The list's rows with each echoed field drawn; the same array when no echo touches them. */
export function withFieldRows(rows: MiniBoardCard[], fieldsOf: FieldsOf): MiniBoardCard[] {
  let changed = false;
  const drawn = rows.map((card) => {
    const one = withFields(card, fieldsOf(card.entity_id));
    if (one !== card) changed = true;
    return one;
  });
  return changed ? drawn : rows;
}
