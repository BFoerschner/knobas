/**
 * The Standup tab's pure half (#829, R-13.4): where a digest line opens, the
 * digest as text for *Copy as text*, and the protocol's three sections read
 * out of and written back into its note body.
 *
 * ## The protocol is still a note
 *
 * Prototype-6 draws the protocol as *Who was there*, a note per person and
 * the action items. The backend keeps a protocol as one Markdown note whose
 * template (`knobas_app::protocol::template_of`) has exactly those three
 * headings, and publishes that body to Confluence. So the tab is a form over
 * the body, not a new store: each edit rewrites one section's bullets and
 * leaves every other line as the reader or the template left it.
 *
 * - **Attendees** are the bullets under `## Attendees`.
 * - **Notes** are the bullets under `## Notes` that start with an attendee's
 *   name and a colon (`- Mara Lindqvist: review open`). A bullet that names
 *   nobody stays in the body untouched; it is published, just not drawn.
 * - **Action items** are what `standup/protocol.ts`'s `actionItems` reads, the
 *   rule the Rust side shares.
 *
 * A section emptied of bullets gets the template's empty bullet back, so the
 * body keeps the shape a new protocol has.
 */
import type { DigestLine, StandupDigest } from "../../ipc/entity";
import { actionItems, type ActionItem } from "../../standup/protocol";
import { dayLabel } from "../../time/day";
import { itemHash, keyOf } from "../job/job";

/** The protocol as the tab draws it. */
export interface ProtocolForm {
  attendees: string[];
  /** Each attendee's note, by name; someone with no note is absent. */
  notes: Record<string, string>;
  items: ActionItem[];
}

const ATTENDEES = "Attendees";
const NOTES = "Notes";
const ACTION_ITEMS = "Action items";

/** A Markdown list bullet's text, or `null` for a line that is not one. */
function bulletText(line: string): string | null {
  const trimmed = line.trim();
  const found = /^[-*+](?:\s+(.*))?$/.exec(trimmed);
  return found ? (found[1] ?? "").trim() : null;
}

function isHeading(line: string): boolean {
  return line.trim().startsWith("#");
}

