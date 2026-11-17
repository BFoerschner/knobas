/**
 * An OpenAPI spec read for its view (ux18, `docs/decisions/2026-11-17-an-api-spec-is-drawn-as-its-endpoints.md`):
 * OpenAPI 3.0 and 3.1 and Swagger 2.0, YAML or JSON. This module runs where the parse runs, in the spec worker
 * (`openapi.worker.ts`) in the app and on the test's own thread under jsdom (`openapi-port.ts`), and the view never
 * holds the parsed document: it asks for the pieces it draws.
 *
 * - **`openSpec`** parses and checks a file and answers its {@link SpecIndex}: the head (title, version, description,
 *   servers), the tags, one row per operation and the schemas' names. A file that does not parse, or is not a spec
 *   knobas can draw, answers the error and the line it is on.
 * - **`operation`** answers one operation as the view draws it: the path's parameters merged into its own, the
 *   parameters, request body and responses whose `$ref`s point inside their own file resolved, and Swagger 2.0's
 *   body parameter, `produces` and `consumes` read as 3.x's request body and content.
 * - **`resolve`** follows one `$ref` from the file a node came from: a JSON pointer in that file, or in another file
 *   of the repository (`./schemas/pet.yaml#/Pet`), which answers `{ need }` until the view has read it and handed it
 *   over with `addFile`. A ref to a URL is never followed: it answers what it names.
 *
 * Nodes are the document's own values: a schema is drawn from its JSON, and its `$ref`s stay refs until the view
 * opens one.
 */
import { JSON_SCHEMA, load } from "js-yaml";

export const METHODS = ["get", "put", "post", "delete", "options", "head", "patch", "trace"] as const;

/** One operation's row in the list. */
export interface OpRow {
  /** Its place in `ops`, what `operation` takes. */
  i: number;
  method: string;
  path: string;
  summary: string;
  operationId: string | null;
  tags: string[];
  deprecated: boolean;
  /** The operation's anchor (`get /pets/{petId}`): what a link to it names. */
  anchor: string;
}

/** A path whose item lives in another file (`$ref: ./paths/pets.yaml`): listed by its path, read when opened. */
export interface PathRef {
  path: string;
  ref: string;
}

export interface Server {
  url: string;
  description: string;
}

export interface Tag {
  name: string;
  description: string;
}

export interface SpecIndex {
  /** `openapi` or `swagger`, and the version it gave (`3.1.0`, `2.0`). */
  key: "openapi" | "swagger";
  version: string;
  title: string;
  /** The API's own version (`info.version`). */
  apiVersion: string;
  /** Markdown. */
  description: string;
  servers: Server[];
  /** The tags in the order the spec lists them, then those only operations name, in the order they first appear. */
  tags: Tag[];
  ops: OpRow[];
  pathRefs: PathRef[];
  /** The named schemas (`components.schemas`, 2.0's `definitions`), in the spec's order. */
  schemas: string[];
  /** Where the named schemas live: `#/components/schemas/` or `#/definitions/`. */
  schemaBase: string;
  /** What was passed over and why (an operation that is not an object): the view lists them under the head. */
  problems: string[];
}

export type Opened = { ok: true; index: SpecIndex } | { ok: false; error: string; line: number | null };

/** A node and the file it came from, where the refs inside it are read from. */
export interface Placed {
  node: unknown;
  base: string;
}

export interface Param {
  name: string;
  in: string;
  required: boolean;
  deprecated: boolean;
  description: string;
  schema: unknown;
  example: unknown;
  /** A parameter that is a ref not yet followed (another file's): drawn as the ref. */
  ref: string | null;
}

export interface Example {
  name: string;
  summary: string;
  value: unknown;
  /** An example kept outside the spec (`externalValue`): named, never read. */
  external: string | null;
}

export interface Media {
  type: string;
  schema: unknown;
  examples: Example[];
}

export interface Header {
  name: string;
  description: string;
  schema: unknown;
}

export interface Body {
  description: string;
  required: boolean;
  content: Media[];
  ref: string | null;
}

