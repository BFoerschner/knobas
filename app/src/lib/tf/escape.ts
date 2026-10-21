/**
 * The ticket-first shell's `Esc` (#809, R-14): one layer per press, in this
 * order.
 *
 * 1. a dialog is open        → close the topmost dialog;
 * 2. a drawer is open        → close it;
 * 3. a filter field has text → clear it, focus stays;
 * 4. a text field has focus  → blur it;
 * 5. a job's sub-tab is open → back to the job's first tab;
 * 6. a full view is open     → back to the job you were last in, or to
 *    Tickets when there is none;
 * 7. a job is open           → back to Tickets;
 * 8. on Tickets              → nothing.
 *
 * Rung 8 is the rooms ladder's rung 5 again: `Esc` never leaves the app or
 * throws anything away, and a key that sometimes does nothing is what makes
 * the others safe to press.
 *
 * Rung 6 without a job goes to Tickets. Prototype-6 does nothing there, and
 * its breadcrumb still says *Back* beside `Esc`; a key the page names should
 * do what it names, and Tickets is where the ladder ends anyway.
 *
 * Pure: the shell describes what is open and acts on the answer. Every dialog
 * registers on the layer stack (`TfDialog`, and the rooms UI's `Modal` the
 * asset tree still draws, #1527), so rung 1 sees it even when focus is
 * outside it. `Modal` also answers an `Esc` pressed inside it and stops the
 * event, so that press never reaches the shell.
 */
import { jobHash, type TfRoute } from "./route";

/** What `Esc` can see when it is pressed. */
export interface EscState {
  /** Open dialogs and drawers, oldest first. */
  layers: readonly { kind: "dialog" | "drawer" }[];
  /** The focused text field, or `null` when focus is not in one. */
  field: { filter: boolean; text: string } | null;
  route: TfRoute;
  /** The address of the job the reader was last in (its key, or its entity id, #1525), if any. */
  jobAt: string | null;
}

/** What one press does. */
export type EscStep =
  | { do: "close"; index: number }
  | { do: "clear-filter" }
  | { do: "blur" }
  | { do: "go"; hash: string }
  | { do: "nothing" };

export function escStep(state: EscState): EscStep {
  for (const kind of ["dialog", "drawer"] as const) {
    const index = state.layers.map((layer) => layer.kind).lastIndexOf(kind);
    if (index !== -1) return { do: "close", index };
  }
  if (state.field) {
    if (state.field.filter && state.field.text !== "") return { do: "clear-filter" };
    return { do: "blur" };
  }
  const route = state.route;
  if (route.view === "job") {
    if (route.tab !== null) return { do: "go", hash: jobHash(route.key) };
    return { do: "go", hash: "#/tickets" };
  }
  if (route.view === "tickets") return { do: "nothing" };
  return { do: "go", hash: state.jobAt === null ? "#/tickets" : jobHash(state.jobAt) };
}
