/**
 * The three strings a launcher row is made of that are not just a field.
 *
 * Kept out of the components because every one of them is a rule rather than a
 * layout: what "synced" reads as, when a source's health *replaces* the sync
 * age instead of sitting beside it, and what two letters stand for a source.
 * `Results.svelte` and `Board.svelte` both draw rows and they have to draw them
 * identically — that is the whole point of the board reusing the result row.
 */
import type { CredentialHealth } from "../ipc/sources";
import { isActionable } from "../shell/health.svelte";
import { sourceMonogram } from "../shell/monogram";
import { ago } from "../shell/time";

/**
 * `"synced 4 min ago"` — spec §4's per-row provenance.
 *
 * `synced_at` is knobas' own clock, so unlike a source's `updated_at` it is
 * always known and never null. `now` is injectable so the boundaries are
 * testable rather than waited for.
 */
export function syncAge(iso: string, now?: Date): string {
  return `synced ${ago(iso, now)}`;
}

/**
 * What to say about a source whose credential needs attention.
 *
 * Which states those are is `shell/health.svelte`'s {@link isActionable} — one
 * spelling, total over `AuthState`, because the row's complaint here, the
 * board's source strip and the chip's failure dot in `Chips.svelte` have to
 * agree about what a red reading means, and a second inline copy is a second
 * chance for them to drift the day a state is added (#37).
 */
const COMPLAINT: Record<string, string> = {
  unauthorized: "sign in again",
  unreachable: "unreachable",
  missing_secret: "no secret",
};

/**
 * The right-hand provenance line for one row: its sync age, or its source's
 * complaint when the source has one.
 *
 * Spec §4 pairs these deliberately — "synced 4 min ago" is worth nothing if
 * the reason it says 4 minutes is that the source has been refusing knobas'
 * credential ever since. The round-3 mockup drew the same pairing
 * (`sync failed …` against `synced …`).
 */
export function provenance(
  sourceId: string,
  syncedAt: string,
  sources: CredentialHealth[],
  now?: Date,
): { text: string; failing: boolean } {
  const health = sources.find((source) => source.source_id === sourceId);
  // `health &&` is not the call-site guard #72 folded into the rule. That one
  // was a second place deciding what "no reading yet" means; this one is a
  // `find()` result checked before `health.state` is read — twice, once here
  // and once in the branch body. `isActionable` answering `undefined` does not
  // remove it, it only respells it as `health?.state` and costs the narrowing
  // that `COMPLAINT[health.state]` below depends on.
  if (health && isActionable(health.state)) {
    return {
      text: `${sourceId} · ${COMPLAINT[health.state] ?? health.state}`,
      failing: true,
    };
  }
  return { text: syncAge(syncedAt, now), failing: false };
}

/**
 * Re-exported so `launcher/index.ts` keeps its shape; the rule itself lives in
 * `shell/monogram.ts`, because the top strip and the sources view draw it too.
 */
export { sourceMonogram };

/**
 * Re-exported so the strings a launcher row is made of stay reachable from one
 * place; the rule itself lives in `ancestors.ts` because the **detail view**
 * draws the same path out of the same payload, and this module pulls in the
 * credential-health store that a detail panel has no use for.
 */
export { ancestorPath } from "./ancestors";
