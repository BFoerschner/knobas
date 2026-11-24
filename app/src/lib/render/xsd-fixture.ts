/**
 * The schema explorer's fixtures (the XSD design, section 32): the four families in `fixtures/xsd/`, read whole as a
 * repository would hold them, and small schemas of the tests' own for what the families lack. Node only (tests).
 */
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative, resolve } from "node:path";

import { loadSet, type SchemaSet } from "./xsd";
import type { Pointed, SchemaIndex } from "./xsd-locate";
import { sniffNamespace } from "./xsd-sniff";

const ROOT = resolve(__dirname, "../../../../fixtures/xsd");

export type Family = "opentrans-2.1" | "bmecat-2005fd" | "ubl-2.1" | "cii-d16b";

/** Each family's entry, relative to its folder. */
export const ENTRY: Record<Family, string> = {
  "opentrans-2.1": "opentrans_2_1.xsd",
  "bmecat-2005fd": "bmecat_2005.xsd",
  "ubl-2.1": "xsd/maindoc/UBL-Invoice-2.1.xsd",
  "cii-d16b": "data/standard/CrossIndustryInvoice_100pD16B.xsd",
};

function walk(dir: string, out: string[] = []): string[] {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) walk(p, out);
    else if (name.endsWith(".xsd")) out.push(p);
  }
  return out;
}

/** A family's files as a repository holds them: path (under `prefix`) to text. */
export function familyFiles(family: Family, prefix = ""): Map<string, string> {
  const dir = join(ROOT, family);
  const out = new Map<string, string>();
  for (const p of walk(dir)) out.set(prefix + relative(dir, p).split("\\").join("/"), readFileSync(p, "utf8"));
  return out;
}

/** The schemas that come with knobas, by namespace and by path. */
export function bundledFiles(): { byNs: Map<string, string>; texts: Map<string, string> } {
  const dir = resolve(__dirname, "xsd-known");
  const texts = new Map<string, string>();
  const byNs = new Map<string, string>();
  for (const name of ["xml.xsd", "xmldsig-core-schema.xsd"]) {
    const text = readFileSync(join(dir, name), "utf8");
    texts.set(`@knobas/${name}`, text);
    byNs.set(sniffNamespace(text)!, `@knobas/${name}`);
  }
  return { byNs, texts };
}

/** The repository's index made from its texts at once (what `xsd-index.ts` builds by reads). */
export function indexOf(files: ReadonlyMap<string, string>): SchemaIndex {
  const declares = new Map<string, string | undefined>();
  for (const [p, t] of files) if (p.endsWith(".xsd")) declares.set(p, sniffNamespace(t));
  return { declares, catalogs: [], total: declares.size, read: declares.size };
}

/**
 * A set loaded the way the worker loads it, round after round, every file it needs answered from `files` (and the
 * bundled schemas), the index built when asked for.
 */
export function loadAll(files: ReadonlyMap<string, string>, entry: string, options: { pointed?: Pointed[]; noIndex?: boolean } = {}): { set: SchemaSet; rounds: number; indexed: boolean } {
  // As the port does: the paths come with the index, a file beside is tried by reading it.
  let paths: Set<string> | null = null;
  const bundled = bundledFiles();
  const texts = new Map<string, string | null>([[entry, files.get(entry) ?? null]]);
  let index: SchemaIndex | null = null;
  let indexed = false;
  for (let round = 1; round < 50; round += 1) {
    const set = loadSet({ entry, texts, paths, pointed: options.pointed ?? [], index, elsewhere: null, bundled: bundled.byNs });
    if (set.need.length === 0) return { set, rounds: round, indexed };
    for (const n of set.need) {
      if (n === "?index") {
        index = options.noIndex ? { declares: new Map(), catalogs: [], total: 0, read: 0 } : indexOf(files);
        paths = new Set(files.keys());
        indexed = true;
      } else texts.set(n, files.get(n) ?? bundled.texts.get(n) ?? null);
    }
  }
  throw new Error("the set never settled");
}

/** A family loaded whole. */
export function family(name: Family): SchemaSet {
  const files = familyFiles(name);
  return loadAll(files, ENTRY[name]).set;
}
