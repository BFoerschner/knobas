/**
 * Passwords (ADR-0056): `#/passwords`, `#/passwords/in/<folder>`, `#/passwords/source/<source>` and
 * `#/passwords/entry/<entry>`, one page, a view of the Passwords role whose frame draws its *Sync now*.
 */
import { definePage } from "../page-entry";
import { PASSWORDS } from "../roles.svelte";
import type { TfRoute } from "../route";
import TfPasswords from "./TfPasswords.svelte";

type PasswordsRoute = Extract<TfRoute, { view: "full" }> & { section: "passwords" };

export const page = definePage({
  matches: (route): route is PasswordsRoute => route.view === "full" && route.section === "passwords",
  role: PASSWORDS,
  component: TfPasswords,
  crumbs: true,
  knows: (route) => route.rest.length === 0 || (["in", "source", "entry"].includes(route.rest[0]!) && !!route.rest[1]),
  props: ({ route, ready, visible, timedTicket, layers }) => ({ ready, rest: route.rest, visible, timedTicket, layers }),
});
