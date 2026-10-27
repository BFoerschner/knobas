/**
 * Where each asset sits, from Dependencies' read (#875): the places *Inside*
 * offers, each named by its path, and what sits under an asset, so a move
 * into itself or into something it holds is never offered.
 */
import type { Dependencies } from "../../ipc/assets";
import { isAsset } from "./deps";

export interface Place {
  id: string;
  name: string;
  /** "hel1 › hv-01 › vm-pay-01". */
  path: string;
}

export function assetPaths(read: Dependencies): { places: Place[]; under: (id: string) => Set<string> } {
  const assets = read.items.filter((item) => isAsset(item.kind));
  const name = new Map(assets.map((item) => [item.id, item.name]));
  const parent = new Map<string, string>();
  const children = new Map<string, string[]>();
  for (const relation of read.relations) {
    if (relation.relation !== "inside" || !name.has(relation.from) || !name.has(relation.to)) continue;
    parent.set(relation.from, relation.to);
    (children.get(relation.to) ?? children.set(relation.to, []).get(relation.to)!).push(relation.from);
  }
  const pathOf = (id: string) => {
    const names: string[] = [];
    const seen = new Set<string>();
    for (let at: string | undefined = id; at !== undefined && !seen.has(at); at = parent.get(at)) {
      seen.add(at);
      names.unshift(name.get(at)!);
    }
    return names.join(" › ");
  };
  const places = assets
    .map((item) => ({ id: item.id, name: item.name, path: pathOf(item.id) }))
    .sort((a, b) => a.path.localeCompare(b.path));
  const under = (id: string) => {
    const all = new Set<string>();
    const walk = [id];
    while (walk.length > 0) for (const child of children.get(walk.pop()!) ?? []) if (!all.has(child)) (all.add(child), walk.push(child));
    return all;
  };
  return { places, under };
}

/**
 * The places an asset may move into: every one but the asset and what it
 * holds. What it holds is walked once, not once a place (#2200's class: an
 * engine holding 5,000 containers took a second to open its editor).
 */
export function placesOutside(paths: { places: Place[]; under: (id: string) => Set<string> }, id: string): Place[] {
  const held = paths.under(id);
  return paths.places.filter((place) => place.id !== id && !held.has(place.id));
}