function headingIs(line: string, name: string): boolean {
  return isHeading(line) && line.trim().replace(/^#+/, "").trim().toLowerCase() === name.toLowerCase();
}

/** The bullets' texts under a heading, empty ones dropped. */
function bulletsUnder(body: string, heading: string): string[] {
  const lines = body.split("\n");
  const start = lines.findIndex((line) => headingIs(line, heading));
  if (start < 0) return [];
  const out: string[] = [];
  for (const line of lines.slice(start + 1)) {
    if (isHeading(line)) break;
    const text = bulletText(line);
    if (text !== null && text !== "") out.push(text);
  }
  return out;
}

/**
 * Rewrite the bullet lines under `heading` with `edit`, keeping every other
 * line. `edit` gets the section's non-empty bullet lines and answers the new
 * ones; none left puts back `empty`, the template's placeholder. A missing
 * heading is added at the end of the body.
 */
function editSection(body: string, heading: string, empty: string, edit: (bullets: string[]) => string[]): string {
  const lines = body.split("\n");
  const start = lines.findIndex((line) => headingIs(line, heading));
  if (start < 0) {
    const bullets = edit([]);
    if (bullets.length === 0) return body;
    const base = body.replace(/\n*$/, "");
    return `${base}\n\n## ${heading}\n\n${bullets.join("\n")}\n`;
  }
  let end = lines.length;
  for (let index = start + 1; index < lines.length; index++) {
    if (isHeading(lines[index]!)) {
      end = index;
      break;
    }
  }
  const section = lines.slice(start + 1, end);
  const isBullet = (line: string) => bulletText(line) !== null;
  const first = section.findIndex(isBullet);
  const kept = section.filter((line) => isBullet(line) && bulletText(line) !== "" && !/^[-*+]\s*\[[ xX]\]\s*$/.test(line.trim()));
  const next = edit(kept);
  const bullets = next.length === 0 ? [empty] : next;
  let rebuilt: string[];
  if (first < 0) {
    // No bullets at all: after the heading's blank line.
    const lead = section.length > 0 && section[0]!.trim() === "" ? 1 : 0;
    rebuilt = [...section.slice(0, lead), ...(lead === 0 ? [""] : []), ...bullets, ...section.slice(lead)];
    if (section.length === 0) rebuilt.push("");
  } else {
    const before = section.slice(0, first);
    const after = section.slice(first).filter((line) => !isBullet(line));
    rebuilt = [...before, ...bullets, ...after];
  }
  return [...lines.slice(0, start + 1), ...rebuilt, ...lines.slice(end)].join("\n");
}

/** The note bullet's name and text, when it starts with `Name:`. */
function noteOf(text: string, attendees: readonly string[]): [string, string] | null {
  const colon = text.indexOf(":");
  if (colon < 0) return null;
  const name = text.slice(0, colon).trim();
  return attendees.includes(name) ? [name, text.slice(colon + 1).trim()] : null;
}

/** The three sections out of a protocol body. */
export function readProtocol(body: string): ProtocolForm {
  const attendees = bulletsUnder(body, ATTENDEES);
  const notes: Record<string, string> = {};
  for (const text of bulletsUnder(body, NOTES)) {
    const note = noteOf(text, attendees);
    if (note) notes[note[0]] = note[1];
  }
  return { attendees, notes, items: actionItems(body) };
}

/** The body with exactly these attendees, in this order. */
export function withAttendees(body: string, names: readonly string[]): string {
  return editSection(body, ATTENDEES, "- ", () => names.map((name) => `- ${name}`));
}

/**
 * The body with `name`'s note set: their bullet replaced in place, or a new
 * one after the section's last bullet. A note is one bullet, so line breaks
 * become spaces; an empty note removes the bullet.
 */
export function withNote(body: string, name: string, text: string): string {
  const flat = text.replace(/\s*\n\s*/g, " ").trim();
  const mine = (line: string) => noteOf(bulletText(line) ?? "", [name]) !== null;
  return editSection(body, NOTES, "- ", (bullets) => {
    const replacement = flat === "" ? [] : [`- ${name}: ${flat}`];
    const at = bullets.findIndex(mine);
    if (at < 0) return [...bullets, ...replacement];
    return [...bullets.slice(0, at), ...replacement, ...bullets.slice(at + 1)];
  });
}

/** The body with one more open action item at the end of its list. */
export function withActionItem(body: string, text: string): string {
  const flat = text.replace(/\s*\n\s*/g, " ").trim();
  if (flat === "") return body;
  return editSection(body, ACTION_ITEMS, "- [ ] ", (bullets) => [...bullets, `- [ ] ${flat}`]);
}

/**
 * Where a digest line opens in the new shell, or `null` for the one line with
 * nothing behind it (a running timer on a label). A page opens Docs.
 */
export function lineHash(line: Pick<DigestLine, "entity_id" | "kind">): string | null {
  if (line.entity_id === null) return null;
  if (line.kind === "page") return `#/page/${encodeURIComponent(line.entity_id)}`;
  return itemHash({ entity_id: line.entity_id, kind: line.kind ?? "" });
}

/** The ticket key a line leads with, or `null` for anything not a ticket. */
export function lineKey(line: Pick<DigestLine, "entity_id" | "kind">): string | null {
  return line.kind === "ticket" && line.entity_id !== null ? keyOf(line.entity_id) : null;
}

/** `"Thursday 21"`: the weekday and the date, as prototype-6 heads yesterday. */
export function shortDay(key: string): string {
  return dayLabel(key).split(" ").slice(0, 2).join(" ");
}

/** `"Friday 22 August"`: the protocol's day, without the year. */
export function protocolDay(key: string): string {
  return dayLabel(key).split(" ").slice(0, 3).join(" ");
}

/** What follows a line's title: `: reason` for yesterday, `(reason)` after. */
function textOf(line: DigestLine, yesterday: boolean): string {
  const key = lineKey(line);
  const head = key === null ? line.title : `${key} ${line.title}`;
  if (line.reason === "") return head;
  return yesterday ? `${head}: ${line.reason}` : `${head} (${line.reason})`;
}

/** The yesterday heading: `Yesterday, Thursday 21`, or bare when none was found. */
export function yesterdayHeading(day: string | null): string {
  return day === null ? "Yesterday" : `Yesterday, ${shortDay(day)}`;
}

/** The digest as plain text, for pasting into a chat before the meeting. */
export function digestText(digest: StandupDigest): string {
  const list = (lines: DigestLine[], yesterday: boolean) =>
    lines.length === 0 ? ["- None"] : lines.map((line) => `- ${textOf(line, yesterday)}`);
  return [
    yesterdayHeading(digest.yesterday_day),
    ...list(digest.yesterday, true),
    "",
    "Today",
    ...list(digest.today, false),
    "",
    "Blockers",
    ...list(digest.blockers, false),
  ].join("\n");
}
