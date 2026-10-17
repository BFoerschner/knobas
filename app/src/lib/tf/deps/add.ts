/**
 * *Add ▾* and the route dialog's rules (#876, R-11.20, R-11.21), pure.
 *
 * New asset asks for a name, a type and a place: the layer's types first,
 * then any type, and a line naming what usually goes inside the place picked.
 * The route dialog asks for an address with a scheme, a name (from the
 * address when left empty), the asset that exposes it, where it lands and who
 * can reach it; editing sends one `edit_route` with only what changed.
 */
import { ipcErrorMessage } from "../../ipc";
import type { AssetType, Dependencies, RouteEdit, RouteRow, Visibility } from "../../ipc/assets";

/** The type picker's two groups: *In this layer*, then *Any type*. */
export function typeChoices(layerKinds: readonly string[], types: readonly AssetType[]): { here: AssetType[]; rest: AssetType[] } {
  const here = layerKinds.flatMap((kind) => types.filter((t) => t.id === kind));
  return { here, rest: types.filter((t) => !here.includes(t)) };
}

/** The line under *Inside*, in prototype-6's words. */
export function usualLine(parentId: string | null, read: Dependencies, types: readonly AssetType[]): string {
  const parent = parentId === null ? undefined : read.items.find((item) => item.id === parentId);
  if (parent === undefined) return "At the top of the estate: usually a site or a hypervisor.";
  const suggests = types.find((t) => t.id === parent.kind)?.suggests ?? [];
  const labels = suggests.map((id) => types.find((t) => t.id === id)?.label ?? id);
  return labels.length > 0
    ? `Usual inside ${parent.name}: ${labels.join(", ")}. Any type is allowed.`
    : `Nothing in particular usually goes inside ${parent.name}. Any type is allowed.`;
}

const SCHEME = /^[a-z][a-z0-9+.-]*:\/\/./i;

/** Whether an address carries a scheme and something after it. */
export const hasScheme = (url: string): boolean => SCHEME.test(url.trim());

/** The name a route takes when none is typed: the address less its scheme and path. */
export const routeNameOf = (url: string): string =>
  url
    .trim()
    .replace(/^[a-z][a-z0-9+.-]*:\/\//i, "")
    .replace(/\/.*/, "");

/**
 * *Link it to <key>*, the second call once the new item exists: `null` when
 * it linked or was not asked to, else why it did not.
 */
export async function linkToTicket(
  createLink: (fromId: string, toId: string) => Promise<unknown>,
  ticket: { id: string } | null,
  wanted: boolean,
  itemId: string,
): Promise<string | null> {
  if (ticket === null || !wanted) return null;
  try {
    await createLink(ticket.id, itemId);
    return null;
  } catch (error) {
    return ipcErrorMessage(error);
  }
}

/** What the route dialog's fields hold. `target` `null` is *Nothing knobas knows*. */
export interface RouteForm {
  url: string;
  name: string;
  target: string | null;
  visibility: Visibility;
}

/** Only what changed, as one `edit_route` list, in the dialog's order. */
export function routeEditsOf(row: RouteRow, form: RouteForm): RouteEdit[] {
  const edits: RouteEdit[] = [];
  const url = form.url.trim();
  if (url !== row.url) edits.push({ field: "url", value: url });
  const name = form.name.trim() || routeNameOf(url);
  if (name !== row.name) edits.push({ field: "name", value: name });
  if (form.target !== row.target_id) edits.push({ field: "target", value: form.target });
  if (form.visibility !== row.visibility) edits.push({ field: "visibility", value: form.visibility });
  return edits;
}
