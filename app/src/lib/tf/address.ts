/**
 * Where an entity opens in the ticket-first shell, and where an address the
 * rooms UI used to hand out lands now (#1085).
 *
 * Pure. {@link entityAddress} is the one rule every producer outside a page
 * uses: the launcher's rows, a desktop notification's click and the capture
 * window's *Open in knobas*. It is `job.ts`' `itemHash` plus the two kinds
 * that function has no page for: a route opens Dependencies' asset tree at
 * the asset exposing it (`#/route/<id>`, as before #881), and a note opens
 * its own page (`#/note/<id>`, #1504), which sends the reader to Settings ›
 * Notes and capture while notes hide (D10).
 *
 * {@link legacyAddress} reads an address the new shell has no place for. A
 * notification delivered before #1085 still carries `#/entity/<id>`, and a
 * bookmark may hold `#/ticket/<id>` or a room; the shell asks this module
 * before it answers *Nothing lives at …*.
 */
import { itemHash } from "./job/job";
import { jobHash } from "./route";

/**
 * The address of an entity of `kind`: `itemHash`, which owns every kind's
 * rule (#1101). A ticket opens its job by its entity id, the address that
 * names the source, so a key two sources hold opens the one it was drawn
 * from (#1476, #1977). `title`, where the producer holds one, lets a standup
 * protocol note open Time › Standup at its date rather than the note page
 * (#1504).
 */
export function entityAddress(kind: string, entityId: string, title?: string): string {
  return itemHash({ kind, entity_id: entityId, title });
}

/**
 * What an old address means now: an address to go to in its place, an entity
 * id whose kind has to be read first (`#/entity/<id>`), or `null` when the
 * rooms UI never handed this address out.
 */
export type Legacy = { go: string } | { resolve: string } | null;

/**
 * The kinds the rooms UI addressed as `#/<kind>/<id>`. Several are first
 * segments of the new shell too (`#/pr/…`, `#/build/…`), which never reach
 * here; they are listed so the rule reads whole. `route` is not a kind the
 * rooms UI had (`#/route/<id>` was a view), and the new shell reads it itself.
 */
const ROOM_KINDS = new Set([
  "ticket",
  "pr",
  "repo",
  "branch",
  "commit",
  "build",
  "page",
  "asset",
  "monitor",
  "note",
]);

/** Read a rooms-UI address. */
export function legacyAddress(hash: string): Legacy {
  const segments = hash.replace(/^#\/?/, "").split("/");
  const [head = "", ...rest] = segments;
  const id = decode(rest.join("/"));
  // A room, any room: the new shell has one list of work, and it is Tickets.
  if (head === "ctx") return { go: "#/tickets" };
  // The sources view and the first-run wizard both live in Settings › Sources.
  if (head === "sources" || head === "first-run") return { go: "#/settings/sources" };
  // The standup digest is a tab of Time.
  if (head === "standup") return { go: "#/time/standup" };
  // The Assets view's *Monitors* tab (#448): Monitors is a section of its own
  // since #864, and the tab's roster went with the rooms UI (#1087, #1459).
  if (head === "assets" && rest[0] === "monitors") return { go: "#/monitors" };
  if (id === "") return null;
  if (head === "entity") return { resolve: id };
  // The id the rooms UI held is the entity id, which names the source (#1676).
  if (head === "start-work" || head === "ticket") return { go: jobHash(id) };
  if (ROOM_KINDS.has(head)) return { go: entityAddress(head, id) };
  return null;
}

function decode(segment: string): string {
  try {
    return decodeURIComponent(segment);
  } catch {
    return segment;
  }
}
