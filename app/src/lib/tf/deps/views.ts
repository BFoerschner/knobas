/**
 * The saved views of Dependencies (#873, R-11.13, R-11.16), pure: what a view
 * keeps, and the words its menu says.
 *
 * A view keeps per shared layer whether it shows, folds and how it groups,
 * plus the environment filter, *Problems and what they touch*, tile size and
 * fold count, and nothing else. The layers themselves are shared and saved at
 * once by their own writes.
 */
import type { DepGroup, DepTiles, DepViews, DepViewSettings, LayerInView } from "../../ipc/assets";
import { UNNAMED_LAYER } from "./draw";

/** *Group by*'s choices, in prototype-6's words and order. */
export const GROUPS: readonly { value: DepGroup; label: string }[] = [
  { value: "none", label: "No groups" },
  { value: "environment", label: "Environment" },
  { value: "owner", label: "Owner or team" },
  { value: "parent", label: "What it runs on" },
  { value: "project", label: "Project" },
  { value: "status", label: "Status" },
];

/** *Tiles*' choices. */
export const TILES: readonly { value: DepTiles; label: string }[] = [
  { value: "auto", label: "Small when a layer holds more than 30" },
  { value: "roomy", label: "Always roomy" },
  { value: "compact", label: "Always small" },
];

/** The environment chips, in prototype-6's order. */
export const ENVIRONMENTS = ["prod", "stage", "dev", "shared"] as const;

/** Healthy groups fold above this many items unless a view says (the backend's `FOLD_AT`). */
export const FOLD_AT = 24;

/** A view's settings from a few layers' choices and a set of hidden layers. */
export function settingsOf(layers: Record<string, Partial<LayerInView>>, hidden: ReadonlySet<string> = new Set()): DepViewSettings {
  const all: Record<string, LayerInView> = {};
  for (const [id, over] of Object.entries(layers)) all[id] = { ...UNNAMED_LAYER, ...over };
  for (const id of hidden) all[id] = { ...(all[id] ?? UNNAMED_LAYER), hidden: true };
  return { layers: all, environments: [], problems: false, tiles: "auto", fold_at: FOLD_AT };
}

/** A copy a draft can change without touching what was saved. */
export function cloneSettings(settings: DepViewSettings): DepViewSettings {
  return {
    ...settings,
    layers: Object.fromEntries(Object.entries(settings.layers).map(([id, layer]) => [id, { ...layer }])),
    environments: [...settings.environments],
  };
}

/** Whether two settings say the same thing. */
export function sameSettings(a: DepViewSettings, b: DepViewSettings): boolean {
  const keys = new Set([...Object.keys(a.layers), ...Object.keys(b.layers)]);
  for (const key of keys) {
    const x: LayerInView = a.layers[key] ?? UNNAMED_LAYER;
    const y: LayerInView = b.layers[key] ?? UNNAMED_LAYER;
    if (x.hidden !== y.hidden || x.folded !== y.folded || x.group !== y.group) return false;
  }
  return (
    a.problems === b.problems &&
    a.tiles === b.tiles &&
    a.fold_at === b.fold_at &&
    [...a.environments].sort().join() === [...b.environments].sort().join()
  );
}

/**
 * What the view menu says under a view's name: which opening it is the
 * default for, else how many layers it shows.
 */
export function viewBadge(views: DepViews, id: string, layerCount: number): string {
  const badges = [
    views.opens_from_ticket === id ? "opens from a ticket" : "",
    views.opens_from_top === id ? "opens from the top bar" : "",
  ].filter(Boolean);
  if (badges.length > 0) return badges.join(", ");
  const view = views.views.find((v) => v.id === id);
  const shown = view ? layerCount - Object.values(view.settings.layers).filter((layer) => layer.hidden).length : layerCount;
  return `${shown} of ${layerCount} layers`;
}

/** A free name for a new layer: *New layer*, then *New layer 2* and on. */
export function freeLayerName(taken: readonly string[]): string {
  const used = new Set(taken.map((name) => name.trim().toLowerCase()));
  if (!used.has("new layer")) return "New layer";
  let n = 2;
  while (used.has(`new layer ${n}`)) n += 1;
  return `New layer ${n}`;
}
