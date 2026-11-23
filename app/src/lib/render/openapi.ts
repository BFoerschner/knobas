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
import { specChanges, type SpecChanges } from "./openapi-changes";
import { JSON_SCHEMA, load } from "js-yaml";

import { makeExample, type Direction } from "./openapi-example";
import { pathAtLine, placeOfPath } from "./openapi-line";
import { collection as bruCollection } from "./openapi-bru";
import { firstSchemeOf, startDraft, type RequestDraft } from "./openapi-request";
import { groupsOf } from "./openapi-view";
import { fastYaml } from "./yaml-fast";

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
  /** A 3.1 webhook: `path` is its name, and it is never sent (ux31). */
  webhook?: true;
}

/** A path whose item lives in another file (`$ref: ./paths/pets.yaml`): listed by its path, read when opened. */
export interface PathRef {
  path: string;
  ref: string;
}

export interface ServerVariable {
  default: string;
  enum: string[];
  description: string;
}

export interface Server {
  url: string;
  description: string;
  /** A `{variable}` of the address, its default and choices (ux31). */
  variables: Record<string, ServerVariable>;
}

/** A security scheme (`components.securitySchemes`, 2.0's `securityDefinitions`), as the overview and a request read it. */
export interface Scheme {
  name: string;
  /** `http`, `apiKey`, `oauth2`, `openIdConnect`, `mutualTLS`; 2.0's `basic` reads as `http` with scheme `basic`. */
  type: string;
  /** An `http` scheme's own: `bearer`, `basic`. */
  scheme: string;
  /** Where an API key goes: `header`, `query` or `cookie`. */
  in: string;
  /** An API key's own name (`X-API-Key`). */
  keyName: string;
  bearerFormat: string;
  description: string;
  /** An OAuth 2.0 scheme's flows by name (`authorizationCode`). */
  flows: string[];
}

export interface Contact {
  name: string;
  email: string;
  url: string;
}