export interface Response {
  status: string;
  description: string;
  content: Media[];
  headers: Header[];
  ref: string | null;
}

export interface Operation {
  i: number;
  method: string;
  path: string;
  summary: string;
  description: string;
  operationId: string | null;
  deprecated: boolean;
  parameters: Param[];
  body: Body | null;
  responses: Response[];
  /** The security schemes it asks for, by name, each requirement's names joined with `+`. */
  security: string[];
  /** The file the operation's refs are read from. */
  base: string;
}

/** What a ref resolves to: the node and its file, a file to read first, or why it cannot be followed. */
export type Resolved = { ok: true; node: unknown; base: string } | { need: string } | { ok: false; error: string };

type Obj = Record<string, unknown>;

const isObj = (v: unknown): v is Obj => typeof v === "object" && v !== null && !Array.isArray(v);
const str = (v: unknown): string => (typeof v === "string" ? v : typeof v === "number" || typeof v === "boolean" ? String(v) : "");

/** One call of the view to where its spec is kept (`openapi-port.ts`). */
export type SpecCall =
  | { kind: "open"; path: string; text: string }
  | { kind: "close" }
  | { kind: "operation"; i: number }
  | { kind: "pathItem"; ref: PathRef }
  | { kind: "resolve"; base: string; ref: string }
  | { kind: "addFile"; path: string; text: string };

/** Answers `call` for `session` among `sessions`: the spec worker's and this thread's one dispatch. */
export function answerCall(sessions: Map<number, SpecFiles>, session: number, call: SpecCall): unknown {
  if (call.kind === "open") {
    const files = new SpecFiles(call.path);
    sessions.set(session, files);
    return files.open(call.text);
  }
  if (call.kind === "close") {
    sessions.delete(session);
    return null;
  }
  const files = sessions.get(session);
  if (files === undefined) return { ok: false, error: "The spec was closed." };
  if (call.kind === "operation") return files.operation(call.i);
  if (call.kind === "pathItem") return files.pathItem(call.ref);
  if (call.kind === "resolve") return files.resolve(call.base, call.ref);
  return files.addFile(call.path, call.text);
}

/** The files of one spec: the spec itself and every file a ref has read, by their paths from the repository's top. */
export class SpecFiles {
  readonly root: string;
  readonly #docs = new Map<string, unknown>();
  /** How many other files are kept, the least lately used dropped first (Fable's bound: 64). */
  readonly #keep: number;
  #index: SpecIndex | null = null;

  constructor(root: string, keep = 64) {
    this.root = root;
    this.#keep = keep;
  }

  get index(): SpecIndex | null {
    return this.#index;
  }

  /** Parses and checks the spec at `root`. */
  open(text: string): Opened {
    const parsed = parseDoc(text, this.root);
    if (!parsed.ok) return parsed;
    const checked = indexSpec(parsed.doc, writtenVersion(text));
    if (!checked.ok) return checked;
    this.#docs.set(this.root, parsed.doc);
    this.#index = checked.index;
    return checked;
  }

  /** Whether the file at `path` is held. */
  has(path: string): boolean {
    return this.#docs.has(path);
  }

  /** Takes another file a ref named: its parsed document, or why it could not be. */
  addFile(path: string, text: string): { ok: true } | { ok: false; error: string } {
    const parsed = parseDoc(text, path);
    if (!parsed.ok) return { ok: false, error: parsed.line === null ? parsed.error : `${parsed.error} (line ${parsed.line})` };
    this.#docs.delete(path);
    this.#docs.set(path, parsed.doc);
    // The spec itself stays; the rest go oldest first.
    while (this.#docs.size > this.#keep + 1) {
      for (const key of this.#docs.keys()) {
        if (key !== this.root) {
          this.#docs.delete(key);
          break;
        }
      }
    }
    return { ok: true };
  }

