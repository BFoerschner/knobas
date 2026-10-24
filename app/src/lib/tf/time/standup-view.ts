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
import type { DigestLine, LinkEntry, StandupDigest } from "../../ipc/entity";
import { actionItems, bullet, type ActionItem } from "../../standup/protocol";
import { clockReading, dayLabel } from "../../time/day";
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

/** A Markdown list bullet's text, or `null` for a line that is not one: `actionItems`' reading. */
function bulletText(line: string): string | null {
  return bullet(line.trim());
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
 * line where it was. `edit` gets the section's non-empty bullet lines and
 * answers the new ones; none left puts back `empty`, the template's
 * placeholder. A missing heading is added at the end of the body.
 *
 * Bullets move only as far as the edit needs (#934): a kept bullet stays on
 * its line, a changed one takes the line of the bullet it replaces, and a new
 * one goes after the section's last bullet and the lines that continue it. So
 * prose between bullets, a wrapped bullet's tail and the blank lines stay put.
 * A body written with CRLF gets CRLF on the lines this writes.
 */
function editSection(body: string, heading: string, empty: string, edit: (bullets: string[]) => string[]): string {
  const cr = body.includes("\r\n") ? "\r" : "";
  const lines = body.split("\n");
  const start = lines.findIndex((line) => headingIs(line, heading));
  if (start < 0) {
    const bullets = edit([]);
    if (bullets.length === 0) return body;
    const nl = `${cr}\n`;
    const base = body.replace(/(\r?\n)*$/, "");
    return `${base}${nl}${nl}## ${heading}${nl}${nl}${bullets.join(nl)}${nl}`;
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
  const isKept = (line: string) =>
    isBullet(line) && bulletText(line) !== "" && !/^[-*+]\s*\[[ xX]\]\s*$/.test(line.trim());
  const kept = section.filter(isKept);
  const next = edit(kept);
  // What `edit` wrote carries no `\r`; a kept line keeps its own.
  const bullets = (next.length === 0 ? [empty] : next).map((line) => (line.endsWith("\r") ? line : `${line}${cr}`));
  let rebuilt: string[];
  let last = section.length - 1;
  while (last >= 0 && !isBullet(section[last]!)) last--;
  if (last < 0) {
    // No bullets at all: after the heading's blank line.
    const lead = section.length > 0 && section[0]!.trim() === "" ? 1 : 0;
    rebuilt = [...section.slice(0, lead), ...(lead === 0 ? [cr] : []), ...bullets, ...section.slice(lead)];
    if (section.length === 0) rebuilt.push("");
  } else {
    // New bullets go after the last bullet and the non-blank lines that continue it.
    let insertAt = last + 1;
    while (insertAt < section.length && section[insertAt]!.trim() !== "" && !isBullet(section[insertAt]!)) insertAt++;
    rebuilt = [];
    let at = 0;
    let keptSeen = 0;
    section.forEach((line, index) => {
      if (index === insertAt) {
        rebuilt.push(...bullets.slice(at));
        at = bullets.length;
      }
      if (!isBullet(line)) {
        rebuilt.push(line);
        return;
      }
      if (!isKept(line)) return; // the template's empty bullet
      keptSeen++;
      const found = bullets.indexOf(line, at);
      if (found >= 0) {
        rebuilt.push(...bullets.slice(at, found + 1));
        at = found + 1;
      } else if (at < bullets.length && !kept.slice(keptSeen).includes(bullets[at]!)) {
        rebuilt.push(bullets[at]!);
        at++;
      }
    });
    if (insertAt === section.length) rebuilt.push(...bullets.slice(at));
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

/**
 * The tickets a protocol's links already name for its action items (#1433):
 * a link whose other end is a ticket, not withdrawn, titled exactly as the
 * item's words. That is the match the backend made when it drew the link
 * (`sources::write_queue::ticket_filed`, where the source named no key): the
 * create titles the ticket with the item's words, trimmed, and the link is
 * drawn to the ticket so titled.
 */
export function itemTickets(
  links: readonly LinkEntry[],
  items: readonly { text: string }[],
): { text: string; ticket: string }[] {
  const tickets = links.filter((entry) => entry.other.kind === "ticket" && entry.other.deleted_at === null);
  return items.flatMap(({ text }) => {
    const found = tickets.find((entry) => entry.other.title === text.trim());
    return found ? [{ text, ticket: found.other.entity_id }] : [];
  });
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
export function lineHash(line: Pick<DigestLine, "entity_id" | "kind" | "title">): string | null {
  if (line.entity_id === null) return null;
  if (line.kind === "page") return `#/page/${encodeURIComponent(line.entity_id)}`;
  return itemHash({ entity_id: line.entity_id, kind: line.kind ?? "", title: line.title });
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

/**
 * When a monitor alert opened, as the reader's clock reads it: `from 03:10`.
 * The duration is the line's reason; the clock is here because only this side
 * knows the reader's timezone.
 */
export function alertSince(line: Pick<DigestLine, "at">): string {
  return `from ${clockReading(line.at)}`;
}

/** An Infrastructure line as plain text: `payout-api was down 12 min, from 03:10`. */
function alertText(line: DigestLine): string {
  return `${line.title} ${line.reason}, ${alertSince(line)}`;
}

/**
 * An action item's words and its trailing `(Owner)`, which prototype-6 dims.
 * Only a last parenthesis counts; the words are the reader's.
 */
export function ownerOf(text: string): { what: string; who: string | null } {
  const found = /^(.*\S)\s+(\([^()]+\))$/.exec(text);
  return found ? { what: found[1]!, who: found[2]! } : { what: text, who: null };
}

/** The yesterday heading: `Yesterday, Thursday 21`, or bare when none was found. */
export function yesterdayHeading(day: string | null): string {
  return day === null ? "Yesterday" : `Yesterday, ${shortDay(day)}`;
}

/** The digest as plain text, for pasting into a chat before the meeting. */
export function digestText(digest: StandupDigest): string {
  const list = (lines: DigestLine[], text: (line: DigestLine) => string) =>
    lines.length === 0 ? ["- None"] : lines.map((line) => `- ${text(line)}`);
  const after = (line: DigestLine) => textOf(line, false);
  return [
    yesterdayHeading(digest.yesterday_day),
    ...list(digest.yesterday, (line) => textOf(line, true)),
    "",
    "Today",
    ...list(digest.today, after),
    "",
    "Blockers",
    ...list(digest.blockers, after),
    "",
    "Infrastructure",
    ...list(digest.infrastructure, alertText),
  ].join("\n");
}
