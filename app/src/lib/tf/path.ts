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
