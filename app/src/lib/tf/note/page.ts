/**
 * A note's read-only page (#1504): `#/note/<note id>`, while notes are shown
 * (D10). It lights no nav entry and draws the shell's breadcrumb itself, as
 * the item page does.
 */
import { definePage } from "../page-entry";
import type { TfRoute } from "../route";
import TfNote from "./TfNote.svelte";

type NoteRoute = Extract<TfRoute, { view: "note" }>;

export const page = definePage({
  matches: (route): route is NoteRoute => route.view === "note",
  component: TfNote,
  address: (route) => route.id,
  props: ({ route, ready, job, layers }) => ({ id: route.id, ready, job, layers }),
});
