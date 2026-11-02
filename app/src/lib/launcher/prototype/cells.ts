/**
 * PROTOTYPE: the key column of a row (round 9): an identifier where the
 * kind has one a person reads, in the mono face; otherwise the kind's word.
 */
import { prNumber } from "../../tf/code/code";
import { keyOf } from "../../time/format";

export function cellOf(kind: string, entityId: string, kindWord: string): { cell: string; mono: boolean } {
  switch (kind) {
    case "ticket":
      return { cell: keyOf(entityId), mono: true };
    case "pr":
      return { cell: prNumber(entityId) ?? keyOf(entityId), mono: true };
    case "inbox":
      return { cell: keyOf(entityId).replace(/ \(\d+\)$/, ""), mono: true };
    default:
      return { cell: kindWord.toLowerCase(), mono: false };
  }
}
