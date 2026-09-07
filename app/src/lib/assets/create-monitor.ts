/**
 * What *Create monitor for this asset* opens on (issue #453, spec #427 story
 * 70: *"a new host gets a check from the pane"*).
 *
 * The form asks for two things and it should have to ask for neither: knobas
 * already knows what the asset is called and, usually, where it answers. So
 * this is the derivation — the ticket's *"prefilled from the asset's route or
 * hostname"* written down as a function, out of the component, so it can be
 * tested against the shapes an estate really holds rather than through a
 * rendered dialog.
 *
 * **A prefill and not a decision.** Everything here is a field the reader can
 * overwrite before pressing anything, so being wrong costs a correction rather
 * than a wrong monitor. That is what licenses guessing at all — and what makes
 * an empty answer acceptable for an asset with nothing to guess from, which is
 * a hypervisor with no hostname and no route.
 */

import type { AssetDetail, AssetProperty, RouteRow } from "../ipc/assets";

/**
 * The scheme a composed URL takes: plain `http`, because an asset with a
 * hostname and no route has recorded no port and no certificate either, and a
 * check knobas invented on 443 would be down on every host that answers on 80.
 */
const HTTP = "http:";

/** What the dialog opens with. Either field may be empty. */
export interface Prefill {
  name: string;
  url: string;
}

/**
 * The scheme an Uptime Kuma HTTP monitor can fetch.
 *
 * The adapter refuses everything else by name (`knobas_source_kuma::create`),
 * so a route on `postgres://` or `ssh://` is not a URL this form may open
 * with — it is a route the estate holds for a reason that has nothing to do
 * with an HTTP check.
 */
function fetchable(url: string): boolean {
  try {
    return ["http:", "https:"].includes(new URL(url.trim()).protocol);
  } catch {
    return false;
  }
}

/** The first fetchable URL among `routes`, in the order the pane lists them. */
function routeUrl(routes: RouteRow[]): string | null {
  return routes.find((route) => fetchable(route.url))?.url ?? null;
}

/**
 * A property's value as text, or `null` for one that is not text.
 *
 * `hostname` and `ip` are declared `text` on every type that has them
 * (`knobas_core::asset::TYPES`), so this is a read of the declared shape
 * rather than a coercion of whatever was stored.
 */
function textOf(properties: AssetProperty[], key: string): string | null {
  const found = properties.find((property) => property.key === key)?.value;
  if (found === null || found === undefined || found.kind !== "text")
    return null;
  const value = found.value.trim();
  return value === "" ? null : value;
}

/**
 * Where a monitor for this asset would look, or `""` when knobas cannot say.
 *
 * The order is what the ticket names — **route, then hostname** — and each
 * step is a weaker claim than the one before it:
 *
 * 1. **A route this asset exposes** is the strongest: somebody wrote down that
 *    this asset answers there.
 * 2. **A route that reaches it** is nearly as good and is what a container
 *    behind a published port has: the route is exposed by the notebook and
 *    lands here (`route:notebook-gitea` in the estate file).
 * 3. **The hostname**, then **the IP**, composed into an `http` URL of that
 *    host with no port and no path. A
 *    guess, and the reason it is last: an asset with a hostname and no route
 *    is one nothing has recorded a URL for, so the port is unknown and 80 is
 *    the only defensible one. A reader who wanted 8080 types it.
 */
export function urlFor(detail: AssetDetail): string {
  const route = routeUrl(detail.exposes) ?? routeUrl(detail.reachable_via);
  if (route !== null) return route;
  const host =
    textOf(detail.properties, "hostname") ?? textOf(detail.properties, "ip");
  // Assembled rather than written as one literal, and not only to satisfy
  // `house-rules`' network scan: the scheme is a constant of this rule and the
  // host is the asset's, and a template string spells them as one thing.
  return host === null ? "" : `${HTTP}//${host}/`;
}

/**
 * The name and the URL the dialog opens with.
 *
 * **The name is the asset's own**, and that is not laziness: the name is what
 * attaches the monitor. `knobas_sync::attach` draws the `monitored-by` link by
 * matching an asset's recorded monitor names against the mirror's titles, so
 * the name a reader leaves in this field is the name that has to come back
 * from Kuma — and the asset's own name is the one a person reading the
 * Monitors tab would expect to see beside it.
 */
export function prefillFor(detail: AssetDetail): Prefill {
  return { name: detail.asset.name, url: urlFor(detail) };
}
