/**
 * The shown page's own name (spec #2227): what its breadcrumb calls the page
 * itself, which the shell stamps as the opener's name when the reader leaves
 * it, so *Back to <name>* names the page `Esc` returns to (`escape.ts`'s
 * `openerName`).
 *
 * A page says its name; nothing reads it off the DOM. Three ends:
 *
 * - the shell ({@link provideShownPageName}) holds which page is shown and
 *   reads its name as the address moves, before the page it left is hidden;
 * - each kept page (`TfKeptPage.svelte`, {@link providePageName}) holds its
 *   page's name and tells the shell while it is the one shown;
 * - the page names itself ({@link namePage}): `TfCrumbs` with its last part
 *   when that is the page's own (unlinked) name, and a page that draws its
 *   breadcrumb by hand (a repository, a pull request, a doc page) with what
 *   it draws there.
 *
 * Outside a shell or a kept page (a test mounting one page alone) each end
 * does nothing.
 */
import { getContext, setContext } from "svelte";

const SHELL = Symbol("shown-page-name");
const PAGE = Symbol("page-name");

/** One kept page's own name, `null` until it says one. */
interface Named {
  name: string | null;
  /** Which {@link namePage} call said it, so one that goes leaves another's name alone. */
  by: object | null;
}

/** What the shell reads: the shown page's own name, or `null`. */
export interface ShownPageName {
  readonly name: string | null;
}

class Shown implements ShownPageName {
  page: Named | null = null;
  get name(): string | null {
    return this.page?.name ?? null;
  }
}

/** The shell's end: call during its init; the pages it draws report to it. */
export function provideShownPageName(): ShownPageName {
  const shown = new Shown();
  setContext(SHELL, shown);
  return shown;
}

/** A kept page's end: call during its init, with whether it is the page shown. */
export function providePageName(visible: () => boolean): void {
  const own: Named = { name: null, by: null };
  setContext(PAGE, own);
  const shown = getContext<Shown | undefined>(SHELL);
  if (shown === undefined) return;
  $effect(() => {
    if (!visible()) return;
    shown.page = own;
    return () => {
      // Another page may have been shown in the same flush: leave its name alone.
      if (shown.page === own) shown.page = null;
    };
  });
}

/** The page's end: `name` is what the page calls itself now, or `null` while it does not know. */
export function namePage(name: () => string | null): void {
  const own = getContext<Named | undefined>(PAGE);
  if (own === undefined) return;
  const me = {};
  $effect(() => {
    own.name = name();
    own.by = me;
    return () => {
      if (own.by !== me) return;
      own.name = null;
      own.by = null;
    };
  });
}
