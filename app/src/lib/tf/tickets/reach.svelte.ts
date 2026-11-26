/**
 * Where a ticket can go (ticket-edit decisions, section 5; ADR-0054): what each source said its tickets can **reach**
 * (`reachable_transitions`), kept by the window for the session so a pickup or a status menu lights from memory in one
 * frame and never waits for a source.
 *
 * - **Exact**: an answer about this ticket while it stood where it stands, keyed by its entity id, status and
 *   `updated_at` as the card holds them. A ticket the mirror shows changed is a miss.
 * - **Like**: the last answer about another ticket of the same source, issue type, status and key stem (`PAY`,
 *   `tidewater/payout-service`). A prediction: a workflow's conditions can differ by ticket.
 * - **Observed**: with neither, the statuses the board's last read says the ticket's source uses. Not a claim about the
 *   workflow, and the drop treats it so (`drop.ts`).
 *
 * Asked ahead, at most once per exact key while one is held or in flight: when the pointer rests on a card (at most two
 * such reads out at once), when a key selects one, when one is pressed, when a ticket page opens. Asked again at every
 * pickup unless an exact answer landed in the last ten seconds. A rest and a selection ask behind (the background lane),
 * a press, a pickup and a page a person's own read.
 *
 * The answer is never stored in the database and never trusted by the write: the adapter resolves the status at write
 * time and refuses by name (CONTEXT.md, *Reachable transition*).
 *
 * The credential's grant for a move (`refused_writes`, `transition`) rides with it: asked once per ticket per session
 * wherever reach is asked, and kept by entity.
 */
import type { MiniBoardCard, SourceStatuses } from "../../ipc/entity";
import { reachableTransitions } from "../../ipc/entity";
import { askRefused, type RefusedWritesPort } from "../../shell/refused-writes";
import { sourceRoles } from "../roles.svelte";

export interface Reach {
  /** In the source's order and spelling. */
  statuses: string[];
  standing: "exact" | "like" | "observed";
  /** The last read's failure, where this is not an exact answer. */
  failed?: string;
}

export type ReachKey = Pick<MiniBoardCard, "entity_id" | "source_id" | "key" | "status" | "issue_type" | "updated_at">;

/** Why a read was asked: a rest and a selection ask behind; a press, a pickup and a page are the person's own. */
export type ReachWhen = "rest" | "select" | "press" | "pickup" | "open";

/** The read, injectable so a test needs no bridge: `reachable_transitions`, behind or not. */
export type ReachPort = (entityId: string, behind: boolean) => Promise<string[]>;

/** How many exact and like answers the window keeps, least recently used out first. */
export const EXACT_MOST = 2_000;
export const LIKE_MOST = 500;
/** A pickup asks again unless an exact answer landed this recently (ms). */
export const FRESH_MS = 10_000;
/** At most this many reads a rest has out at once; a rest while they are out asks nothing. */
export const RESTS_OUT_MOST = 2;

