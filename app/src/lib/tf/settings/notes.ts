/**
 * Settings › Notes and capture (#832, R-13a.6, D10), in prototype-6's words.
 *
 * Pure, so every sentence the section draws is testable as a string.
 */

/** *1 note*, *37 notes*. */
export function noteCount(total: number): string {
  return total === 1 ? "1 note" : `${total} notes`;
}

/** *37 notes, the last from 12 August*; the year joins a date from another year. */
export function notesLine(total: number, last: string | null, now: Date): string {
  if (total === 0) return "No notes";
  const count = noteCount(total);
  if (last === null) return count;
  const at = new Date(last);
  const date = at.toLocaleDateString("en-GB", {
    day: "numeric",
    month: "long",
    ...(at.getFullYear() === now.getFullYear() ? {} : { year: "numeric" }),
  });
  return total === 1 ? `${count}, from ${date}` : `${count}, the last from ${date}`;
}

/** The symbols a modifier is drawn with on a key cap, as macOS draws them. */
const KEY_CAPS: Record<string, string> = {
  cmdorctrl: "⌘",
  commandorcontrol: "⌘",
  cmd: "⌘",
  command: "⌘",
  super: "⌘",
  meta: "⌘",
  alt: "⌥",
  option: "⌥",
  shift: "⇧",
  ctrl: "⌃",
  control: "⌃",
};

/** Mac's modifier order: ⌃ ⌥ ⇧ ⌘, then the key. */
const ORDER = ["⌃", "⌥", "⇧", "⌘"];

/**
 * An accelerator (`CmdOrCtrl+Alt+N`) as key caps (`⌥ ⌘ N`), modifiers in the
 * order macOS draws them.
 */
export function keyCaps(accelerator: string): string[] {
  const parts = accelerator
    .split("+")
    .map((part) => part.trim())
    .filter((part) => part !== "");
  const caps = parts.map((part) => KEY_CAPS[part.toLowerCase()] ?? part.toUpperCase());
  const modifiers = caps.filter((cap) => ORDER.includes(cap)).sort((a, b) => ORDER.indexOf(a) - ORDER.indexOf(b));
  return [...modifiers, ...caps.filter((cap) => !ORDER.includes(cap))];
}

/** A folder under a home directory, written from `~` as prototype-6 writes it. */
export function homePath(folder: string): string {
  return folder.replace(/^(\/Users\/[^/]+|\/home\/[^/]+|[A-Za-z]:\\Users\\[^\\]+)(?=[/\\]|$)/, "~");
}
