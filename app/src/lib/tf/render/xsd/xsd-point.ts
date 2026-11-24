/**
 * Pointing a reference at a file (the XSD design, section 17): the candidates a picker lists, and the answer kept in
 * `xsd.locations` for the namespace and location asked for, the last given winning.
 */
import type { PointedSchema } from "../../../ipc/api";
import type { Pointed } from "../../../render/xsd-locate";

export interface Candidate {
  readonly repo: string;
  /** The repository's name, for another repository's file; `null` for this one's. */
  readonly repoName: string | null;
  readonly path: string;
  /** What it declares: `""` for no namespace. */
  readonly declares: string;
}

/** The repository's schema files as candidates, from its index's sniffs. */
export function candidatesOf(repo: string, declares: readonly (readonly [string, string | null])[]): Candidate[] {
  return declares.filter(([, ns]) => ns !== null).map(([path, ns]) => ({ repo, repoName: null, path, declares: ns ?? "" }));
}

/** The list kept with one answer more for `namespace` and `location`: the last given wins. */
export function withPointed(list: readonly PointedSchema[], namespace: string | null, location: string | null, c: Candidate): PointedSchema[] {
  const ns = namespace ?? "";
  const loc = location ?? "";
  const to: PointedSchema["to"] = c.repoName === null ? { repo: c.repo, path: c.path } : { repo: c.repo, path: c.path, name: c.repoName };
  return [...list.filter((p) => !(p.namespace === ns && p.location === loc)), { namespace: ns, location: loc, to }];
}

/** The kept list as the set's reader takes it. */
export const asPointed = (list: readonly PointedSchema[]): Pointed[] => list.map((p) => ({ namespace: p.namespace, location: p.location, to: p.to }));

/** A pointed file of this repository is read as one of its own; another repository's through its own read. */
export function splitAway(path: string): { repo: string; path: string } | null {
  const m = /^@repo\/([^/]+)\/(.+)$/.exec(path);
  return m === null ? null : { repo: decodeURIComponent(m[1]!), path: m[2]! };
}
