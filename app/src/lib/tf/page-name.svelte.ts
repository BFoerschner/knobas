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
 *
 * A page that knows its own path also says its parent ({@link parentPage}): the
 * place its breadcrumb names above it, which an address alone cannot say (a pull
 * request's repository, a run's pipeline by name, an asset's holder). With no
 * opener `Esc` and *Back to <name>* go there (`escape.ts`).
 */
import { getContext, setContext } from "svelte";

const SHELL = Symbol("shown-page-name");
const PAGE = Symbol("page-name");

/** A place above a page: its address and what the breadcrumb calls it. */
export interface PageParent {
  hash: string;
  name: string;
}

/** One kept page's own name, `null` until it says one. */
interface Named {
  name: string | null;
  /** The place the page says it sits under; reactive, so the hint follows the read. */
  parent: PageParent | null;
  /** Which {@link namePage} call said it, so one that goes leaves another's name alone. */
  by: object | null;
}

/** What the shell reads: the shown page's own name, or `null`. */
export interface ShownPageName {
  readonly name: string | null;
  /** The place the shown page says it sits under, or `null` when the address decides. */
  readonly parent: PageParent | null;
}

/** A kept page's own: only the parent is reactive, because the shell's hint draws from it. */
class Own implements Named {
  name: string | null = null;
  parent = $state.raw<PageParent | null>(null);
  by: object | null = null;
}

class Shown implements ShownPageName {
  page = $state.raw<Named | null>(null);
  get name(): string | null {
    return this.page?.name ?? null;
  }
  get parent(): PageParent | null {
    return this.page?.parent ?? null;
  }
}

/** What a press on *Back to <name>* does: the shell's own `Esc` step. */
type BackStep = (hash: string, opened: boolean) => void;
const BACK = Symbol("back-step");

/** The shell's end of *Back to <name>*: a press goes where the key goes, the way the key goes. */
export function provideBackStep(step: BackStep): void {
  setContext(BACK, step);
}

/** The hint's end: the shell's step, or `null` outside a shell (a test), where the link is a plain link. */
export function backStep(): BackStep | null {
  return getContext<BackStep | undefined>(BACK) ?? null;
}

/** The shell's end: call during its init; the pages it draws report to it. */
export function provideShownPageName(): ShownPageName {
  const shown = new Shown();
  setContext(SHELL, shown);
  return shown;
}

/** A kept page's end: call during its init, with whether it is the page shown. */
export function providePageName(visible: () => boolean): void {
  const own = new Own();
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

/** The page's end of its parent: what its breadcrumb names above it now, or `null` while it does not know. */
export function parentPage(parent: () => PageParent | null): void {
  const own = getContext<Named | undefined>(PAGE);
  if (own === undefined) return;
  $effect(() => {
    own.parent = parent();
    return () => {
      own.parent = null;
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
