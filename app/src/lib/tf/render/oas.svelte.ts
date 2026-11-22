/**
 * What the API explorer's parts share (ux31, `docs/decisions/2026-11-22-an-api-spec-is-explored-sent-from-and-opened-in-
 * a-client.md`): the spec's session and index, the rail's mode, folds and filter, the page shown and the pages the
 * worker has answered, the shapes a field's caret opened, what is open and picked on a page, and the way back.
 *
 * - **A pick is drawn at once** (section 29): `current` moves in the frame of the press, so the rail's pill does;
 *   the page column keeps the old page until the worker's one answer lands (`shown`), then swaps whole. Past
 *   200 ms it says *Reading…* in the old page's place.
 * - **The last 30 pages answered are kept**, so going back to an operation asks nothing.
 * - **What is open or picked on a page** (a caret, *Show all*, a media type, a status, an example, a choice) is kept
 *   by key while the spec is open, so an operation visited again is as it was left.
 */
import { SvelteMap, SvelteSet } from "svelte/reactivity";

import { refTarget, type OpRow, type PageAnswer, type Resolved, type Shape, type SpecIndex } from "../../render/openapi";
import { firstOpen, groupsOf, hasInside, namedSchema, pathTree, refName, type Dir, type Group, type RailMode } from "../../render/openapi-view";
import type { SpecSession } from "../../render/openapi-port";
import type { RequestDraft } from "../../render/openapi-request";

/** A send as the request column draws it: running, answered or failed, refused before it went. */
export type Sending =
  | { state: "running"; id: number; line: string }
  | { state: "done"; id: number; line: string; answer: unknown }
  | { state: "refused"; line: string; words: string };

type Obj = Record<string, unknown>;
const isObj = (v: unknown): v is Obj => typeof v === "object" && v !== null && !Array.isArray(v);

/** What a ref draws as right now: resolved, being read from another file, or why not. */
export type RefState = Resolved | { reading: string };

/** How many answered pages are kept (section 29). */
export const KEPT_PAGES = 30;
/** How deep *Expand all* and Alt-click open, and how many carets at most. */
export const OPEN_ALL_DEPTH = 4;
export const OPEN_ALL_MOST = 300;
/** After this long a page still asked for says *Reading…* (section 29). */
export const READING_AFTER_MS = 200;

/** A place in the explorer: `""` the overview, `get /pets` an operation, `schemas/Pet` a schema, `tags/pets` a fold. */
export type Place = string;

/**
 * Hosts a request that changes something was confirmed for, with *Don't ask again*: kept until knobas quits, for
 * every spec (Björn, 2026-11-22: once per host until restart). The one place the rule's memory lives.
 */
export const confirmedHosts = new SvelteSet<string>();

/** The overview's place. */
export const OVERVIEW: Place = "";

/** What a place names: the page it shows. A tag's place shows the overview. */
export function pageOf(place: Place): Place {
  return place.startsWith("tags/") ? OVERVIEW : place;
}

/** One step back: the page a followed link left, what it was called, and where its column stood. */
export interface Back {
  place: Place;
  label: string;
  scroll: number;
}

