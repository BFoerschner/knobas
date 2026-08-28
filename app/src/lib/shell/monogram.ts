/**
 * A source's two-letter monogram — `jira` → `JI`, `teamcity` → `TE`.
 *
 * From the source **id**, which is what knobas has everywhere it draws one:
 * `CredentialHealth` carries no display name (interfaces §2.2), and a
 * per-adapter table of pretty monograms is exactly what spec §3a forbids ("a
 * new ticket system is browsable on day one").
 *
 * So it is the first two letters and nothing cleverer. The mockup hand-wrote
 * `TC` for TeamCity; this returns `TE`, and the difference is the point — the
 * mockup knew six sources by name and knobas may not know any of them.
 *
 * **In `shell/` rather than in `launcher/format.ts`, where it started**, and
 * spelled once. Four places draw a monogram for a source — the launcher's
 * board strip, the top strip's health cluster, the sources view's rows and the
 * add-source dialog's adapter tiles — and three of them had inlined their own
 * `.slice(0, 2).toUpperCase()`, with three different answers for an empty
 * string and, in the sources view, over the *adapter kind* rather than the id.
 * That last one is the reason this is worth one module: with `jira` and
 * `tidewater-jira` both configured, the top strip drew `JI` and `TI` while the
 * sources view drew `JI` for both — the same two sources, told apart in one
 * place and not the other.
 */
export function sourceMonogram(sourceId: string): string {
  return sourceId.slice(0, 2).toUpperCase() || "?";
}
