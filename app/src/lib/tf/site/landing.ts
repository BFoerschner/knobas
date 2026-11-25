/**
 * What a site's *Back to* calls the page `Esc` twice lands on (Fable's ruling, 2026-11-25): its title as knobas drew
 * it, the Docs tree's or a list's row, else its row's own read (`head`), a ticket by its key as `openerName` calls one,
 * and the kind's plain name until one of them is known.
 */
import type { UrlMatch } from "../../ipc/entity";
import { heldRow } from "../../ipc/answer-cache";
import { kindSingular } from "../../shell/kinds";
import { heldTree } from "../docs/tree-store";
import { segmentKey } from "../job/job";

/** The kind's plain name, which stands until the item's own is known. */
export function kindName(kind: string): string {
  return kindSingular(kind);
}

/** The name the site's bar gives the page `match` lands on; `head` is its row's title where that was read. */
export function landingName(match: UrlMatch, head: { id: string; title: string } | null): string {
  if (match.kind === "ticket") return segmentKey(match.entity_id);
  if (head !== null && head.id === match.entity_id) return head.title;
  if (match.kind === "page") {
    for (const space of heldTree.tree?.spaces ?? []) {
      const node = space.pages.find((one) => one.entity_id === match.entity_id);
      if (node !== undefined) return node.title;
    }
  }
  return heldRow(match.entity_id)?.title ?? kindName(match.kind);
}
