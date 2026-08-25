/**
 * `>` — the navigation half of spec §4's action palette.
 *
 * §4 gives `>` two jobs: *"do it here"* rows that act on the selected result
 * (start a timer, comment, change status) and, "with an empty query, the app's
 * navigation menu". The first needs write-back (M2) and the timer (M3) and is
 * out of M1 entirely — reserving rows for it would be dead chrome. The second
 * is a list of addresses, and M1 has three.
 */
import { hashFor } from "../shell/router.svelte";

/** One row of the `>` palette. */
export interface LauncherAction {
  /** Stable id, used as the list key. */
  id: string;
  label: string;
  /** The second line: what pressing it does. */
  detail: string;
  /** Where it goes. Every M1 action is a navigation. */
  hash: string;
}

/**
 * The addresses M1 actually has (`shell/router.svelte.ts`).
 *
 * **Two of the plan's four are deliberately absent, and both for the same
 * reason — they are not addresses.**
 *
 * *Diagnostics* has no route: interfaces §2.3 puts the sync log inside the
 * sources view, so `#/diagnostics` parses as `unknown` and would render "this
 * arrives in a later milestone". An action that navigates to an apology is
 * worse than no action.
 *
 * *Reload demo data* is a **write** (`demo_load`), refused outside the demo
 * profile (ruling P13), and the launcher does not know which profile it is in
 * — `AppStatus.demo` is stream D's. It belongs in the `actions` prop, which
 * exists so the shell can append what only the shell knows.
 */
export function navigationActions(): LauncherAction[] {
  return [
    {
      id: "room-all",
      label: "All work",
      detail: "The room over every configured source",
      hash: hashFor({ view: "room", ctx: "all", detail: null }),
    },
    {
      id: "sources",
      label: "Sources",
      detail: "Add, edit and sync the systems knobas reads",
      hash: hashFor({ view: "sources" }),
    },
    {
      id: "first-run",
      label: "First-run setup",
      detail: "The guided walk through connecting a source",
      hash: hashFor({ view: "first-run" }),
    },
  ];
}

/** The actions matching `text`, in the order they were declared. */
export function filterActions(actions: LauncherAction[], text: string): LauncherAction[] {
  const needle = text.trim().toLowerCase();
  if (!needle) return actions;
  return actions.filter(
    (action) =>
      action.label.toLowerCase().includes(needle) ||
      action.detail.toLowerCase().includes(needle),
  );
}
