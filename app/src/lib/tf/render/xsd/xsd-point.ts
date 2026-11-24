/**
 * Pointing a reference at a file (the XSD design, section 17): the candidates a picker lists, and the answer kept in
 * `xsd.locations` for the namespace and location asked for, the last given winning.
 */
import { xsdLocations, xsdSetLocations, type DeclaringFile, type PointedSchema } from "../../../ipc/api";
import { buildIndex } from "../../../render/xsd-index";
import { keyOf } from "../../../time/format";
import type { Elsewhere, Pointed } from "../../../render/xsd-locate";

export interface Candidate {
  readonly repo: string;
  /** The repository's name, for another repository's file; `null` for this one's. */
  readonly repoName: string | null;
  /** The branch another repository's file is read on; `null` for this one's. */
  readonly branch?: string | null;
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

/** A repository's name from its entity id: `gitea:tidewater/catalog-tools` is `tidewater/catalog-tools`. */
export const repoNameOf = (id: string): string => keyOf(id);

/** The files of other synced repositories that declare a namespace, as the order's step 6 reads them: this one's left out. */
export function elsewhereOf(here: string, list: readonly DeclaringFile[]): Elsewhere[] {
  return list.filter((f) => f.repo !== here).map((f) => ({ repo: f.repo, name: repoNameOf(f.repo), branch: f.branch, path: f.path }));
}

/** Other repositories' files that declare `namespace`, as the picker lists them after this repository's. */
export function candidatesElsewhere(namespace: string, list: readonly Elsewhere[]): Candidate[] {
  return list.map((e) => ({ repo: e.repo, repoName: e.name, branch: e.branch, path: e.path, declares: namespace }));
}

/** Where the picker's candidates are read: this repository's schemas, and other repositories' that declare a namespace. */
export interface CandidateReads {
  readonly paths: () => Promise<readonly string[]>;
  readonly read: (path: string) => Promise<string | null>;
  readonly elsewhere: ((namespace: string) => Promise<Elsewhere[]>) | null;
}

/**
 * The picker's candidates for `namespace`: this repository's `.xsd` files (from its index, started from `from`), then
 * another synced repository's that declare the namespace (section 16, step 6). Nothing where none can be read.
 */
export async function gatherCandidates(repo: string, from: string, namespace: string | null, reads: CandidateReads): Promise<Candidate[]> {
  const away = namespace === null || namespace === "" || reads.elsewhere === null ? Promise.resolve([]) : reads.elsewhere(namespace).catch(() => []);
  const here = reads
    .paths()
    .then((list) => buildIndex(repo, list, from, reads.read))
    .then(
      (index) => candidatesOf(repo, index.declares),
      () => [],
    );
  const [mine, there] = await Promise.all([here, away]);
  return [...mine, ...candidatesElsewhere(namespace ?? "", there)];
}

/** Keeps `c` as the answer for `namespace` and `location` (`xsd.locations`); the list as kept, as the set reads it. */
export async function keepPick(namespace: string | null, location: string | null, c: Candidate): Promise<Pointed[]> {
  const kept = await xsdSetLocations({ pointed: withPointed((await xsdLocations()).pointed, namespace, location, c) });
  return asPointed(kept.pointed);
}