export interface Info {
  contact: Contact | null;
  license: { name: string; url: string } | null;
  terms: string;
  externalDocs: { url: string; description: string } | null;
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
  /** The security schemes, in the spec's order (ux31). */
  security: Scheme[];
  /** Contact, licence, terms and the external docs (ux31). */
  info: Info;
  /** 3.1's `webhooks`, one row each, numbered after `ops` (ux31). */
  webhooks: OpRow[];
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

export interface Link {
  name: string;
  operationId: string;
  operationRef: string;
  description: string;
}

export interface Response {
  status: string;
  description: string;
  content: Media[];
  headers: Header[];
  /** What may follow (`links`), each naming an operation (ux31). */
  links: Link[];
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
  /** The requirements it accepts, any one of them: each the scheme names it needs together; `[]` for *or nothing*. */
  security: string[][];
  /** Where it goes: its own servers, else its path item's, else the spec's (ux31). */
  servers: Server[];
  /** Its callbacks, by name. */
  callbacks: string[];
  externalDocs: { url: string; description: string } | null;
  /** A 3.1 webhook: drawn with no request. */
  webhook: boolean;
  /** The file the operation's refs are read from. */
  base: string;
}

/** What a ref resolves to: the node and its file, a file to read first, or why it cannot be followed. */
export type Resolved = { ok: true; node: unknown; base: string } | { need: string } | { ok: false; error: string };

type Obj = Record<string, unknown>;

const isObj = (v: unknown): v is Obj => typeof v === "object" && v !== null && !Array.isArray(v);
const str = (v: unknown): string => (typeof v === "string" ? v : typeof v === "number" || typeof v === "boolean" ? String(v) : "");

/** One property of a {@link Shape}: its schema as the spec writes it (a ref stays a ref), and its words. */
export interface Prop {
  name: string;
  node: unknown;
  required: boolean;
  /** Its own description, else the description of the schema its ref names in the same file. */
  words: string;
}

/** A choice among schemas (`oneOf`, `anyOf`), the reader picks one. */
export interface Variants {
  key: "oneOf" | "anyOf";
  list: unknown[];
  /** The property that tells them apart, where the spec names one. */
  discriminator: string;
}

/**
 * What a schema holds, as a reader wants it (ux31, section 8): `allOf` merged into one list of properties in the
 * parts' order with their `required` lists joined and the first description kept; the first `oneOf` or `anyOf` of
 * more than one schema a choice; a choice of one (or one and null) that one. Refs in the node's own file are
 * followed to merge; refs inside the properties stay refs.
 */
export interface Shape {
  props: Prop[];
  variants: Variants | null;
  /** What each item of an array is. */
  items: unknown;
  /** Any other key (`additionalProperties`). */
  extra: unknown;
  description: string;
  /** The merged node's own facts (enum, default, bounds, pattern), for the page's line of facts. */
  facts: Record<string, unknown>;
  /** The file the shape's refs are read from. */
  base: string;
}

/** Which operations name a schema, by `ops` index, and which named schemas hold it: direct uses only. */
export interface Uses {
  ops: number[];
  schemas: string[];
}

/** A page's first screen in one answer (section 29): an operation and the shapes it shows, or a schema's page. */
export type PageAnswer =
  | { kind: "op"; op: Operation; shapes: Record<string, Shape>; samples: Record<string, unknown> }
  | { kind: "schema"; name: string; node: unknown; base: string; shape: Shape; uses: Uses; sample: unknown; given: boolean }
  | { kind: "missing" };

/** One call of the view to where its spec is kept (`openapi-port.ts`). */
export type SpecCall =
  | { kind: "open"; path: string; text: string }
  | { kind: "close" }
  | { kind: "operation"; i: number }
  | { kind: "pathItem"; ref: PathRef }
  | { kind: "resolve"; base: string; ref: string }
  | { kind: "addFile"; path: string; text: string }
  | { kind: "shape"; base: string; node: unknown }
  | { kind: "uses"; name: string }
  | { kind: "sample"; base: string; node: unknown; direction?: Direction; choice?: number }
  | { kind: "page"; place: string }
  | { kind: "placeAt"; line: number }
  | { kind: "collection"; specFile: string }
  | { kind: "changes"; path: string; before: string };

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
  if (call.kind === "shape") return files.shape(call.base, call.node);
  if (call.kind === "uses") return files.uses(call.name);
  if (call.kind === "sample") return files.sample(call.base, call.node, call.direction ?? "any", call.choice ?? 0);
  if (call.kind === "page") return files.page(call.place);
  if (call.kind === "placeAt") return files.placeAt(call.line);
  if (call.kind === "collection") return files.collection(call.specFile);
  if (call.kind === "changes") return files.changes(call.path, call.before);
  return files.addFile(call.path, call.text);
}

/** The files of one spec: the spec itself and every file a ref has read, by their paths from the repository's top. */
export class SpecFiles {
  readonly root: string;
  readonly #docs = new Map<string, unknown>();
  /** How many other files are kept, the least lately used dropped first (Fable's bound: 64). */
  readonly #keep: number;
  #index: SpecIndex | null = null;
  /** The spec's text, for a line's place (`placeAt`). */
  #text = "";

  constructor(root: string, keep = 64) {
    this.root = root;
    this.#keep = keep;
  }

  get index(): SpecIndex | null {
    return this.#index;
  }

  /** The spec's own parsed document. */
  get doc(): unknown {
    return this.#docs.get(this.root);
  }

  /** Parses and checks the spec at `root`; a text read lately is not parsed again (`recent`). */
  open(text: string): Opened {
    const parsed = recentParse(text, this.root);
    if (!parsed.ok) return parsed;
    const checked = indexOf(parsed.doc, text);
    if (!checked.ok) return checked;
    this.#docs.set(this.root, parsed.doc);
    this.#index = checked.index;
    this.#text = text;
    return checked;
  }