  /** Follows `ref` from the file `base`. */
  resolve(base: string, ref: string): Resolved {
    const target = refTarget(base, ref);
    if (!target.ok) return target;
    const doc = this.#docs.get(target.file);
    if (doc === undefined) return { need: target.file };
    // Used again: the newest.
    if (target.file !== this.root) {
      this.#docs.delete(target.file);
      this.#docs.set(target.file, doc);
    }
    const node = pointer(doc, target.pointer);
    if (node === undefined) return { ok: false, error: `${target.pointer || "/"} is not in ${target.file}` };
    return { ok: true, node, base: target.file };
  }

  /** Operation `i` as the view draws it. */
  operation(i: number): Operation | null {
    const doc = this.#docs.get(this.root);
    const row = this.#index?.ops[i];
    if (!isObj(doc) || row === undefined) return null;
    let item = (doc.paths as Obj)[row.path];
    if (isObj(item) && typeof item.$ref === "string") {
      const r = this.resolve(this.root, item.$ref);
      item = "ok" in r && r.ok ? r.node : {};
    }
    if (!isObj(item)) return null;
    return buildOperation(this, doc, item, row, this.root);
  }

  /** The operations of a path item that lives in another file, each whole, once the view has read that file. */
  pathItem(ref: PathRef): { ok: true; ops: Operation[] } | { need: string } | { ok: false; error: string } {
    const doc = this.#docs.get(this.root);
    const r = this.resolve(this.root, ref.ref);
    if (!("ok" in r)) return r;
    if (!r.ok) return r;
    if (!isObj(r.node) || !isObj(doc)) return { ok: false, error: `${ref.ref} is not a path item` };
    const item = r.node;
    const ops: Operation[] = [];
    for (const method of METHODS) {
      const op = item[method];
      if (!isObj(op)) continue;
      const row: OpRow = {
        i: -1,
        method,
        path: ref.path,
        summary: str(op.summary),
        operationId: str(op.operationId) || null,
        tags: [],
        deprecated: op.deprecated === true,
        anchor: `${method} ${ref.path}`,
      };
      ops.push(buildOperation(this, doc, item, row, r.base, op));
    }
    return { ok: true, ops };
  }
}

/**
 * The `openapi` version as the text writes it: `openapi: 3.0` unquoted is the number 3 to YAML. Not `openapi-sniff.ts`,
 * which the app's start-up holds: importing it here would make it a chunk of its own that start-up fetches.
 */
