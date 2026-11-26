/**
 * Pointing a reference at a file (the XSD design, section 17): the candidates a picker lists, and the answer kept in
 * `xsd.locations` for the namespace and location asked for, the last given winning.
 */
import { xsdDownloaded, xsdFetch, xsdLocations, xsdSetLocations, type DeclaringFile, type DownloadedSchema, type PointedSchema } from "../../../ipc/api";
import { heldIndex } from "../../../render/xsd-index";
import { forgetKeptSets } from "../../../render/xsd-port";
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

/** What *Find a schema file* ranks a file by (#2535): its repository's name (none for this one's), then its path. */
export function pointText(c: Candidate): string {
  return `${c.repoName ?? ""} ${c.path}`;
}

/**
 * The files in the picker's order: those that `fits` first, this repository's before another's, then, with words
 * typed, the ranking's order (`list` as ranked, best first), else the shorter path.
 */
export function pointSorted(list: readonly Candidate[], fits: (c: Candidate) => boolean, ranked: boolean): Candidate[] {
  return [...list].sort(
    (a, b) =>
      Number(fits(b)) - Number(fits(a)) ||
      Number(a.repoName !== null) - Number(b.repoName !== null) ||
      (ranked ? 0 : a.path.length - b.path.length || (a.path < b.path ? -1 : a.path > b.path ? 1 : 0)),
  );
}

/** The repository's schema files as candidates, from its index's sniffs. */
export function candidatesOf(repo: string, declares: readonly (readonly [string, string | null])[]): Candidate[] {
  return declares.filter(([, ns]) => ns !== null).map(([path, ns]) => ({ repo, repoName: null, path, declares: ns ?? "" }));
}

/** `list` with `entry` the answer for its namespace and location: in the place of the one it replaces, else last. */
function put(list: readonly PointedSchema[], entry: PointedSchema): PointedSchema[] {
  const at = list.findIndex((p) => p.namespace === entry.namespace && p.location === entry.location);
  return at === -1 ? [...list, entry] : list.map((p, i) => (i === at ? entry : p));
}

/** The list kept with one answer more for `namespace` and `location`: the last given wins. */
export function withPointed(list: readonly PointedSchema[], namespace: string | null, location: string | null, c: Candidate): PointedSchema[] {
  const to: PointedSchema["to"] = c.repoName === null ? { repo: c.repo, path: c.path } : { repo: c.repo, path: c.path, name: c.repoName };
  return put(list, { namespace: namespace ?? "", location: location ?? "", to });
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

/** This repository's candidates: its index, read once a listing and shared with the set's own need of it. */
function ownCandidates(repo: string, paths: readonly string[], from: string, read: (path: string) => Promise<string | null>): Promise<Candidate[]> {
  return heldIndex(repo, paths, from, read).then(
    (index) => candidatesOf(repo, index.declares),
    () => [],
  );
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
  const here = reads.paths().then(
    (list) => ownCandidates(repo, list, from, reads.read),
    () => [],
  );
  const [mine, there] = await Promise.all([here, away]);
  return [...mine, ...candidatesElsewhere(namespace ?? "", there)];
}

/** Keeps `c` as the answer for `namespace` and `location` (`xsd.locations`); the list as kept, as the set reads it. */
export async function keepPick(namespace: string | null, location: string | null, c: Candidate): Promise<Pointed[]> {
  const kept = await xsdSetLocations({ pointed: withPointed((await xsdLocations()).pointed, namespace, location, c) });
  forgetKeptSets();
  return asPointed(kept.pointed);
}

/** A downloaded schema's SHA-256 from its path in a set (`@download/<sha256>`), `null` for any other path. */
export function downloadedSha(path: string): string | null {
  return /^@download\/([0-9a-f]{64})$/.exec(path)?.[1] ?? null;
}

/** A downloaded schema's text, read from the profile; `null` where it is not kept. */
export function readDownloaded(path: string): Promise<string | null> | null {
  const sha = downloadedSha(path);
  return sha === null ? null : xsdDownloaded(sha).catch(() => null);
}

/** The list kept with a download as the answer for `namespace` and `location`: the last given wins. */
export function withDownload(list: readonly PointedSchema[], namespace: string | null, location: string | null, d: DownloadedSchema): PointedSchema[] {
  return put(list, { namespace: namespace ?? "", location: location ?? "", to: { url: d.url, sha256: d.sha256, at: d.at } });
}

/**
 * *Download it* (section 17): the schema at `address` downloaded on the press, kept as the answer for `namespace` and
 * `location`; the list as kept, as the set reads it.
 */
export async function keepDownload(namespace: string | null, location: string | null, address: string): Promise<Pointed[]> {
  const d = await xsdFetch(address);
  const kept = await xsdSetLocations({ pointed: withDownload((await xsdLocations()).pointed, namespace, location, d) });
  forgetKeptSets();
  return asPointed(kept.pointed);
}

/** Whether a downloaded file is still the answer of any entry but the one at `except`: its file stays while one is. */
export function stillUsed(list: readonly PointedSchema[], sha256: string, except: number): boolean {
  return list.some((p, i) => i !== except && "url" in p.to && p.to.sha256 === sha256);
}