  /**
   * What changed in the API from the spec `before` (its text, at `path`) to this one (section 13): read once, the
   * comparison's other side having parsed it lately (`recent`).
   */
  changes(path: string, before: string): { ok: true; changes: SpecChanges } | { ok: false; error: string } {
    const parsed = recentParse(before, path);
    if (!parsed.ok) return { ok: false, error: parsed.error };
    return { ok: true, changes: specChanges(parsed.doc, this.doc) };
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

  /** Operation `i` as the view draws it: an operation of `ops`, or a webhook numbered after them. */
  operation(i: number): Operation | null {
    const doc = this.#docs.get(this.root);
    const index = this.#index;
    if (!isObj(doc) || index === null) return null;
    const row = i < index.ops.length ? index.ops[i] : index.webhooks[i - index.ops.length];
    if (row === undefined) return null;
    let item = row.webhook === true ? (isObj(doc.webhooks) ? doc.webhooks[row.path] : undefined) : (doc.paths as Obj)[row.path];
    if (isObj(item) && typeof item.$ref === "string") {
      const r = this.resolve(this.root, item.$ref);
      item = "ok" in r && r.ok ? r.node : {};
    }
    if (!isObj(item)) return null;
    return buildOperation(this, doc, item, row, this.root);
  }

  /** Follows refs inside `base` itself, a few deep: the node a same-file ref names, or the node as it is. */
  #local(base: string, node: unknown): unknown {
    return local(this, base, node).node;
  }

