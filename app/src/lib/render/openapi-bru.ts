/**
 * A spec as a Bruno collection, written by knobas itself (ux31, section 25): `.bru` files every Bruno from 1.x to 4.x
 * reads, made from the request each operation starts as in the explorer (`openapi-request.ts`), so a request in
 * Bruno starts with the values it starts with in knobas. It runs in the spec worker.
 *
 * - `bruno.json`: the collection's name, and the spec beside it named as its OpenAPI source.
 * - `collection.bru`: the title, the sign-in the spec's first scheme asks for with its secret as a variable
 *   (`{{token}}`, `{{password}}`, `{{apiKey}}`), and the API's description as its docs.
 * - `environments/<server>.bru`, one a server: `baseUrl`, and the secret's name as a secret variable with **no
 *   value**. No secret is ever written: the person types it in Bruno.
 * - A folder a group, in the rail's *Tags* order, each with `folder.bru`; a request an operation, named by its
 *   summary, else its method and path, with `:name` for a path parameter, each query and header row (an optional
 *   one switched off with `~`), its starting body, and its description as docs.
 *
 * Bruno's quoting, read from `@usebruno/lang`'s writer: a key holding `:`, `"`, `{`, `}` or a space is quoted; a
 * value is one line; a body and docs are indented two spaces inside their block. A name is made safe for every file
 * system. The witness is Bruno's own parser (`openapi-bru.test.ts`).
 */
import type { Operation, Scheme, SpecIndex, Server } from "./openapi";
import { firstSchemeOf, serverUrl, type RequestDraft } from "./openapi-request";
import type { Group } from "./openapi-view";

