/**
 * The check (the XSD design, 2026-11-23, section 24; ADR-0058): libxml2, compiled to WebAssembly, judges an XML
 * document against a schema set, in the schema worker. It is independent of knobas's own reading of the schema, and it
 * is what the files' authors already use through `xmllint`.
 *
 * - **It reads the set from memory and from nowhere else.** Every file of the set is handed over under a made-up root
 *   (`mem:///set/`), so a relative `schemaLocation` resolves there; a reference knobas found somewhere other than
 *   beside its file is answered from where knobas found it. Anything else libxml2 asks for does not exist: one input
 *   provider answers, and what it is asked is kept (`asked`), so a test holds that nothing else was.
 * - **It says when it did not check**: a file the schema needs not found here, XML Schema 1.1, a document over
 *   {@link CHECK_CAP}, a schema libxml2 does not compile.
 * - **Its findings are libxml2's**, `{ message, line, xpath }`. The line is 16 bits; a finding is put on its line by
 *   its path, over the document's outline (`nodeAtPath`), by the view.
 * - The compiled schema is kept for the set's life (by the set's files: each path and its text, so a file changed to
 *   the same length is compiled again), and disposed when another set is checked.
 * - libxml2-wasm loads with the first check, never before.
 */
import type { SchemaSet } from "./xsd";
import type { Finding } from "./xsd-findings";
import { baseOf, isRemote, joinPath } from "./xsd-locate";
import { XS } from "./xsd-xml";

/** The largest document checked. */
export const CHECK_CAP = 16 * 1024 * 1024;
/** How many findings cross; the rest are counted. */
export const FINDINGS_CAP = 500;

const ROOT = "mem:///set/";
const DOC_URL = "mem:///document.xml";

export type CheckAnswer =
  | { readonly state: "checked"; readonly findings: readonly Finding[]; readonly total: number; readonly ms: number; readonly compileMs: number; readonly asked: readonly string[] }
  | { readonly state: "not-checked"; readonly why: string; readonly detail: readonly string[] };

type Lib = typeof import("libxml2-wasm");
let lib: Promise<Lib> | null = null;
const loadLib = (): Promise<Lib> => (lib ??= import("libxml2-wasm"));

/** What the one provider answers now: the set's files by URL, the names libxml2 asks for mapped to them. */
const provided = { buffers: new Map<string, Uint8Array>(), alias: new Map<string, string>(), asked: [] as string[] };
let registered = false;

const urlOf = (path: string) => ROOT + encodeURI(path);

/** The compiled schema kept for the set last checked. */
let held: { files: readonly (readonly [string, string | null])[]; validator: { validate: (doc: unknown) => void; dispose: () => void }; schema: { dispose: () => void }; asked: string[] } | null = null;

/** Whether the schema held was compiled from these files: each path and its text (the same text is one string, so a kept set compares at once). */
const sameFiles = (a: readonly (readonly [string, string | null])[], b: readonly (readonly [string, string | null])[]) =>
  a.length === b.length && a.every(([p, t], i) => p === b[i]![0] && t === b[i]![1]);

/** Where libxml2 looks for `location` named in a file it knows by `base`. */
function resolved(base: string, location: string): string {
  if (isRemote(location)) return location;
  if (base.startsWith(ROOT)) return urlOf(joinPath(decodeURI(base.slice(ROOT.length)), location) ?? location);
  try {
    return new URL(location, base).href;
  } catch {
    return location;
  }
}

/**
 * Where each reference was found, as libxml2 will ask for it. libxml2 knows a file by the address it asked for, not by
 * where knobas found it: a file answered elsewhere (by name, by namespace, pointed to, in another repository, bundled)
 * has its own references resolved against the reference's address. So every address a file is known by aliases its
 * references too, down the set, until no new address appears.
 */
function aliasAll(set: SchemaSet): void {
  const known = new Map(set.docs.map((d) => [d.path, new Set([urlOf(d.path)])]));
  for (let grew = true, round = 0; grew && round <= set.docs.length; round += 1) {
    grew = false;
    for (const d of set.docs)
      for (const r of d.refs) {
        if (r.to === null || r.location === null) continue;
        const to = urlOf(r.to);
        const by = known.get(r.to);
        for (const base of [...(known.get(d.path) ?? [])]) {
          const asked = resolved(base, r.location);
          if (asked !== to && !provided.alias.has(asked)) provided.alias.set(asked, to);
          if (by !== undefined && !by.has(asked)) {
            by.add(asked);
            grew = true;
          }
        }
      }
  }
}

