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
 * A source's two-letter monogram — `jira` → `JI`, `teamcity` → `TE`.
 *
 * From the source **id**, which is what knobas has: `CredentialHealth` carries
 * no display name (interfaces §2.2), and a per-adapter table of pretty
 * monograms is exactly what spec §3a forbids ("a new ticket system is
 * browsable on day one").
 *
 * So it is the first two letters and nothing cleverer. The mockup hand-wrote
 * `TC` for TeamCity; this returns `TE`, and the difference is the point — the
 * mockup knew six sources by name and knobas may not know any of them. An
 * earlier draft of this comment claimed `TC`, which is how a doc lies: the
 * test below is what caught it.
 */
export function sourceMonogram(sourceId: string): string {
  return sourceId.slice(0, 2).toUpperCase() || "?";
}

/**
 * The states that mean *a human has to do something*.
 *
 * **`unknown` is deliberately not one of them**, and this is a correction to
 * the plan's "replaced by the source's health when it is not `ok`". `unknown`
 * is migration 0002's default: it means nothing has tested the credential yet,
 * which is the state every source is in until stream F's scheduler runs. On
 * that reading every row in a fresh install would show a health complaint in
 * place of its sync age, which is both wrong and the loudest possible way to
 * be wrong.
 */
const ACTIONABLE = new Set(["unauthorized", "unreachable", "missing_secret"]);

/** What to say about a source whose credential needs attention. */
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
  if (health && ACTIONABLE.has(health.state)) {
    return { text: `${sourceId} · ${COMPLAINT[health.state] ?? health.state}`, failing: true };
  }
  return { text: syncAge(syncedAt, now), failing: false };
}