/** A key as Bruno's writer quotes it. */
export function bruKey(k: string): string {
  return /[:"{} ]/.test(k) ? `"${k.replace(/"/g, '\\"')}"` : k;
}

/** A value on one line: a line break becomes a space. */
export function bruOne(v: unknown): string {
  return String(v ?? "").replace(/\s*[\r\n]+\s*/g, " ");
}

const indent = (text: string) =>
  text
    .split("\n")
    .map((l) => `  ${l}`)
    .join("\n");

/** A block of `name { … }` with a line each, or nothing for no lines. */
function block(name: string, lines: readonly string[]): string {
  return lines.length === 0 ? "" : `${name} {\n${lines.map((l) => `  ${l}`).join("\n")}\n}\n\n`;
}

/** A file or folder name every file system takes: `<>:"/\|?*` and controls become spaces, at most 120 characters. */
export function safeName(name: string): string {
  // eslint-disable-next-line no-control-regex
  const out = name.replace(/[<>:"/\\|?*\u0000-\u001f]/g, " ").replace(/\s+/g, " ").replace(/[. ]+$/g, "").replace(/^[. ]+/, "").trim().slice(0, 120);
  // A name Windows keeps for a device is no file's.
  return out === "" || /^(con|prn|aux|nul|com\d|lpt\d)$/i.test(out) ? "request" : out;
}

/** The collection's sign-in for `scheme`: its blocks, its plain variables, its secret variables. */
function auth(scheme: Scheme | null): { collection: string; vars: string[]; secrets: string[] } {
  if (scheme === null) return { collection: "", vars: [], secrets: [] };
  if (scheme.type === "http" && scheme.scheme === "basic") {
    return { collection: `${block("auth", ["mode: basic"])}${block("auth:basic", ["username: {{username}}", "password: {{password}}"])}`, vars: ["username: "], secrets: ["password"] };
  }
  if (scheme.type === "apiKey") {
    return {
      collection: `${block("auth", ["mode: apikey"])}${block("auth:apikey", [`key: ${bruOne(scheme.keyName)}`, "value: {{apiKey}}", `placement: ${scheme.in === "query" ? "query" : "header"}`])}`,
      vars: [],
      secrets: ["apiKey"],
    };
  }
  return { collection: `${block("auth", ["mode: bearer"])}${block("auth:bearer", ["token: {{token}}"])}`, vars: [], secrets: ["token"] };
}

/** One request's `.bru` text, from its operation and the draft it starts as. */
export function bruRequest(op: Operation, draft: RequestDraft, seq: number): string {
  const media = draft.media;
  const kind = media === null || draft.body.trim() === "" ? "none" : /json/.test(media) ? "json" : /xml/.test(media) ? "xml" : /x-www-form-urlencoded/.test(media) ? "formUrlEncoded" : /multipart/.test(media) ? "multipartForm" : "text";
  const url = `{{baseUrl}}${op.path.replace(/\{([^}]+)\}/g, ":$1")}`;
  const name = op.summary || `${op.method.toUpperCase()} ${op.path}`;
  // `security` is the operation's own, else the spec's: none at all, or only `{}`, is an operation that needs no sign-in.
  const anonymous = op.security.every((r) => r.length === 0);
  let out = block("meta", [`name: ${bruOne(name)}`, "type: http", `seq: ${seq}`]);
  out += block(op.method.toLowerCase(), [`url: ${url}`, `body: ${kind}`, `auth: ${anonymous ? "none" : "inherit"}`]);
  const kv = (p: RequestDraft["params"][number]) => `${p.on || p.required ? "" : "~"}${bruKey(p.name)}: ${bruOne(p.value)}`;
  out += block("params:query", draft.params.filter((p) => p.in === "query").map(kv));
  out += block("params:path", draft.params.filter((p) => p.in === "path").map((p) => `${bruKey(p.name)}: ${bruOne(p.value)}`));
  out += block("headers", draft.params.filter((p) => p.in === "header").map(kv));
  if (kind === "json" || kind === "xml" || kind === "text") out += `body:${kind} {\n${indent(draft.body)}\n}\n\n`;
  else if (kind === "formUrlEncoded" || kind === "multipartForm") {
    const pairs = draft.body
      .split("\n")
      .filter((l) => l.includes("="))
      .map((l) => `${bruKey(l.slice(0, l.indexOf("=")))}: ${bruOne(l.slice(l.indexOf("=") + 1))}`);
    out += block(kind === "formUrlEncoded" ? "body:form-urlencoded" : "body:multipart-form", pairs);
  }
  const words = [op.description, op.deprecated ? "Deprecated." : ""].filter((w) => w !== "").join("\n\n");
  if (words !== "") out += `docs {\n${indent(words)}\n}\n`;
  return out;
}

/** What the writer reads of the spec: its index and groups, and each operation with the draft it starts as. */
export interface CollectionInput {
  index: SpecIndex;
  groups: readonly Group[];
  operation: (i: number) => { op: Operation; draft: RequestDraft } | null;
  /** The spec's own file name beside the collection (`openapi.yaml`), named as its OpenAPI source. */
  specFile: string;
}

/** The collection's files, by path from its folder. */
export function collection(input: CollectionInput): [string, string][] {
  const { index } = input;
  const files: [string, string][] = [];
  const scheme = firstScheme(input) ?? index.security[0] ?? null;
  const a = auth(scheme);
  files.push(["bruno.json", `${JSON.stringify({ version: "1", name: index.title, type: "collection", ignore: ["node_modules", ".git"], openapi: [{ sourceUrl: input.specFile }] }, null, 2)}\n`]);
  files.push(["collection.bru", `${block("meta", [`name: ${bruOne(index.title)}`])}${a.collection}${index.description !== "" ? `docs {\n${indent(index.description)}\n}\n` : ""}`]);
  const servers: Server[] = index.servers.length > 0 ? index.servers : [{ url: "http://localhost", description: "", variables: {} }];
  const envNames = new Set<string>();
  servers.forEach((s, n) => {
    let label = safeName(s.description || (servers.length === 1 ? "Server" : `Server ${n + 1}`));
    for (let k = 2; envNames.has(label.toLowerCase()); k++) label = `${label.slice(0, 110)} ${k}`;
    envNames.add(label.toLowerCase());
    const url = serverUrl(s, {}).replace(/\{([^}]+)\}/g, "{{$1}}").replace(/\/$/, "");
    files.push([`environments/${label}.bru`, `${block("vars", [`baseUrl: ${bruOne(url)}`, ...a.vars])}${a.secrets.length > 0 ? `vars:secret [\n${a.secrets.map((v) => `  ${v}`).join(",\n")}\n]\n` : ""}`]);
  });
  const folders = new Set<string>();
  for (const g of input.groups) {
    let folder = safeName(g.name.replace(/^\//, ""));
    for (let k = 2; folders.has(folder.toLowerCase()); k++) folder = `${safeName(g.name.replace(/^\//, "")).slice(0, 110)} ${k}`;
    folders.add(folder.toLowerCase());
    files.push([`${folder}/folder.bru`, block("meta", [`name: ${bruOne(g.name)}`]) + (g.description !== "" ? `docs {\n${indent(g.description)}\n}\n` : "")]);
    const taken = new Set<string>(["folder"]);
    g.ops.forEach((row, seq) => {
      const made = input.operation(row.i);
      if (made === null) return;
      const base = safeName(row.summary || `${row.method.toUpperCase()} ${row.path}`);
      let file = base;
      for (let k = 2; taken.has(file.toLowerCase()); k++) file = `${base.slice(0, 110)} ${k}`;
      taken.add(file.toLowerCase());
      files.push([`${folder}/${file}.bru`, bruRequest(made.op, made.draft, seq + 1)]);
    });
  }
  return files;
}

/** The collection's sign-in: the scheme the first operation that asks for one names first. */
function firstScheme(input: CollectionInput): Scheme | null {
  // The first operations say it; a spec whose first twenty name no scheme has none to offer.
  for (const row of input.index.ops.slice(0, 20)) {
    const made = input.operation(row.i);
    const name = made === null ? null : firstSchemeOf(made.op, input.index.security);
    if (name !== null) return input.index.security.find((s) => s.name === name) ?? null;
  }
  return null;
}
