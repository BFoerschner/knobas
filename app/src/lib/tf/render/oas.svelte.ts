/**
 * What the OpenAPI view's parts share (ux18): the spec's session, the refs resolved so far, which nodes are open,
 * and how a ref is followed. Held by `TfOpenApi` for one spec; its schema and operation parts read it.
 *
 * Every node that opens has its refs resolved first and opens with them (`openNode`): the worker answers within a
 * frame, so a node opens once, whole, and nothing under it grows after it drew. A ref to another file is the one
 * exception: it opens at once on *Reading ⟨file⟩…* and fills in below that line, which moves nothing above it.
 */
import { SvelteMap, SvelteSet } from "svelte/reactivity";

import { refTarget, type Resolved } from "../../render/openapi";
import { namedSchema, refName } from "../../render/openapi-view";
import type { SpecSession } from "../../render/openapi-port";

type Obj = Record<string, unknown>;
const isObj = (v: unknown): v is Obj => typeof v === "object" && v !== null && !Array.isArray(v);

/** What a ref draws as right now: resolved, being read from another file, or why not. */
export type RefState = Resolved | { reading: string };

export class OasState {
  readonly session: SpecSession;
  readonly root: string;
  schemaBase = $state("#/components/schemas/");
  schemas = $state<ReadonlySet<string>>(new Set());
  /** Refs resolved or being read, by `base\nref`. */
  readonly refs = new SvelteMap<string, RefState>();
  /** The nodes open: a property, a ref, a media type picked; by the key their part builds. */
  readonly open = new SvelteSet<string>();
  /** The media type picked where a body or response has several, by its key. */
  readonly picked = new SvelteMap<string, string>();
  /** Goes to named schema `name`, from the row `from` (for *Back to*). */
  follow: (name: string, from: { k: string; label: string }) => void = () => {};
  /** A file of the repository on the Code page, at a place in it where it is a spec. */
  fileHref: (file: string, place: string | null) => string | null = () => null;

  constructor(session: SpecSession, root: string) {
    this.session = session;
    this.root = root;
  }

  /** The ref as it draws now, or `undefined` before anything asked for it. */
  ref(base: string, ref: string): RefState | undefined {
    return this.refs.get(`${base}\n${ref}`);
  }

  /** Resolves `ref` from `base` into `refs`. A ref into another file shows *Reading* until its file is read. */
  async ensure(base: string, ref: string): Promise<void> {
    const key = `${base}\n${ref}`;
    const held = this.refs.get(key);
    if (held !== undefined && !("reading" in held)) return;
    const target = refTarget(base, ref);
    if (target.ok && target.file !== base && held === undefined) this.refs.set(key, { reading: target.file });
    const done = await this.session.resolve(base, ref);
    this.refs.set(key, done);
  }

  /**
   * Opens node `key` once the refs it shows are resolved: the node's own (`node` a ref, or an array of one) and its
   * properties' one-line refs. A ref into another file opens at once and reads below its line.
   */
  async openNode(key: string, node: unknown, base: string): Promise<void> {
    const own = innerRef(node);
    if (own !== null) {
      const target = refTarget(base, own);
      if (target.ok && target.file !== base) {
        this.open.add(key);
        await this.ensure(base, own);
        return;
      }
      await this.ensure(base, own);
    }
    this.open.add(key);
  }

  toggle(key: string, node: unknown, base: string): void {
    if (this.open.has(key)) this.open.delete(key);
    else void this.openNode(key, node, base);
  }

  /** The named schema of the spec a ref lands on, so its name can be followed to it. */
  named(base: string, ref: string): string | null {
    return base === this.root ? namedSchema(ref, this.schemaBase, this.schemas) : null;
  }

  /** Where a ref into another file opens on the Code page: that file, at the named schema where it names one. */
  hrefOf(base: string, ref: string): string | null {
    const target = refTarget(base, ref);
    if (!target.ok || target.file === base) return null;
    const m = /^\/(?:components\/schemas|definitions)\/([^/]+)$/.exec(target.pointer);
    return this.fileHref(target.file, m === null ? null : `schemas/${refName(ref)}`);
  }
}

/** The ref a node opens onto: its own, or its items' where it is an array of one. */
export function innerRef(node: unknown): string | null {
  if (!isObj(node)) return null;
  if (typeof node.$ref === "string") return node.$ref;
  if (isObj(node.items) && typeof node.items.$ref === "string") return node.items.$ref;
  return null;
}
