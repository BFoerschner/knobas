/**
 * A filesystem path split where `TfPath` cuts it: everything up to and with the last separator, and the folder or file
 * after it. A path that ends in a separator keeps it on its end, so the folder it names is still the end.
 */
export function pathParts(path: string): { head: string; tail: string } {
  const trimmed = path.replace(/[\\/]+$/, "");
  const at = Math.max(trimmed.lastIndexOf("/"), trimmed.lastIndexOf("\\"));
  if (at < 0 || at === trimmed.length - 1) return { head: "", tail: path };
  return { head: path.slice(0, at + 1), tail: path.slice(at + 1) };
}

/** The characters a file's name keeps at its end before its extension, when it gives up its middle. */
const NAME_END = 12;

/**
 * A file's name split where it gives up its middle (#2531, Secrets' attachments): the end keeps the extension and the
 * {@link NAME_END} characters before it, so `…bundle 2026-11.p12` still says which file it is; the head before it is
 * what an ellipsis cuts. An extension is a dot not at the start and up to eight letters or digits after it; a name
 * without one keeps its last {@link NAME_END} characters. A short name is all end.
 */
export function nameParts(name: string): { head: string; tail: string } {
  const ext = /(?<=.)\.[A-Za-z0-9]{1,8}$/.exec(name)?.[0] ?? "";
  let at = name.length - NAME_END - ext.length;
  if (at <= 0) return { head: "", tail: name };
  // Never between the halves of a character outside the BMP.
  const code = name.charCodeAt(at);
  if (code >= 0xdc00 && code <= 0xdfff) at -= 1;
  return { head: name.slice(0, at), tail: name.slice(at) };
}

/**
 * The program a command line runs: its first word, where quotes group words as the backend splits a command
 * (`knobas-core`'s `checkout::open::words`), so `"C:\Program Files\x\y.exe" --flag` is `C:\Program Files\x\y.exe`. A menu
 * shows this, cut in its middle by `TfPath`, and keeps the whole command for its title (the render-as pass, 2026-11-19).
 */
export function programOf(command: string): string {
  const text = command.trimStart();
  let word = "";
  let quote: string | null = null;
  for (const ch of text) {
    if (quote !== null) {
      if (ch === quote) quote = null;
      else word += ch;
    } else if (ch === '"' || ch === "'") quote = ch;
    else if (/\s/.test(ch)) break;
    else word += ch;
  }
  return word;
}

/**
 * A command line split where `TfCommand` draws it: its first word as typed, quotes kept, cut as `pathParts` cuts a path, and
 * the rest after it, spaces at its start dropped. All of it is drawn, so a selection copies the command as typed (the
 * renderers-columns pass, 2026-11-20).
 */
export function commandParts(command: string): { head: string; tail: string; rest: string } {
  const text = command.trim();
  let quote: string | null = null;
  let at = 0;
  for (const ch of text) {
    if (quote !== null) {
      if (ch === quote) quote = null;
    } else if (ch === '"' || ch === "'") quote = ch;
    else if (/\s/.test(ch)) break;
    at += ch.length;
  }
  return { ...pathParts(text.slice(0, at)), rest: text.slice(at).trimStart() };
}

/**
 * A list drawn on one line (`TfWords`): its first `shown` entries, and how many more there are, which the line says as "+N".
 */
export function wordsShown(list: readonly string[], shown = 3): { shown: string[]; more: number } {
  return { shown: list.slice(0, shown), more: Math.max(0, list.length - shown) };
}
