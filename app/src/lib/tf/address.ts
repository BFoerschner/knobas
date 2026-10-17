/**
 * Where an entity opens in the ticket-first shell, and where an address the
 * rooms UI used to hand out lands now (#1085).
 *
 * Pure. {@link entityAddress} is the one rule every producer outside a page
 * uses: the launcher's rows, a desktop notification's click and the capture
 * window's *Open in knobas*. It is `job.ts`' `itemHash` plus the two kinds
 * that function has no page for: a route opens Dependencies' asset tree at
 * the asset exposing it (`#/route/<id>`, as before #881), and a note opens
 * Settings › Notes and capture, the one place notes are drawn while they hide
 * (D10).
 *
 * {@link legacyAddress} reads an address the new shell has no place for. A
 * notification delivered before #1085 still carries `#/entity/<id>`, and a
 * bookmark may hold `#/ticket/<id>` or a room; the shell asks this module
 * before it answers *Nothing lives at …*.
 */
import { itemHash } from "./job/job";
import { jobHash } from "./route";

/** Where Settings shows the notes and the capture shortcut (D10). */
const NOTES_HASH = "#/settings/notes";

/** The address of an entity of `kind`. */
export function entityAddress(kind: string, entityId: string): string {
  if (kind === "note") return NOTES_HASH;
  if (kind === "route") return `#/route/${encodeURIComponent(entityId)}`;
  return itemHash({ kind, entity_id: entityId });
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
  if (id === "") return null;
  if (head === "entity") return { resolve: id };
  if (head === "start-work") return { go: jobHash(id) };
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
