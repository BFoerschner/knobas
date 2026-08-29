/**
 * How a suggestion reads on screen.
 *
 * The tray draws two things the reader judges a proposal by, and they are not
 * the same thing: the **reason** — which is data, produced by the detector and
 * stored with the row — and the **class**, which is how much that kind of
 * evidence is worth. This module owns only the second. The reason is never
 * assembled here: a suggestion whose reason cannot be shown is not shippable
 * (#41), and a reason rebuilt at display time from a rule id is one the next
 * surface can forget to build.
 */
import type { LinkRow } from "../ipc/entity";

/** The class, as a badge beside the reason. */
export interface ClassBadge {
  /** One word, lower case. Shown in the row. */
  label: string;
  /**
   * Whether this class is a guess rather than a statement.
   *
   * The tray marks these, because #41's story 16 is about *calibration*: the
   * point of showing the class at all is that a reader trusts an issue key in
   * a branch name and a shared vocabulary differently, and a row that looked
   * identical either way would make the distinction invisible.
   */
  speculative: boolean;
}

/**
 * What to call a proposal's evidence, or `null` when there is nothing to say.
 *
 * `null` is unreachable through the tray — the database refuses an unconfirmed
 * row with no class — but the wire type allows it, because a *confirmed* link
 * a person drew has no class at all. Rendering nothing is the honest answer to
 * that, and it is what keeps this function total.
 */
export function classBadge(ruleClass: LinkRow["rule_class"]): ClassBadge | null {
  switch (ruleClass) {
    case "exact_key":
      // The two ends name each other. Nothing was inferred.
      return { label: "exact", speculative: false };
    case "source_relation":
      // The source system already states this relation; knobas is repeating it,
      // not guessing it — and still asking.
      return { label: "recorded", speculative: false };
    case "similarity":
      return { label: "guess", speculative: true };
    default:
      return null;
  }
}

/**
 * The heading's count, as a word rather than a bare number.
 *
 * #41's story 19 is "see how many suggestions are waiting, so that I can choose
 * when to spend attention on them" — which is a question about *whether to
 * look*, so the answer has to read as one at a glance.
 */
export function waitingLabel(total: number): string {
  if (total === 0) return "none waiting";
  return total === 1 ? "1 waiting" : `${total} waiting`;
}
