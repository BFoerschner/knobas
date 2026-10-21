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

/**
 * The symbols a modifier is drawn with on a key cap, as macOS draws them. The
 * names are `parse_hotkey`'s modifiers in global-hotkey 0.8.0 (Cargo.lock);
 * re-check this table and `KEYS` below when that crate is bumped (#1467).
 */
const KEY_CAPS: Record<string, string> = {
  cmdorctrl: "⌘",
  commandorcontrol: "⌘",
  commandorctrl: "⌘",
  cmdorcontrol: "⌘",
  cmd: "⌘",
  command: "⌘",
  super: "⌘",
  alt: "⌥",
  option: "⌥",
  shift: "⇧",
  ctrl: "⌃",
  control: "⌃",
};

/** Mac's modifier order: ⌃ ⌥ ⇧ ⌘, then the key. */
const ORDER = ["⌃", "⌥", "⇧", "⌘"];

/**
 * The keys the capture registrar accepts (global-hotkey 0.8.0's `parse_key`, upper
 * case; a hand copy, re-check it on a bump of the crate): a token outside this set and the modifiers is refused (#1467).
 */
const KEYS = new Set(
  [
    "BACKQUOTE `", "BACKSLASH \\", "BRACKETLEFT [", "BRACKETRIGHT ]", "PAUSE PAUSEBREAK", "COMMA ,", "EQUAL =",
    "MINUS -", "PERIOD .", "QUOTE '", "SEMICOLON ;", "SLASH /", "BACKSPACE", "CAPSLOCK", "ENTER", "SPACE", "TAB",
    "DELETE", "END", "HOME", "INSERT", "PAGEDOWN", "PAGEUP", "PRINTSCREEN", "SCROLLLOCK", "ARROWDOWN DOWN",
    "ARROWLEFT LEFT", "ARROWRIGHT RIGHT", "ARROWUP UP", "NUMLOCK", "ESCAPE ESC", "AUDIOVOLUMEDOWN VOLUMEDOWN",
    "AUDIOVOLUMEUP VOLUMEUP", "AUDIOVOLUMEMUTE VOLUMEMUTE", "MEDIAPLAY", "MEDIAPAUSE", "MEDIAPLAYPAUSE", "MEDIASTOP",
    "MEDIATRACKNEXT", "MEDIATRACKPREV MEDIATRACKPREVIOUS",
    "NUMADD NUMPADADD NUMPADPLUS NUMPLUS NUMPADDECIMAL NUMDECIMAL NUMPADDIVIDE NUMDIVIDE NUMPADENTER NUMENTER",
    "NUMPADEQUAL NUMEQUAL NUMPADMULTIPLY NUMMULTIPLY NUMPADSUBTRACT NUMSUBTRACT",
    ...Array.from({ length: 10 }, (_, n) => `${n} DIGIT${n} NUMPAD${n} NUM${n}`),
    ...Array.from({ length: 26 }, (_, n) => {
      const letter = String.fromCharCode(65 + n);
      return `${letter} KEY${letter}`;
    }),
    ...Array.from({ length: 24 }, (_, n) => `F${n + 1}`),
  ].flatMap((names) => names.split(" ")),
);

/**
 * An accelerator (`CmdOrCtrl+Alt+N`) as key caps (`⌥ ⌘ N`), modifiers in the
 * order macOS draws them. It draws only what the registrar's parser accepts:
 * modifiers, then exactly one key from its set. Anything else yields no caps
 * (#1467), never an unknown token in capitals.
 */
export function keyCaps(accelerator: string): string[] {
  const tokens = accelerator.split("+").map((part) => part.trim());
  const key = tokens[tokens.length - 1] ?? "";
  const modifiers = tokens.slice(0, -1);
  if (!KEYS.has(key.toUpperCase())) return [];
  const caps: string[] = [];
  for (const token of modifiers) {
    const cap = KEY_CAPS[token.toLowerCase()];
    if (cap === undefined) return [];
    caps.push(cap);
  }
  caps.sort((a, b) => ORDER.indexOf(a) - ORDER.indexOf(b));
  return [...caps, key.toUpperCase()];
}

/** A folder under a home directory, written from `~` as prototype-6 writes it. */
export function homePath(folder: string): string {
  return folder.replace(/^(\/Users\/[^/]+|\/home\/[^/]+|[A-Za-z]:\\Users\\[^\\]+)(?=[/\\]|$)/, "~");
}
