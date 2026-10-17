/**
 * `mini_board`'s `lanes` (#939), over the fixture: a whole board put in
 * lanes after it is read. A restatement of `knobas_core::mini_board::in_lanes`,
 * which is the authority, as the rest of the fake board is.
 */
import type { LaneColumn, LaneField, MiniBoardCard, MiniBoardLane, SourceStatuses } from "../../../ipc/entity";
import { laneOf } from "../../../tf/tickets/tickets";

type Card = Pick<
  MiniBoardCard,
  "entity_id" | "updated_at" | "path" | "assignee" | "priority" | "issue_type" | "labels"
>;
interface Board<C extends Card> {
  columns: { status: string | null; cards: C[]; total: number }[];
  sources: SourceStatuses[];
}

/**
 * Each lane's column keeps its first `perColumn` cards in the column's
 * order and counts them all; lanes rank biggest first, a tie to the lane
 * with the newer ticket, then by key; only the first `first` are kept. The
 * columns keep their totals and the kept cards.
 */
export function fakeLanes<C extends Card>(
  whole: Board<C>,
  by: LaneField,
  perColumn: number | null,
  first: number | null,
): Board<C> & { lanes: MiniBoardLane[]; more_lanes: number } {
  const lanes = new Map<string, { count: number; newest: string; columns: LaneColumn[] }>();
  for (const column of whole.columns) {
    for (const card of column.cards) {
      const key = laneOf(card, by);
      const lane = lanes.get(key) ?? { count: 0, newest: "", columns: [] };
      lanes.set(key, lane);
      lane.count += 1;
      if ((card.updated_at ?? "") > lane.newest) lane.newest = card.updated_at ?? "";
      let cell = lane.columns.at(-1);
      if (cell === undefined || cell.status !== column.status) {
        cell = { status: column.status, ids: [], total: 0 };
        lane.columns.push(cell);
      }
      cell.total += 1;
      if (perColumn === null || cell.ids.length < perColumn) cell.ids.push(card.entity_id);
    }
  }
  const ranked = [...lanes]
    .sort(([aKey, a], [bKey, b]) => b.count - a.count || b.newest.localeCompare(a.newest) || aKey.localeCompare(bKey))
    .map(([key, lane]): MiniBoardLane => ({ key, count: lane.count, columns: lane.columns }));
  const sent = first === null ? ranked : ranked.slice(0, Math.max(1, first));
  const kept = new Set(sent.flatMap((lane) => lane.columns.flatMap((cell) => cell.ids)));
  return {
    columns: whole.columns.map((column) => ({
      ...column,
      cards: column.cards.filter((card) => kept.has(card.entity_id)),
    })),
    sources: whole.sources,
    lanes: sent,
    more_lanes: ranked.length - sent.length,
  };
}