  /** What schema `node` of file `base` holds (section 8): `allOf` merged, a choice told, refs in its own file followed. */
  shape(base: string, node: unknown): Shape {
    const out: Shape = { props: [], variants: null, items: undefined, extra: undefined, description: "", facts: {}, base };
    const required = new Set<string>();
    const seen = new Set<string>();
    const take = (raw: unknown, depth: number) => {
      if (depth > 12) return;
      let n = raw;
      const ref = refOf(n);
      if (ref !== null) {
        if (seen.has(ref)) return;
        seen.add(ref);
        const r = this.resolve(base, ref);
        // A ref into another file, or a broken one, stays what it is: the view reads it when opened.
        if (!("ok" in r) || !r.ok || r.base !== base) return;
        n = r.node;
      }
      if (!isObj(n)) return;
      if (out.description === "" && typeof n.description === "string") out.description = n.description;
      for (const k of FACT_KEYS) if (n[k] !== undefined && out.facts[k] === undefined) out.facts[k] = n[k];
      if (Array.isArray(n.required)) for (const r of n.required) required.add(str(r));
      if (isObj(n.properties)) {
        for (const [name, schema] of Object.entries(n.properties)) {
          const had = out.props.find((p) => p.name === name);
          if (had === undefined) out.props.push({ name, node: schema, required: false, words: "" });
          // A property two parts define is both: GitHub's forkee says `created_at` is `oneOf [integer, string]`, then `string`.
          else had.node = { allOf: [had.node, schema] };
        }
      }
      for (const key of ["oneOf", "anyOf"] as const) {
        const list = n[key];
        if (!Array.isArray(list)) continue;
        // A choice of one schema and null, or of one, is that schema.
        const real = list.filter((m) => !isNullSchema(this.#local(base, m)));
        if (real.length === 1) take(real[0], depth + 1);
        else if (real.length > 1 && out.variants === null) {
          out.variants = { key, list: real, discriminator: isObj(n.discriminator) ? str(n.discriminator.propertyName) : "" };
        }
      }
      if (n.items !== undefined && out.items === undefined) out.items = n.items;
      if (n.additionalProperties !== undefined && typeof n.additionalProperties === "object" && out.extra === undefined) out.extra = n.additionalProperties;
      if (Array.isArray(n.allOf)) for (const part of n.allOf) take(part, depth + 1);
    };
    take(node, 0);
    for (const p of out.props) {
      p.required = required.has(p.name);
      const own = isObj(p.node) && typeof p.node.description === "string" ? p.node.description : "";
      if (own !== "") p.words = own;
      else {
        const named = this.#local(base, p.node);
        p.words = isObj(named) ? str(named.description) : "";
      }
    }
    return out;
  }

  /** Which operations name schema `name`, and which schemas hold it: the index is built once, by one pass. */
  uses(name: string): Uses {
    this.#uses ??= buildUses(this);
    return { ops: this.#uses.ops.get(name) ?? [], schemas: this.#uses.schemas.get(name) ?? [] };
  }
  #uses: { ops: Map<string, number[]>; schemas: Map<string, string[]> } | null = null;

  /**
   * An example schema `node` allows, made from its shape where it names none (`openapi-example.ts`), deterministic.
   * `name` is the name it stands under where it is no property's: a parameter's.
   */
  sample(base: string, node: unknown, direction: Direction = "any", choice = 0, given = true, name?: string): unknown {
    const reader = {
      resolve: (b: string, ref: string) => {
        const r = this.resolve(b, ref);
        return "ok" in r && r.ok ? { node: r.node, base: r.base } : null;
      },
      shape: (b: string, n: unknown) => this.shape(b, n),
    };
    return makeExample(reader, base, node, { direction, choice, given, name });
  }

  /**
   * The spec as a Bruno collection (`openapi-bru.ts`): each operation's request as it starts in the explorer, its
   * body the spec's example else one made from the schema (as a request). Its files, by path from its folder.
   */
  collection(specFile: string): [string, string][] {
    const index = this.#index;
    if (index === null) return [];
    const made = new Map<number, { op: Operation; draft: RequestDraft } | null>();
    const operation = (i: number) => {
      if (!made.has(i)) {
        const op = this.operation(i);
        if (op === null) made.set(i, null);
        else {
          const bodies: Record<string, unknown> = {};
          for (const m of op.body?.content ?? []) bodies[m.type] = m.examples[0]?.value ?? (m.schema === null ? undefined : this.sample(op.base, m.schema, "request"));
          const resolve = (schema: unknown) => this.#local(op.base, schema);
          made.set(i, { op, draft: startDraft(op, bodies, firstSchemeOf(op, index.security), resolve) });
        }
      }
      return made.get(i)!;
    };
    return bruCollection({ index, groups: groupsOf(index), operation, specFile });
  }

  /** The place line `line` (1-based) of the spec's text stands in: an operation, a schema, or `null` (`openapi-line.ts`). */
  placeAt(line: number): string | null {
    const index = this.#index;
    if (index === null || line < 1) return null;
    return placeOfPath(pathAtLine(this.#text, line), (p) => index.ops.find((o) => o.path === p)?.anchor ?? null);
  }

  /** A page's first screen in one answer: `get /pets` an operation, `schemas/Pet` a schema. */
  page(place: string): PageAnswer {
    const index = this.#index;
    if (index === null) return { kind: "missing" };
    if (place.startsWith("schemas/")) {
      const name = place.slice("schemas/".length);
      if (!index.schemas.includes(name)) return { kind: "missing" };
      const r = this.resolve(this.root, index.schemaBase + encodePointer(name));
      if (!("ok" in r) || !r.ok) return { kind: "missing" };
      // The sample starts from the ref, so a schema that holds itself closes where it comes round again.
      const sample = this.sample(this.root, { $ref: index.schemaBase + encodePointer(name) });
      const given = isObj(r.node) && (r.node.example !== undefined || (Array.isArray(r.node.examples) && r.node.examples.length > 0));
      return { kind: "schema", name, node: r.node, base: r.base, shape: this.shape(r.base, r.node), uses: this.uses(name), sample, given };
    }
    const row = index.ops.find((o) => o.anchor === place) ?? index.webhooks.find((o) => o.anchor === place);
    if (row === undefined) return { kind: "missing" };
    const op = this.operation(row.i);
    if (op === null) return { kind: "missing" };
    return { kind: "op", op, shapes: this.pageShapes(op), samples: this.pageSamples(op) };
  }

  /**
   * The examples an operation's page makes where the spec gives none (Björn, 2026-11-22): each body's media (as a
   * request), each response's media (as an answer), each parameter (as a request), by the same keys as the shapes.
   */
  pageSamples(op: Operation): Record<string, unknown> {
    const out: Record<string, unknown> = {};
    for (const m of op.body?.content ?? []) if (m.examples.length === 0 && m.schema !== null) out[`b:${m.type}`] = this.sample(op.base, m.schema, "request");
    for (const r of op.responses) for (const m of r.content) if (m.examples.length === 0 && m.schema !== null) out[`r:${r.status}:${m.type}`] = this.sample(op.base, m.schema, "response");
    for (const p of op.parameters) if (p.ref === null && p.example === undefined && p.schema !== null) out[`p:${p.in}:${p.name}`] = this.sample(op.base, p.schema, "request", 0, true, p.name);
    return out;
  }

  /** The shapes an operation's page draws unopened: each body's and each response's media, and its items or first choice. */
  pageShapes(op: Operation): Record<string, Shape> {
    const shapes: Record<string, Shape> = {};
    const add = (key: string, node: unknown) => {
      if (node === null || node === undefined) return;
      const sh = this.shape(op.base, node);
      shapes[key] = sh;
      if (sh.props.length === 0 && sh.variants === null && sh.items !== undefined) shapes[`${key}/[]`] = this.shape(op.base, sh.items);
      if (sh.variants !== null) shapes[`${key}|0`] = this.shape(op.base, sh.variants.list[0]);
    };
    for (const m of op.body?.content ?? []) add(`b:${m.type}`, m.schema);
    for (const r of op.responses) for (const m of r.content) add(`r:${r.status}:${m.type}`, m.schema);
    return shapes;
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

/**
 * The specs parsed lately, by their text, the newest last: going back to a spec (another file, then this one again)
 * does not parse it again. Documents are only read once parsed, so sessions share them.
 */
const recent = new Map<string, { ok: true; doc: unknown }>();
const RECENT = 4;

/**
 * A recent document's index, by the document: the same text is the same document (`recent`), so going back to a spec
 * neither parses it nor walks its paths again. An index is only read once made.
 */
const indexed = new WeakMap<object, Opened>();

function indexOf(doc: unknown, text: string): Opened {
  const held = isObj(doc) ? indexed.get(doc) : undefined;
  if (held !== undefined) return held;
  const made = indexSpec(doc, writtenVersion(text));
  if (isObj(doc)) indexed.set(doc, made);
  return made;
}

function recentParse(text: string, path: string): ReturnType<typeof parseDoc> {
  const held = recent.get(text);
  if (held !== undefined) {
    recent.delete(text);
    recent.set(text, held);
    return held;
  }
  const parsed = parseDoc(text, path);
  if (parsed.ok) {
    recent.set(text, parsed);
    if (recent.size > RECENT) recent.delete(recent.keys().next().value!);
  }
  return parsed;
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
  // The YAML spec tools write, read fast; anything else, and every broken file, by js-yaml (`yaml-fast.ts`).
  const fast = fastYaml(body);
  if (fast !== undefined) return { ok: true, doc: fast };
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
  const webhooks: OpRow[] = [];
  if (isObj(doc.webhooks)) {
    for (const [name, raw] of Object.entries(doc.webhooks)) {
      const item = isObj(raw) && typeof raw.$ref === "string" && raw.$ref.startsWith("#") ? pointer(doc, raw.$ref.slice(1)) : raw;
      if (!isObj(item)) continue;
      for (const method of METHODS) {
        const op = item[method];
        if (!isObj(op)) continue;
        webhooks.push({
          i: ops.length + webhooks.length,
          method,
          path: name,
          summary: str(op.summary),
          operationId: str(op.operationId) || null,
          tags: [],
          deprecated: op.deprecated === true,
          anchor: `${method} ${name}`,
          webhook: true,
        });
      }
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
      servers: serversOf(doc, doc),
      security: schemesOf(doc),
      info: infoOf(doc, info),
      webhooks,
      tags,
      ops,
      pathRefs,
      schemas: isObj(named) ? Object.keys(named) : [],
      schemaBase,
      problems,
    },
  };
}

/**
 * The servers `holder` names (the spec, a path item or an operation), each with its `{variables}`. A 2.0 spec's
 * `host`, `basePath` and `schemes` are its servers; one with no `host` has a relative address, which needs a server
 * of the person's own.
 */
export function serversOf(doc: Obj, holder: Obj): Server[] {
  if (Array.isArray(holder.servers)) {
    return holder.servers
      .filter(isObj)
      .map((s) => ({ url: str(s.url), description: str(s.description), variables: variablesOf(s.variables) }))
      .filter((s) => s.url !== "");
  }
  if (holder === doc && (typeof doc.host === "string" || typeof doc.basePath === "string")) {
    if (typeof doc.host !== "string" || doc.host === "") return [{ url: str(doc.basePath) || "/", description: "", variables: {} }];
    const schemes = Array.isArray(doc.schemes) && doc.schemes.length > 0 ? doc.schemes.map(str) : ["https"];
    return schemes.map((scheme) => ({ url: `${scheme}://${str(doc.host)}${str(doc.basePath)}`, description: "", variables: {} }));
  }
  return [];
}

function variablesOf(raw: unknown): Record<string, ServerVariable> {
  const out: Record<string, ServerVariable> = {};
  if (!isObj(raw)) return out;
  for (const [name, v] of Object.entries(raw)) {
    if (!isObj(v)) continue;
    out[name] = { default: str(v.default), enum: Array.isArray(v.enum) ? v.enum.map(str) : [], description: str(v.description) };
  }
  return out;
}

/** The spec's security schemes, 3.x's `components.securitySchemes` or 2.0's `securityDefinitions`. */
function schemesOf(doc: Obj): Scheme[] {
  const raw = isObj(doc.components) && isObj(doc.components.securitySchemes) ? doc.components.securitySchemes : isObj(doc.securityDefinitions) ? doc.securityDefinitions : {};
  const out: Scheme[] = [];
  for (const [name, node] of Object.entries(raw)) {
    const s = isObj(node) && typeof node.$ref === "string" && node.$ref.startsWith("#") ? pointer(doc, node.$ref.slice(1)) : node;
    if (!isObj(s)) continue;
    const basic = s.type === "basic";
    out.push({
      name,
      type: basic ? "http" : str(s.type),
      scheme: basic ? "basic" : str(s.scheme).toLowerCase(),
      in: str(s.in),
      keyName: str(s.name),
      bearerFormat: str(s.bearerFormat),
      description: str(s.description),
      flows: isObj(s.flows) ? Object.keys(s.flows) : typeof s.flow === "string" ? [s.flow] : [],
    });
  }
  return out;
}

function infoOf(doc: Obj, info: Obj): Info {
  const c = info.contact;
  const l = info.license;
  const e = doc.externalDocs;
  return {
    contact: isObj(c) ? { name: str(c.name), email: str(c.email), url: str(c.url) } : null,
    license: isObj(l) ? { name: str(l.name), url: str(l.url) } : null,
    terms: str(info.termsOfService),
    externalDocs: isObj(e) && str(e.url) !== "" ? { url: str(e.url), description: str(e.description) } : null,
  };
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

/** The keys a field's line of facts reads (section 8). */
export const FACT_KEYS = [
  "type", "format", "enum", "const", "default", "example", "minimum", "maximum", "exclusiveMinimum", "exclusiveMaximum",
  "minLength", "maxLength", "minItems", "maxItems", "uniqueItems", "pattern", "readOnly", "writeOnly", "deprecated", "nullable",
] as const;

/** Whether a schema says only "null" (3.1's `type: "null"`, or an `enum` of only `null` or the empty string). */
function isNullSchema(node: unknown): boolean {
  if (!isObj(node)) return false;
  if (node.type === "null") return true;
  return Array.isArray(node.enum) && node.enum.length === 1 && (node.enum[0] === null || node.enum[0] === "");
}

/** A name as one part of a JSON pointer. */
export function encodePointer(name: string): string {
  return name.replace(/~/g, "~0").replace(/\//g, "~1");
}

/** The named schemas a value's refs name, each once: `#/components/schemas/Pet` and the like, read from its JSON. */
function namedIn(value: unknown, schemaBase: string, out: Set<string>, follow?: (ref: string) => unknown, seen = new Set<string>()): void {
  const walk = (v: unknown, depth: number) => {
    if (depth > 64) return;
    if (Array.isArray(v)) {
      for (const x of v) walk(x, depth + 1);
      return;
    }
    if (!isObj(v)) return;
    const ref = v.$ref;
    if (typeof ref === "string") {
      if (ref.startsWith(schemaBase)) {
        out.add(decodeURIComponent(ref.slice(schemaBase.length)).replace(/~1/g, "/").replace(/~0/g, "~"));
      } else if (follow !== undefined && ref.startsWith("#/") && !seen.has(ref)) {
        // A parameter, a response or a body kept in `components`: what it names, the operation names.
        seen.add(ref);
        walk(follow(ref), depth + 1);
      }
      return;
    }
    for (const x of Object.values(v)) walk(x, depth + 1);
  };
  walk(value, 0);
}

/** The *Used by* and *Part of* index of a spec: one pass over its operations and its named schemas. */
function buildUses(files: SpecFiles): { ops: Map<string, number[]>; schemas: Map<string, string[]> } {
  const ops = new Map<string, number[]>();
  const schemas = new Map<string, string[]>();
  const index = files.index;
  const doc = files.doc;
  if (index === null || !isObj(doc)) return { ops, schemas };
  const follow = (ref: string) => pointer(doc, ref.slice(1));
  const paths = isObj(doc.paths) ? doc.paths : {};
  for (const row of index.ops) {
    let item = paths[row.path];
    if (isObj(item) && typeof item.$ref === "string" && item.$ref.startsWith("#")) item = follow(item.$ref);
    if (!isObj(item)) continue;
    const names = new Set<string>();
    namedIn([item[row.method], item.parameters ?? null], index.schemaBase, names, follow);
    for (const name of names) {
      let list = ops.get(name);
      if (list === undefined) ops.set(name, (list = []));
      list.push(row.i);
    }
  }
  const named = index.key === "swagger" ? doc.definitions : isObj(doc.components) ? doc.components.schemas : undefined;
  if (isObj(named)) {
    for (const holder of index.schemas) {
      const names = new Set<string>();
      namedIn(named[holder], index.schemaBase, names);
      names.delete(holder);
      for (const name of names) {
        let list = schemas.get(name);
        if (list === undefined) schemas.set(name, (list = []));
        list.push(holder);
      }
    }
  }
  return { ops, schemas };
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
  const form: Obj[] = [];
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
      if (swagger && node.in === "formData") {
        form.push(node);
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
  // 2.0's `formData` parameters are a form body, as its body parameter is the request body.
  if (swagger && form.length > 0) {
    const file = form.some((p) => p.type === "file");
    const type = file ? "multipart/form-data" : (consumes.find((c) => c.includes("form")) ?? "application/x-www-form-urlencoded");
    const properties: Obj = {};
    for (const p of form) {
      const schema = swaggerSchema(p);
      properties[str(p.name)] = { ...(isObj(schema) ? schema : {}), ...(p.type === "file" ? { type: "string", format: "binary" } : {}), description: str(p.description) || undefined };
    }
    const required = form.filter((p) => p.required === true).map((p) => str(p.name));
    body = { description: "", required: required.length > 0, content: [{ type, schema: { type: "object", required, properties }, examples: [] }], ref: null };
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
        responses.push({ status, description: "", content: [], headers: [], links: [], ref });
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
      responses.push({ status, description: str(node.description), content, headers: headersOf(files, base, node.headers, swagger), links: linksOf(files, base, node.links), ref: null });
    }
  }
  // Each requirement is the schemes it needs together; an empty one (`{}`) is *or nothing*.
  const security = (Array.isArray(op.security) ? op.security : Array.isArray(doc.security) ? doc.security : []).filter(isObj).map((req) => Object.keys(req));
  const ownServers = Array.isArray(op.servers) ? serversOf(doc, op) : [];
  const itemServers = ownServers.length === 0 && Array.isArray(item.servers) ? serversOf(doc, item) : [];
  const servers = ownServers.length > 0 ? ownServers : itemServers.length > 0 ? itemServers : serversOf(doc, doc);
  const ext = op.externalDocs;
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
    servers,
    callbacks: isObj(op.callbacks) ? Object.keys(op.callbacks) : [],
    externalDocs: isObj(ext) && str(ext.url) !== "" ? { url: str(ext.url), description: str(ext.description) } : null,
    webhook: row.webhook === true,
    base,
  };
}

function linksOf(files: SpecFiles, base: string, raw: unknown): Link[] {
  if (!isObj(raw)) return [];
  const out: Link[] = [];
  for (const [name, l0] of Object.entries(raw)) {
    const l = local(files, base, l0).node;
    if (!isObj(l)) continue;
    out.push({ name, operationId: str(l.operationId), operationRef: str(l.operationRef), description: str(l.description) });
  }
  return out;
}
