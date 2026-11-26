/**
 * What a ticket card's *Assets* row marks in the asset tree (`job/scoped.svelte.ts`):
 * the assets the ticket reaches, each with what holds it, so the columns can mark
 * the way to one that is not open yet. Read again when the row's ticket changes;
 * nothing while no row opened the tree.
 */
import { getAsset } from "../../ipc/assets";
import { jobReach } from "../../ipc/entity";
import { keyOf } from "../job/job";

export interface TicketAsset {
  id: string;
  name: string;
  holders: string[];
}

/** The marks the tree's columns draw, by asset id. */
export interface TicketMarks {
  assets: ReadonlySet<string>;
  holders: ReadonlySet<string>;
  key: string;
}

export function ticketAssets(ticket: () => string | null) {
  let reached = $state.raw<TicketAsset[]>([]);
  $effect(() => {
    const id = ticket();
    reached = [];
    if (id === null) return;
    let live = true;
    void jobReach(id)
      .then((rows) => Promise.allSettled(rows.filter((row) => row.kind === "asset").map((row) => getAsset(row.entity_id))))
      .then((reads) => {
        if (!live) return;
        reached = reads.flatMap((read) =>
          read.status === "fulfilled" ? [{ id: read.value.asset.id, name: read.value.asset.name, holders: read.value.held_by.map((holder) => holder.id) }] : [],
        );
      })
      .catch(() => {});
    return () => {
      live = false;
    };
  });
  return {
    get list(): readonly TicketAsset[] {
      return reached;
    },
    get marks(): TicketMarks | undefined {
      const id = ticket();
      if (id === null) return undefined;
      return { assets: new Set(reached.map((asset) => asset.id)), holders: new Set(reached.flatMap((asset) => asset.holders)), key: keyOf(id) };
    },
  };
}
