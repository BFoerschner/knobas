/**
 * Where an item sits inside its source, as one line.
 *
 * A leaf on purpose: this module imports nothing. The detail panel draws this
 * string today; the launcher row is meant to draw the same one and cannot yet,
 * because a search hit carries no payload to read it out of -- see the PR for
 * #284. It is not in `format.ts`, the launcher's other row rules, because that
 * module reaches the credential-health store, which a detail panel has no
 * business loading to render a breadcrumb.
 */

/**
 * The ancestor path a page sits at, as one line — `Engineering › Payments`.
 *
 * ## Why this is a payload read, and how it fails (ADR-0007)
 *
 * Contract §4.1 normalizes four fields and an ancestor path is not one of
 * them, so the titles can only come out of the source's own record — which
 * makes this a **payload read outside an adapter**, and it lands under all
 * three of ADR-0007's requirements:
 *
 * 1. **It misses, never guesses.** Anything that is not an array of objects
 *    with non-blank string titles contributes nothing, and a record with no
 *    recognizable ancestors yields `null` — a row with no path, never a
 *    wrong one. A Jira ticket or a Gitea commit passed through here is
 *    exactly that case, which is why no caller has to ask what kind it holds.
 * 2. **One named function**, so a second source that nests its items is one
 *    more shape read here and nothing anywhere else.
 * 3. **The failure direction is absence**, pinned by
 *    `ancestors.test.ts`'s `a record with no readable
 *    ancestors has no path`
 *    and the four shapes beside it.
 *
 * ## Outermost first, and the space home kept
 *
 * Confluence answers `ancestors` outermost-first, which is the order a path
 * is read in, so the array is used as it came. The space's home page is the
 * first element and is deliberately **not** dropped: it is the space's own
 * name, and it is the segment that tells two identically-titled runbooks in
 * two spaces apart.
 */
export function ancestorPath(payload: unknown): string | null {
  if (payload === null || typeof payload !== "object") return null;
  const ancestors = (payload as { ancestors?: unknown }).ancestors;
  if (!Array.isArray(ancestors)) return null;
  const titles = ancestors
    .map((ancestor) =>
      ancestor !== null && typeof ancestor === "object"
        ? (ancestor as { title?: unknown }).title
        : null,
    )
    .filter(
      (title): title is string =>
        typeof title === "string" && title.trim() !== "",
    )
    .map((title) => title.trim());
  return titles.length > 0 ? titles.join(" › ") : null;
}
