/**
 * Secrets (ADR-0056, #2515): `#/secrets`, `#/secrets/in/<folder>`, `#/secrets/source/<source>` and
 * `#/secrets/entry/<entry>`, one page, a view of the `passwords` role whose frame draws its *Sync now*.
 */
import { definePage } from "../page-entry";
import { PASSWORDS } from "../roles.svelte";
import type { TfRoute } from "../route";
import TfPasswords from "./TfPasswords.svelte";

type SecretsRoute = Extract<TfRoute, { view: "full" }> & { section: "secrets" };

export const page = definePage({
  matches: (route): route is SecretsRoute => route.view === "full" && route.section === "secrets",
  role: PASSWORDS,
  component: TfPasswords,
  crumbs: true,
  knows: (route) => route.rest.length === 0 || (["in", "source", "entry"].includes(route.rest[0]!) && !!route.rest[1]),
  props: ({ route, ready, visible, timedTicket, layers }) => ({ ready, rest: route.rest, visible, timedTicket, layers }),
});