export class OasState {
  readonly session: SpecSession;
  readonly root: string;
  index = $state.raw<SpecIndex | null>(null);
  groups = $state.raw<Group[]>([]);
  /** The path tree, built the first time *Paths* is shown. */
  #tree: Dir | null = null;
  mode = $state<RailMode>("tags");
  /** The folds open, by key; replaced whole on each change, so the rail's rows read a plain set. */
  folds = $state.raw<ReadonlySet<string>>(new Set());
  /** The filter's words. */
  typed = $state("");
  /** The folds and the rail's scroll as they were when the filter took its first word, given back when it empties. */
  beforeFilter: { folds: ReadonlySet<string>; scroll: number } | null = null;
  /** The place picked: the rail's pill moves here at once. */
  current = $state<Place>(OVERVIEW);
  /** The page drawn, once its answer landed. */
  shown = $state.raw<{ place: Place; answer: PageAnswer | null }>({ place: OVERVIEW, answer: null });
  /** A place an address named that this version of the file does not have. */
  gone = $state<Place | null>(null);
  /** Whether the page asked for has taken long enough to say so. */
  reading = $state(false);
  /** Pages answered, the newest last. */
  readonly #pages = new Map<Place, PageAnswer>();
  /** The way back from followed links. */
  backs = $state<Back[]>([]);
  /** Shapes a caret opened, by its key. */
  readonly shapes = new SvelteMap<string, Shape>();
  /** Examples made from a schema where the spec gives none, by the key of the part they stand in. */
  readonly samples = new SvelteMap<string, unknown>();
  /** What a person typed in each operation's request, by its place, while the spec is open (section 14). */
  readonly drafts = new SvelteMap<Place, RequestDraft>();
  /** A value typed for a parameter, offered for the same name on the next operation of the same server. */
  readonly sticky = new Map<string, string>();
  /** Each operation's last answer, by its place. */
  readonly answers = new SvelteMap<Place, Sending>();
  /** Refs resolved or being read, by `base\nref`: a ref into another file. */
  readonly refs = new SvelteMap<string, RefState>();
  /** Carets open, *Show all* pressed: by key. */
  readonly expand = new SvelteSet<string>();
  /** A media type, status, example or choice picked, by key. */
  readonly picked = new SvelteMap<string, string>();
  /** The row the rail's keys stand on. */
  cursor = $state<string | null>(null);
  /** A file of the repository on the Code page, at a place in it. */
  fileHref: (file: string, place: string | null) => string | null = () => null;
  #asked = 0;

  constructor(session: SpecSession, root: string) {
    this.session = session;
    this.root = root;
  }

  /** Takes the spec's index: its groups, and the folds open at first. */
  take(index: SpecIndex): void {
    this.index = index;
    this.groups = groupsOf(index);
    this.#tree = null;
    // The path tree waits for *Paths*: a 5,000-operation spec's open does not build it on the page's thread.
    this.folds = firstOpen(index, this.groups, null);
  }