/** Why the set cannot be checked yet, or `null`. */
export function cannotCheck(set: SchemaSet, xmlLength: number): { why: string; detail: string[] } | null {
  if (xmlLength > CHECK_CAP) return { why: "larger than knobas checks", detail: [`The document is over ${CHECK_CAP / 1024 / 1024} MiB.`] };
  if (set.need.length > 0 || set.docs.some((d) => d.reading || d.refs.some((r) => r.reading))) return { why: "reading", detail: [] };
  if (set.docs.some((d) => d.v11)) return { why: "XML Schema 1.1", detail: ["This schema uses XML Schema 1.1, which the checker (libxml2) does not read."] };
  const lost = set.docs.flatMap((d) => d.refs.filter((r) => r.to === null && !r.reading && !(r.kind === "import" && r.namespace === XS)));
  if (lost.length > 0) {
    const name = lost[0]!.location ?? lost[0]!.namespace ?? "a file";
    return { why: "not whole", detail: [`The schema needs ${baseOf(name)}, which is not found here.`] };
  }
  const bad = set.docs.find((d) => d.bad !== null);
  if (bad !== undefined) return { why: "not whole", detail: [`${baseOf(bad.path)}: ${bad.bad}`] };
  return null;
}

/** Checks `xml` against the set, whose files' texts `text` answers. */
export async function check(set: SchemaSet, text: (path: string) => string | null, xml: string): Promise<CheckAnswer> {
  const no = cannotCheck(set, xml.length);
  if (no !== null) return { state: "not-checked", why: no.why, detail: no.detail };
  const L = await loadLib();
  const enc = new TextEncoder();
  // The set's texts, and where each reference was found, as libxml2 will ask for them.
  const files = set.docs.map((d) => [d.path, text(d.path)] as const);
  const fresh = held === null || !sameFiles(held.files, files);
  if (fresh) {
    provided.buffers.clear();
    provided.alias.clear();
    for (const [path, t] of files) if (t !== null) provided.buffers.set(urlOf(path), enc.encode(t));
    aliasAll(set);
  }
  provided.asked = [];
  if (!registered) {
    const find = (f: string) => provided.buffers.get(provided.alias.get(f) ?? f);
    const fds = new Map<number, { data: Uint8Array; at: number }>();
    let nextFd = 1;
    registered = L.xmlRegisterInputProvider({
      match: (f: string) => {
        provided.asked.push(f);
        return find(f) !== undefined;
      },
      open: (f: string) => {
        const data = find(f);
        if (data === undefined) return undefined;
        const fd = nextFd++;
        fds.set(fd, { data, at: 0 });
        return fd;
      },
      read: (fd: number, into: Uint8Array) => {
        const h = fds.get(fd);
        if (h === undefined) return -1;
        const n = Math.min(into.length, h.data.length - h.at);
        into.set(h.data.subarray(h.at, h.at + n));
        h.at += n;
        return n;
      },
      close: (fd: number) => fds.delete(fd),
    } as Parameters<Lib["xmlRegisterInputProvider"]>[0]);
  }
  const t0 = performance.now();
  if (held === null || fresh) {
    if (held !== null) {
      held.validator.dispose();
      held.schema.dispose();
      held = null;
    }
    const entry = provided.buffers.get(urlOf(set.entry));
    if (entry === undefined) return { state: "not-checked", why: "not whole", detail: ["The schema's own file could not be read."] };
    let schema: InstanceType<Lib["XmlDocument"]> | null = null;
    try {
      schema = L.XmlDocument.fromBuffer(entry, { url: urlOf(set.entry) });
      const validator = L.XsdValidator.fromDoc(schema);
      held = { files, validator: validator as unknown as { validate: (doc: unknown) => void; dispose: () => void }, schema, asked: [...provided.asked] };
    } catch (e) {
      schema?.dispose();
      const details = ((e as { details?: { message: string }[] }).details ?? []).map((d) => d.message.trim());
      return { state: "not-checked", why: "not compiled", detail: details.length > 0 ? details.slice(0, 3) : [e instanceof Error ? e.message : String(e)] };
    }
  }
  const t1 = performance.now();
  let findings: Finding[] = [];
  let doc: InstanceType<Lib["XmlDocument"]> | null = null;
  try {
    doc = L.XmlDocument.fromString(xml, { url: DOC_URL });
    held.validator.validate(doc);
  } catch (e) {
    const details = (e as { details?: { message: string; line: number; xpath?: string }[] }).details ?? [];
    findings = details.map((d) => ({ message: d.message, line: d.line, xpath: d.xpath ?? null }));
    if (findings.length === 0) findings = [{ message: e instanceof Error ? e.message : String(e), line: 0, xpath: null }];
  } finally {
    doc?.dispose();
  }
  const t2 = performance.now();
  // What libxml2 asked for while it compiled the set, and while it read the document.
  return { state: "checked", findings: findings.slice(0, FINDINGS_CAP), total: findings.length, ms: t2 - t1, compileMs: t1 - t0, asked: [...held.asked, ...provided.asked] };
}
