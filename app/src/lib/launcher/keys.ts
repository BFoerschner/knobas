/**
 * Keys (#2429, spec #2419 "Keys"): what a key press spells as a chord, and
 * which chords nobody may bind.
 *
 * A chord is a modifier and one letter or digit, read by the key's **code**
 * so a layout or ⌥'s character does not change it, and spelled the way the
 * spec spells it: ⌘ ⌃ ⌥ ⇧, then the letter in capitals or the digit
 * (`⌘⇧O`). ⇧ alone is typing. `knobas_search::keys::is_chord` and migration
 * 0070's checks hold the same spelling.
 *
 * Two tables keep them (`keys.svelte.ts`): a saved list's chord, which opens
 * the launcher on the list from anywhere (the shell listens, `TfShell`), and
 * an action's chord per kind, which runs that action of the Tab sheet on the
 * selected row inside the launcher.
 */
import type { SheetSubject } from "./sheet";

const MODIFIERS = [
  ["metaKey", "⌘"],
  ["ctrlKey", "⌃"],
  ["altKey", "⌥"],
  ["shiftKey", "⇧"],
] as const;

type Mods = Pick<KeyboardEvent, "metaKey" | "ctrlKey" | "altKey" | "shiftKey">;

/** The modifiers held, spelled in the chord's order: what the panel draws while a chord is being pressed. */
export function heldOf(event: Mods): string[] {
  return MODIFIERS.filter(([flag]) => event[flag]).map(([, glyph]) => glyph);
}

/** The letter or digit a code names (`KeyO` → `O`, `Digit1` → `1`), else `null`. */
function keyOfCode(code: string): string | null {
  const letter = /^Key([A-Z])$/.exec(code);
  if (letter) return letter[1]!;
  const digit = /^Digit([0-9])$/.exec(code);
  return digit ? digit[1]! : null;
}

/** The chord a key press spells, `⌘⇧O`, or `null` when it is typing or no chord at all. */
export function chordOf(event: Mods & Pick<KeyboardEvent, "code">): string | null {
  if (!(event.metaKey || event.ctrlKey || event.altKey)) return null;
  const key = keyOfCode(event.code);
  return key === null ? null : `${heldOf(event).join("")}${key}`;
}

/**
 * Whose chord a press is, when it is somebody's, with ⌘ or ⌃ (off the Mac
 * Ctrl is the box's ⌘) and whatever else is held on top:
 * the launcher's own (⌘K opens it, ⌘S saves, ⌘B binds, ⌘D is Titles or
 * Everything), the shell's (⌘T the timer, ⌘R refresh), and the system's
 * editing and window chords, which a key would take from every field.
 */
const TAKEN: Record<string, "launcher" | "shell" | "system"> = {
  K: "launcher",
  S: "launcher",
  B: "launcher",
  D: "launcher",
  T: "shell",
  R: "shell",
  A: "system",
  C: "system",
  V: "system",
  X: "system",
  Z: "system",
  Q: "system",
  W: "system",
  H: "system",
  M: "system",
};

/** ⌘? (⌘⇧/ as most keyboards send it): the box's filters, which is no letter and so no chord. */
function isHelpPress(event: Mods & Pick<KeyboardEvent, "key">): boolean {
  return (event.metaKey || event.ctrlKey) && (event.key === "?" || (event.key === "/" && event.shiftKey));
}

/** Why a press cannot be bound, said as the panel says it, or `null` when it is free (or no chord). */
export function refusalOf(event: Mods & Pick<KeyboardEvent, "code" | "key">): string | null {
  if (isHelpPress(event)) return `${event.metaKey ? "⌘" : "⌃"}? is the launcher's own`;
  const chord = chordOf(event);
  if (chord === null || !(event.metaKey || event.ctrlKey)) return null;
  // ⌘[ and ⌘] are history's (`shell/keys.ts`), typed with ⌥ and a digit on a
  // German board, so they are a chord there.
  if (event.key === "[" || event.key === "]") return `${chord} is the shell's own`;
  // The owners read the character, the chord reads the code: on a German
  // board undo is ⌘ on KeyY. Either reading refuses.
  const owner = TAKEN[chord.slice(-1)] ?? (event.key.length === 1 ? TAKEN[event.key.toUpperCase()] : undefined);
  return owner ? `${chord} is the ${owner}'s own` : null;
}

/** The kind a key is kept on: an entity's own kind (`repo`, `pr`, `ticket`, ...), else the row's (`list`, `view`, `set`, `inbox`). */
export function kindOf(subject: SheetSubject): string {
  return subject.kind === "entity" ? subject.entityKind : subject.kind;
}

/** The kind's word in a sentence: "pull request", "repository", else the kind. */
export function kindWord(kind: string): string {
  switch (kind) {
    case "pr":
      return "pull request";
    case "repo":
      return "repository";
    case "list":
      return "saved list";
    case "inbox":
      return "inbox line";
    default:
      return kind;
  }
}