  /** The path tree, built the first time it is asked for. */
  get tree(): Dir {
    const index = this.index;
    if (this.#tree === null) this.#tree = pathTree(index === null ? [] : index.ops);
    return this.#tree;
  }

  /** Shows *Paths*: the tree built, the first time, with its first folds open in the same frame. */
  showPaths(): void {
    const first = this.#tree === null;
    const tree = this.tree;
    if (first && this.index !== null) {
      const more = [...firstOpen(this.index, [], tree)].filter((k) => k.startsWith("p:"));
      if (more.length > 0) this.folds = new Set([...this.folds, ...more]);
    }
    this.mode = "paths";
  }

  get schemaSet(): ReadonlySet<string> {
    return this.#schemaSet ??= new Set(this.index?.schemas ?? []);
  }
  #schemaSet: ReadonlySet<string> | null = null;

  /** Opens or shuts fold `key`. */
  fold(key: string, open?: boolean): void {
    const next = new Set(this.folds);
    if (open ?? !next.has(key)) next.add(key);
    else next.delete(key);
    this.folds = next;
  }

  /** The operation a place names, of the spec's operations or its webhooks. */
  opAt(place: Place): OpRow | null {
    const index = this.index;
    if (index === null) return null;
    return index.ops.find((o) => o.anchor === place) ?? index.webhooks.find((o) => o.anchor === place) ?? null;
  }

  /** Whether the spec has the page `place` names. */
  has(place: Place): boolean {
    const page = pageOf(place);
    if (page === OVERVIEW) return !place.startsWith("tags/") || this.groups.some((g) => g.name === place.slice(5));
    if (page.startsWith("schemas/")) return this.schemaSet.has(page.slice(8));
    return this.opAt(page) !== null;
  }

  /**
   * Shows `place`: the pill moves now, the page once its one answer is in. A place the spec lacks shows the overview
   * and says so. Answers the page drawn, or `null` where a later pick overtook this one.
   */
  async show(place: Place): Promise<Place | null> {
    const ask = ++this.#asked;
    if (place !== OVERVIEW && !this.has(place)) {
      this.gone = place;
      this.current = OVERVIEW;
      this.shown = { place: OVERVIEW, answer: null };
      this.reading = false;
      return OVERVIEW;
    }
    this.gone = null;
    this.current = place;
    if (place.startsWith("tags/")) {
      const g = this.groups.find((x) => x.name === place.slice(5));
      if (g !== undefined) {
        this.fold(g.key, true);
        this.cursor = g.key;
      }
    }
    const page = pageOf(place);
    if (page === OVERVIEW) {
      this.shown = { place, answer: null };
      this.reading = false;
      return place;
    }
    let answer = this.#pages.get(page);
    if (answer === undefined) {
      const timer = setTimeout(() => {
        if (ask === this.#asked) this.reading = true;
      }, READING_AFTER_MS);
      try {
        answer = await this.load(page);
      } finally {
        clearTimeout(timer);
      }
      if (ask !== this.#asked) return null;
    } else {
      this.#keep(page, answer);
    }
    this.reading = false;
    this.shown = { place, answer };
    return place;
  }

  /**
   * Page `page`'s answer, read once and kept, with the shapes and examples it came with put where its parts look for
   * them. The explorer shows one page at a time (`show`); the inline form opens several (`TfOasInline`).
   */
  async load(page: Place): Promise<PageAnswer> {
    const kept = this.#pages.get(page);
    if (kept !== undefined) return kept;
    const answer = await this.session.page(page);
    this.#keep(page, answer);
    if (answer.kind === "op") {
      for (const [key, shape] of Object.entries(answer.shapes)) if (!this.shapes.has(`${page}/${key}`)) this.shapes.set(`${page}/${key}`, shape);
      for (const [key, sample] of Object.entries(answer.samples)) if (!this.samples.has(`${page}/${key}`)) this.samples.set(`${page}/${key}`, sample);
    } else if (answer.kind === "schema" && !this.shapes.has(page)) this.shapes.set(page, answer.shape);
    return answer;
  }

  /** Keeps `answer` as the newest of the last {@link KEPT_PAGES}. */
  #keep(page: Place, answer: PageAnswer): void {
    this.#pages.delete(page);
    this.#pages.set(page, answer);
    while (this.#pages.size > KEPT_PAGES) this.#pages.delete(this.#pages.keys().next().value!);
  }

  /** How many pages are kept (tests). */
  get keptPages(): number {
    return this.#pages.size;
  }

  /** The shape a caret at `key` opens, asked once; it opens once the shape is in, so it opens whole. */
  async openShape(key: string, base: string, node: unknown): Promise<void> {
    if (!this.shapes.has(key)) {
      const own = isObj(node) && typeof node.$ref === "string" ? node.$ref : null;
      // A ref into another file is read first (at the spec's commit), then shaped from its own file.
      if (own !== null) {
        const target = refTarget(base, own);
        if (target.ok && target.file !== base) {
          this.expand.add(key);
          await this.ensure(base, own);
          const got = this.ref(base, own);
          if (got !== undefined && "ok" in got && got.ok) this.shapes.set(key, await this.session.shape(got.base, got.node));
          return;
        }
      }
      this.shapes.set(key, await this.session.shape(base, node));
    }
    this.expand.add(key);
  }

  /** The example made for choice `choice` of the part at `key`, asked once (a switch over the top `oneOf`). */
  async sampleChoice(key: string, base: string, node: unknown, direction: "request" | "response" | "any", choice: number): Promise<void> {
    const at = `${key}|${choice}`;
    if (this.samples.has(at)) return;
    this.samples.set(at, await this.session.sample(base, node, direction, choice));
  }

  /** Opens or shuts the caret at `key`. */
  toggleShape(key: string, base: string, node: unknown): void {
    if (this.expand.has(key)) this.expand.delete(key);
    else void this.openShape(key, base, node);
  }

  /**
   * Opens the caret at `key` and everything below it (Alt-click, *Expand all*), {@link OPEN_ALL_DEPTH} levels deep and
   * at most {@link OPEN_ALL_MOST} carets, never into a ref the row already sits inside.
   */
  async openAll(key: string, base: string, node: unknown, trail: readonly string[] = []): Promise<void> {
    let left = OPEN_ALL_MOST;
    const walk = async (k: string, b: string, n: unknown, seen: readonly string[], depth: number): Promise<void> => {
      if (left <= 0 || depth > OPEN_ALL_DEPTH) return;
      const ref = innerRef(n);
      if (ref !== null && seen.includes(`${b}#${ref}`)) return;
      if (!hasInside(n)) return;
      left -= 1;
      await this.openShape(k, b, n);
      const shape = this.shapes.get(k);
      if (shape === undefined) return;
      const below = ref === null ? seen : [...seen, `${b}#${ref}`];
      await this.openBelow(k, shape, below, depth, walk);
    };
    await walk(key, base, node, trail, 0);
  }

  /** Opens what a shape holds, as {@link openAll} walks: its properties, its picked choice, its items, any other key. */
  async openBelow(
    k: string,
    shape: Shape,
    seen: readonly string[],
    depth: number,
    walk: (k: string, b: string, n: unknown, seen: readonly string[], depth: number) => Promise<void>,
  ): Promise<void> {
    for (const p of shape.props) await walk(`${k}/${p.name}`, shape.base, p.node, seen, depth + 1);
    if (shape.variants !== null) {
      const pick = Number(this.picked.get(`${k}|`) ?? 0);
      const at = `${k}|${pick}`;
      if (!this.shapes.has(at)) this.shapes.set(at, await this.session.shape(shape.base, shape.variants.list[pick]));
      const choice = this.shapes.get(at)!;
      await this.openBelow(at, choice, seen, depth, walk);
    }
    if (shape.props.length === 0 && shape.variants === null && shape.items !== undefined) {
      const at = `${k}/[]`;
      if (!this.shapes.has(at)) this.shapes.set(at, await this.session.shape(shape.base, shape.items));
      await this.openBelow(at, this.shapes.get(at)!, seen, depth, walk);
    }
    if (shape.extra !== undefined) await walk(`${k}/{}`, shape.base, shape.extra, seen, depth + 1);
  }

  /** Shuts the caret at `key` and everything below it. */
  closeAll(key: string): void {
    for (const k of [...this.expand]) if (k === key || k.startsWith(`${key}/`) || k.startsWith(`${key}|`)) this.expand.delete(k);
  }

  /** Opens everything a shape at `key` holds (*Expand all*), the shape itself drawn already. */
  async expandShape(key: string, shape: Shape, trail: readonly string[] = []): Promise<void> {
    let left = OPEN_ALL_MOST;
    const walk = async (k: string, b: string, n: unknown, seen: readonly string[], depth: number): Promise<void> => {
      if (left <= 0 || depth > OPEN_ALL_DEPTH) return;
      const ref = innerRef(n);
      if (ref !== null && seen.includes(`${b}#${ref}`)) return;
      if (!hasInside(n)) return;
      left -= 1;
      await this.openShape(k, b, n);
      const sh = this.shapes.get(k);
      if (sh !== undefined) await this.openBelow(k, sh, ref === null ? seen : [...seen, `${b}#${ref}`], depth, walk);
    };
    await this.openBelow(key, shape, trail, 0, walk);
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
    this.refs.set(key, await this.session.resolve(base, ref));
  }

  /** The named schema of the spec a ref lands on, so its name can be followed to its page. */
  named(base: string, ref: string): string | null {
    return base === this.root && this.index !== null ? namedSchema(ref, this.index.schemaBase, this.schemaSet) : null;
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
  if (Array.isArray(node.allOf) && node.allOf.length === 1 && isObj(node.allOf[0]) && typeof node.allOf[0].$ref === "string") return node.allOf[0].$ref;
  return null;
}

/** The label of a place, for *Back to*: an operation's method and path, a schema's name, *Overview*. */
export function labelOf(place: Place): string {
  if (place === OVERVIEW || place.startsWith("tags/")) return "Overview";
  if (place.startsWith("schemas/")) return place.slice(8);
  const space = place.indexOf(" ");
  return space < 0 ? place : `${place.slice(0, space).toUpperCase()} ${place.slice(space + 1)}`;
}
