/**
 * The controls specimen (#2280): `#/controls`, in a dev build only.
 *
 * The design system's Controls section names every button size and
 * arrangement; this page draws one of each with the real stylesheet, as the
 * visual reference a UI ticket is held to (`just shot <slot> <dir> '#/controls'`).
 * It is no section of the app: the address is one the shell otherwise calls
 * unknown, and a release build, where `import.meta.env.DEV` folds to `false`,
 * matches it to nothing and drops the specimen with the branch that loads it
 * (`TfControlsPage.svelte`).
 */
import { definePage } from "../page-entry";
import type { TfRoute } from "../route";
import TfControlsPage from "./TfControlsPage.svelte";

type UnknownRoute = Extract<TfRoute, { view: "unknown" }>;

/** The specimen's address. */
export const CONTROLS_HASH = "#/controls";

export const page = definePage({
  matches: (route): route is UnknownRoute =>
    import.meta.env.DEV && route.view === "unknown" && route.hash === CONTROLS_HASH,
  component: TfControlsPage,
  props: () => ({}),
});