/** The key without its number: `PAY-231` is `PAY`, `tidewater/payout-service#37` is `tidewater/payout-service`. */
export function keyStem(key: string): string {
  return key.replace(/[-#]?\d+$/, "");
}

const exactKey = (t: ReachKey) => `${t.entity_id}\u0000${t.status ?? ""}\u0000${t.updated_at ?? ""}`;
const likeKey = (t: ReachKey) => `${t.source_id}\u0000${t.issue_type ?? ""}\u0000${(t.status ?? "").trim().toLowerCase()}\u0000${keyStem(t.key)}`;

/** A `Map` bounded by count: a read or a write moves a key to the newest end, and the oldest goes past `most`. */
class Bounded<V> {
  readonly #map = new Map<string, V>();
  constructor(readonly most: number) {}
  get(key: string): V | undefined {
    const value = this.#map.get(key);
    if (value === undefined) return undefined;
    this.#map.delete(key);
    this.#map.set(key, value);
    return value;
  }
  /** Without moving it: what a reactive read may do, which must not write. */
  peek(key: string): V | undefined {
    return this.#map.get(key);
  }
  set(key: string, value: V): void {
    this.#map.delete(key);
    this.#map.set(key, value);
    while (this.#map.size > this.most) this.#map.delete(this.#map.keys().next().value as string);
  }
  deleteWhere(gone: (key: string) => boolean): void {
    for (const key of [...this.#map.keys()]) if (gone(key)) this.#map.delete(key);
  }
  clear(): void {
    this.#map.clear();
  }
  get size(): number {
    return this.#map.size;
  }
}

const exact = new Bounded<{ statuses: string[]; at: number }>(EXACT_MOST);
const like = new Bounded<string[]>(LIKE_MOST);
/** The statuses each source's tickets were last seen in, by source id. */
const observed = new Map<string, string[]>();
/** The last failure of a read, by entity id, until an answer lands. */
const failures = new Map<string, string>();
/** Reads in flight, by exact key: a second ask joins the first. */
const flying = new Map<string, Promise<Reach | null>>();
let restsOut = 0;
/** Moves on every change, so `reachOf` is reactive without the maps being proxies. */
let version = $state(0);
/** The clock, a test's to set. */
let now: () => number = () => Date.now();

/** The credential's answer for a move, by entity: the reason it is refused, `null` allowed, absent not known. */
const grants = $state<Record<string, string | null>>({});
const grantsAsked = new Set<string>();

/** What the window holds of where `ticket` can go now, or `null`; reactive. */
export function reachOf(ticket: ReachKey): Reach | null {
  void version;
  const failed = failures.get(ticket.entity_id);
  const own = exact.peek(exactKey(ticket));
  if (own !== undefined) return { statuses: [...own.statuses], standing: "exact" };
  const sibling = like.peek(likeKey(ticket));
  if (sibling !== undefined) return { statuses: [...sibling], standing: "like", ...(failed === undefined ? {} : { failed }) };
  const seen = observed.get(ticket.source_id);
  if (seen !== undefined) return { statuses: [...seen], standing: "observed", ...(failed === undefined ? {} : { failed }) };
  return null;
}

/** Whether a read of `ticket` is out now. */
export function asking(ticket: ReachKey): boolean {
  void version;
  return flying.has(exactKey(ticket));
}

/**
 * Ask `ticket`'s source where it can go, `when` says why. Answers what the window then holds; a source that declares
 * no `transition` is never asked and answers `null`.
 */
export function askReach(ticket: ReachKey, when: ReachWhen, read?: ReachPort, grantsRead?: RefusedWritesPort): Promise<Reach | null> {
  if (!sourceRoles.writes(ticket.source_id, "transition")) return Promise.resolve(null);
  askGrant(ticket.entity_id, grantsRead);
  const key = exactKey(ticket);
  const inFlight = flying.get(key);
  if (inFlight !== undefined) return inFlight;
  const held = exact.get(key);
  if (held !== undefined && (when !== "pickup" || now() - held.at < FRESH_MS)) return Promise.resolve(reachOf(ticket));
  const resting = when === "rest";
  if (resting && restsOut >= RESTS_OUT_MOST) return Promise.resolve(reachOf(ticket));
  if (resting) restsOut += 1;
  const behind = when === "rest" || when === "select";
  const port = read ?? ((entityId: string, back: boolean) => reachableTransitions(entityId, back));
  const asked = Promise.resolve()
    .then(() => port(ticket.entity_id, behind))
    .then(
      (statuses) => {
        exact.set(key, { statuses: [...statuses], at: now() });
        like.set(likeKey(ticket), [...statuses]);
        failures.delete(ticket.entity_id);
        return true;
      },
      (error: unknown) => {
        failures.set(ticket.entity_id, messageOf(error));
        return false;
      },
    )
    .then(() => {
      flying.delete(key);
      if (resting) restsOut -= 1;
      version += 1;
      return reachOf(ticket);
    });
  flying.set(key, asked);
  version += 1;
  return asked;
}

/** The statuses each source's tickets stand in, from a board's answer (`MiniBoard.sources`). */
export function noteObserved(sources: readonly SourceStatuses[]): void {
  let changed = false;
  for (const { source_id, statuses } of sources) {
    const had = observed.get(source_id);
    if (had !== undefined && had.length === statuses.length && had.every((status, at) => status === statuses[at])) continue;
    observed.set(source_id, [...statuses]);
    changed = true;
  }
  if (changed) version += 1;
}

/** A landed write on `entityId` (its echo turning *sent*): its exact answers go, the like ones stay. No id: everything, as every test starts. */
export function forgetReach(entityId?: string): void {
  if (entityId === undefined) {
    exact.clear();
    like.clear();
    observed.clear();
    failures.clear();
    flying.clear();
    restsOut = 0;
    grantsAsked.clear();
    for (const key of Object.keys(grants)) delete grants[key];
    now = () => Date.now();
  } else {
    const lead = `${entityId}\u0000`;
    exact.deleteWhere((key) => key.startsWith(lead));
    failures.delete(entityId);
  }
  version += 1;
}

/** Tests only: the clock the ten seconds are read on. */
export function setReachClock(clock: () => number): void {
  now = clock;
}

/** Tests only: how many answers the window holds. */
export function reachSizes(): { exact: number; like: number } {
  return { exact: exact.size, like: like.size };
}

/** Why the credential may not move `entityId`, `null` where it may, `undefined` while not known. Reactive. */
export function moveRefusal(entityId: string): string | null | undefined {
  return grants[entityId];
}

/** Ask once a session whether the credential may move `entityId` (`refused_writes`, `transition`). */
export function askGrant(entityId: string, read?: RefusedWritesPort): void {
  if (grantsAsked.has(entityId)) return;
  grantsAsked.add(entityId);
  void askRefused([{ entity: entityId, op: "transition" }], read).then((answer) => {
    grants[entityId] = answer.find((refused) => refused.entity === entityId && refused.op === "transition")?.reason ?? null;
  });
}

function messageOf(error: unknown): string {
  if (error && typeof error === "object" && "message" in error) return String((error as { message: unknown }).message);
  return String(error);
}