function writtenVersion(text: string): string | undefined {
  return /^(["']?)openapi\1[ \t]*:[ \t]*(["']?)(\d+(?:\.\d+)*)/m.exec(text.slice(0, 64 * 1024))?.[3];
}

/** The document in `text`: JSON where it starts with `{`, else YAML, or the parser's error and its line. */
export function parseDoc(text: string, path: string): { ok: true; doc: unknown } | { ok: false; error: string; line: number | null } {
  const body = text.charCodeAt(0) === 0xfeff ? text.slice(1) : text;
  if (/^\s*[{[]/.test(body)) {
    try {
      return { ok: true, doc: JSON.parse(body) };
    } catch {
      // YAML reads JSON too, and says on which line it stopped, which WebKit's JSON.parse does not.
    }
  }
  try {
    return { ok: true, doc: load(body, { schema: JSON_SCHEMA, json: true, filename: path }) };
  } catch (cause) {
    const e = cause as { reason?: string; mark?: { line: number }; message?: string };
    return { ok: false, error: e.reason ?? e.message ?? String(cause), line: e.mark === undefined ? null : e.mark.line + 1 };
  }
}

/** The spec's index, or why it is not a spec knobas draws. */
export function indexSpec(doc: unknown, written?: string): Opened {
  if (!isObj(doc)) return { ok: false, error: "The file is not a mapping at its top, so it is not an OpenAPI spec.", line: null };
  const key = typeof doc.openapi === "string" || typeof doc.openapi === "number" ? "openapi" : "swagger";
  // `openapi: 3.0` unquoted reads as the number 3 in YAML; the version as written is what the text held.
  const version = typeof doc[key] === "number" && written !== undefined ? written : str(doc[key]);
  if (version === "") return { ok: false, error: "The file has no openapi or swagger version at its top.", line: null };
  if (key === "openapi" ? !/^3\.[01](\.|$)/.test(version) : version !== "2.0") {
    return { ok: false, error: `knobas draws OpenAPI 3.0 and 3.1 and Swagger 2.0; this file says ${key}: ${version}.`, line: null };
  }
  const info = doc.info;
  if (!isObj(info)) return { ok: false, error: "The spec has no info object, which every spec must have.", line: null };
  if (doc.paths !== undefined && doc.paths !== null && !isObj(doc.paths)) {
    return { ok: false, error: "The spec's paths is not a mapping of paths to their operations.", line: null };
  }
  if (doc.paths === undefined && !(key === "openapi" && version.startsWith("3.1"))) {
    return { ok: false, error: "The spec has no paths, which OpenAPI 3.0 and Swagger 2.0 require.", line: null };
  }
  const problems: string[] = [];
  const tags: Tag[] = [];
  const seen = new Set<string>();
  if (Array.isArray(doc.tags)) {
    for (const t of doc.tags) {
      if (isObj(t) && typeof t.name === "string" && !seen.has(t.name)) {
        seen.add(t.name);
        tags.push({ name: t.name, description: str(t.description) });
      }
    }
  }
  const ops: OpRow[] = [];
  const pathRefs: PathRef[] = [];
  const paths = isObj(doc.paths) ? doc.paths : {};
  for (const path of Object.keys(paths)) {
    let item = paths[path];
    if (isObj(item) && typeof item.$ref === "string") {
      const ref = item.$ref;
      if (!ref.startsWith("#")) {
        pathRefs.push({ path, ref });
        continue;
      }
      item = pointer(doc, ref.slice(1));
    }
    if (!isObj(item)) {
      problems.push(`${path} is not a path item, so it is left out.`);
      continue;
    }
    for (const method of METHODS) {
      const op = item[method];
      if (op === undefined) continue;
      if (!isObj(op)) {
        problems.push(`${method.toUpperCase()} ${path} is not an operation, so it is left out.`);
        continue;
      }
      const opTags = Array.isArray(op.tags) ? op.tags.filter((t): t is string => typeof t === "string") : [];
      for (const t of opTags) {
        if (!seen.has(t)) {
          seen.add(t);
          tags.push({ name: t, description: "" });
        }
      }
      ops.push({
        i: ops.length,
        method,
        path,
        summary: str(op.summary),
        operationId: str(op.operationId) || null,
        tags: opTags,
        deprecated: op.deprecated === true,
        anchor: `${method} ${path}`,
      });
    }
  }
  const schemaBase = key === "swagger" ? "#/definitions/" : "#/components/schemas/";
  const named = key === "swagger" ? doc.definitions : isObj(doc.components) ? doc.components.schemas : undefined;
  return {
    ok: true,
    index: {
      key,
      version,
      title: str(info.title) || "Untitled API",
      apiVersion: str(info.version),
      description: str(info.description) || str(info.summary),
      servers: serversOf(doc),
      tags,
      ops,
      pathRefs,
      schemas: isObj(named) ? Object.keys(named) : [],
      schemaBase,
      problems,
    },
  };
}

function serversOf(doc: Obj): Server[] {
  if (Array.isArray(doc.servers)) {
    return doc.servers.filter(isObj).map((s) => ({ url: str(s.url), description: str(s.description) })).filter((s) => s.url !== "");
  }
  if (typeof doc.host === "string" || typeof doc.basePath === "string") {
    const schemes = Array.isArray(doc.schemes) && doc.schemes.length > 0 ? doc.schemes.map(str) : ["https"];
    return schemes.map((scheme) => ({ url: `${scheme}://${str(doc.host)}${str(doc.basePath)}`, description: "" }));
  }
  return [];
}

/** The value at JSON pointer `ptr` (`/components/schemas/Pet`, `""` the whole) in `doc`, or `undefined`. */
export function pointer(doc: unknown, ptr: string): unknown {
  if (ptr === "" || ptr === "/") return doc;
  let at = doc;
  for (const raw of ptr.replace(/^\//, "").split("/")) {
    const part = decodeURIComponent(raw).replace(/~1/g, "/").replace(/~0/g, "~");
    if (Array.isArray(at)) at = at[Number(part)];
    else if (isObj(at)) at = Object.prototype.hasOwnProperty.call(at, part) ? at[part] : undefined;
    else return undefined;
    if (at === undefined) return undefined;
  }
  return at;
}

/** Whether `ref` names a URL: never read, only named. */
export function isUrlRef(ref: string): boolean {
  return /^[a-z][a-z0-9+.-]*:/i.test(ref);
}

/** The file and pointer `ref` names from the file `base`: a path from the repository's top, normalized. */
export function refTarget(base: string, ref: string): { ok: true; file: string; pointer: string } | { ok: false; error: string } {
  if (isUrlRef(ref)) return { ok: false, error: `${ref} is outside the repository; knobas does not fetch it.` };
  const hash = ref.indexOf("#");
  const filePart = hash < 0 ? ref : ref.slice(0, hash);
  const ptr = hash < 0 ? "" : ref.slice(hash + 1);
  if (filePart === "") return { ok: true, file: base, pointer: ptr };
  const dir = base.includes("/") ? base.slice(0, base.lastIndexOf("/")) : "";
  const joined = filePart.startsWith("/") ? filePart.slice(1) : dir === "" ? filePart : `${dir}/${filePart}`;
  const out: string[] = [];
  for (const seg of joined.split("/")) {
    if (seg === "" || seg === ".") continue;
    if (seg === "..") {
      if (out.length === 0) return { ok: false, error: `${ref} points outside the repository.` };
      out.pop();
    } else out.push(seg);
  }
  return { ok: true, file: out.join("/"), pointer: ptr };
}

/** A node that is a `{ $ref }`: its ref, else `null`. */
export function refOf(node: unknown): string | null {
  return isObj(node) && typeof node.$ref === "string" ? node.$ref : null;
}

/** Follows refs inside the node's own file, a few deep, so a parameter or response named once in `components` reads whole. */
function local(files: SpecFiles, base: string, node: unknown): { node: unknown; ref: string | null } {
  let at = node;
  for (let hops = 0; hops < 8; hops++) {
    const ref = refOf(at);
    if (ref === null) return { node: at, ref: null };
    const r = files.resolve(base, ref);
    if (!("ok" in r) || !r.ok || r.base !== base) return { node: at, ref };
    at = r.node;
  }
  return { node: at, ref: refOf(at) };
}

function examplesOf(files: SpecFiles, base: string, holder: Obj): Example[] {
  const out: Example[] = [];
  if (holder.example !== undefined) out.push({ name: "", summary: "", value: holder.example, external: null });
  if (isObj(holder.examples)) {
    for (const [name, raw] of Object.entries(holder.examples)) {
      const ex = local(files, base, raw).node;
      if (!isObj(ex)) continue;
      out.push({ name, summary: str(ex.summary), value: ex.value, external: typeof ex.externalValue === "string" ? ex.externalValue : null });
    }
  }
  return out;
}

function contentOf(files: SpecFiles, base: string, content: unknown): Media[] {
  if (!isObj(content)) return [];
  return Object.entries(content).map(([type, raw]) => {
    const m = isObj(raw) ? raw : {};
    return { type, schema: m.schema ?? null, examples: examplesOf(files, base, m) };
  });
}

function headersOf(files: SpecFiles, base: string, headers: unknown, swagger: boolean): Header[] {
  if (!isObj(headers)) return [];
  return Object.entries(headers).map(([name, raw]) => {
    const h = local(files, base, raw).node;
    const o = isObj(h) ? h : {};
    return { name, description: str(o.description), schema: swagger ? swaggerSchema(o) : (o.schema ?? null) };
  });
}

/** A 2.0 parameter's or header's own `type`, `format`, `items` and `enum` as a schema. */
function swaggerSchema(o: Obj): unknown {
  if (o.schema !== undefined) return o.schema;
  const schema: Obj = {};
  for (const k of ["type", "format", "items", "enum", "default", "minimum", "maximum", "pattern", "collectionFormat"]) {
    if (o[k] !== undefined) schema[k] = o[k];
  }
  return Object.keys(schema).length === 0 ? null : schema;
}

function buildOperation(files: SpecFiles, doc: Obj, item: Obj, row: OpRow, base: string, given?: Obj): Operation {
  const op = given ?? (item[row.method] as Obj);
  const swagger = typeof doc.swagger === "string";
  // The path's parameters, then the operation's, which replace one of the same name and place.
  const merged = new Map<string, Param>();
  let body: Body | null = null;
  const consumes = (Array.isArray(op.consumes) ? op.consumes : Array.isArray(doc.consumes) ? doc.consumes : ["application/json"]).map(str);
  for (const list of [item.parameters, op.parameters]) {
    if (!Array.isArray(list)) continue;
    for (const raw of list) {
      const { node, ref } = local(files, base, raw);
      if (ref !== null && refOf(node) !== null) {
        merged.set(`ref:${ref}`, { name: ref, in: "", required: false, deprecated: false, description: "", schema: null, example: undefined, ref });
        continue;
      }
      if (!isObj(node)) continue;
      if (swagger && node.in === "body") {
        body = {
          description: str(node.description),
          required: node.required === true,
          content: consumes.map((type) => ({ type, schema: node.schema ?? null, examples: [] })),
          ref: null,
        };
        continue;
      }
      const param: Param = {
        name: str(node.name),
        in: str(node.in),
        required: node.required === true || node.in === "path",
        deprecated: node.deprecated === true,
        description: str(node.description),
        schema: swagger ? swaggerSchema(node) : (node.schema ?? (isObj(node.content) ? (Object.values(node.content).find(isObj)?.schema ?? null) : null)),
        example: node.example ?? (isObj(node.examples) ? (Object.values(node.examples).map((e) => local(files, base, e).node).find(isObj)?.value ?? undefined) : undefined),
        ref: null,
      };
      merged.set(`${param.in}:${param.name}`, param);
    }
  }
  if (!swagger && op.requestBody !== undefined) {
    const { node, ref } = local(files, base, op.requestBody);
    body =
      ref !== null && refOf(node) !== null
        ? { description: "", required: false, content: [], ref }
        : isObj(node)
          ? { description: str(node.description), required: node.required === true, content: contentOf(files, base, node.content), ref: null }
          : null;
  }
  const produces = (Array.isArray(op.produces) ? op.produces : Array.isArray(doc.produces) ? doc.produces : ["application/json"]).map(str);
  const responses: Response[] = [];
  if (isObj(op.responses)) {
    for (const [status, raw] of Object.entries(op.responses)) {
      const { node, ref } = local(files, base, raw);
      if (ref !== null && refOf(node) !== null) {
        responses.push({ status, description: "", content: [], headers: [], ref });
        continue;
      }
      if (!isObj(node)) continue;
      let content: Media[];
      if (swagger) {
        const examples = isObj(node.examples) ? node.examples : {};
        content =
          node.schema === undefined && Object.keys(examples).length === 0
            ? []
            : produces.map((type) => ({
                type,
                schema: node.schema ?? null,
                examples: examples[type] === undefined ? [] : [{ name: "", summary: "", value: examples[type], external: null }],
              }));
      } else content = contentOf(files, base, node.content);
      responses.push({ status, description: str(node.description), content, headers: headersOf(files, base, node.headers, swagger), ref: null });
    }
  }
  const security = (Array.isArray(op.security) ? op.security : Array.isArray(doc.security) ? doc.security : [])
    .filter(isObj)
    .map((req) => Object.keys(req).join(" + "))
    .filter((s) => s !== "");
  return {
    i: row.i,
    method: row.method,
    path: row.path,
    summary: str(op.summary),
    description: str(op.description),
    operationId: str(op.operationId) || null,
    deprecated: op.deprecated === true,
    parameters: [...merged.values()],
    body,
    responses,
    security,
    base,
  };
}
