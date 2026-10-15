/**
 * The registry of the new shell's pages (#908): every feature folder's
 * `page.ts`, found by a glob. What a page is, and how one is declared, is
 * `page-entry.ts`; that split keeps a `page.ts` from importing this module
 * while this module is still collecting it.
 */
import type { TfPageEntry } from "./page-entry";
import type { TfRoute } from "./route";

/**
 * Every feature's `page.ts`, in path order. Pages match disjoint routes, so
 * the order decides nothing; `pages.test.ts` holds that for every address it
 * draws.
 */
export const PAGES: readonly TfPageEntry[] = Object.entries(
  import.meta.glob<{ page: TfPageEntry }>("./*/page.ts", { eager: true }),
)
  .sort(([left], [right]) => left.localeCompare(right))
  .map(([, module]) => module.page);

/** The page that draws `route`, or `undefined` for the shell's own fallbacks. */
export function pageFor(route: TfRoute): TfPageEntry | undefined {
  return PAGES.find((page) => page.matches(route));
}
