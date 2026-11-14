/**
 * Key hints as the platform writes them (the Windows pass, 2026-11-14: the search box said ⌘/ in WebView2 on Windows).
 * The one place a modifier is spelled for the eye: on the Mac the glyphs (`⌘K`, `⌘⇧O`), elsewhere the names joined by
 * `+` (`Ctrl+K`, `Ctrl+Shift+O`), modifiers in that platform's order.
 *
 * The app's own hints are written as the Mac spells them, where ⌘ is the key the shell reads as ⌘ or Ctrl
 * (`isBoxChord`, `refreshKey`): {@link keyHint} turns every chord in a text into the platform's. A key a person bound
 * is stored as pressed (`launcher/keys.ts`, where Ctrl is ⌃ and ⌘ is the Windows key off the Mac): {@link chordParts}
 * and {@link chordLabel} name exactly those keys.
 */

/**
 * Whether the app runs on macOS. WKWebView's user agent names the Mac, WebView2's Windows, WebKitGTK's X11 and Linux.
 * Anything that names none of the others draws the Mac's glyphs, as jsdom's `(darwin)` and `(linux)` do, so a test
 * reads the same on every machine.
 */
export function onMac(): boolean {
  if (typeof navigator === "undefined") return true;
  return !/Windows|X11|Linux|CrOS|Android/.test(navigator.userAgent);
}

/** The modifier glyphs a chord is spelled with, and what each is called off the Mac when ⌘ means ⌘ or Ctrl. */
const HINT_NAMES: Record<string, string> = { "⌘": "Ctrl", "⌃": "Ctrl", "⌥": "Alt", "⇧": "Shift" };
/** The same for a chord as pressed, where ⌘ is the Windows key. */
const PRESSED_NAMES: Record<string, string> = { "⌘": "Win", "⌃": "Ctrl", "⌥": "Alt", "⇧": "Shift" };
/** The order Windows and Linux write modifiers in. */
const ORDER = ["Ctrl", "Alt", "Shift", "Win"];

/** A run of modifier glyphs and the key after it: a word key (`Enter`) or one character, else none. */
const CHORD = /([⌘⌃⌥⇧]+)(Enter|Esc|Tab|Space|Backspace|[^\s⌘⌃⌥⇧])?/gu;

function named(glyphs: string, key: string, names: Record<string, string>): string[] {
  const mods = [...new Set([...glyphs].map((glyph) => names[glyph]!))].sort((a, b) => ORDER.indexOf(a) - ORDER.indexOf(b));
  return key === "" ? mods : [...mods, key];
}

/**
 * Every chord in `text`, written as the Mac spells it with ⌘ for ⌘ or Ctrl, as this platform writes it: `⌘K` is
 * `Ctrl+K` off the Mac, `Escape or ⌘Enter` is `Escape or Ctrl+Enter`. On the Mac `text` as it is.
 */
export function keyHint(text: string): string {
  if (onMac()) return text;
  return text.replace(CHORD, (_, glyphs: string, key: string | undefined) => named(glyphs, key ?? "", HINT_NAMES).join("+"));
}

/** A chord as pressed and stored (`⌃⇧O`), one key cap a part: `⌃ ⇧ O` on the Mac, `Ctrl Shift O` elsewhere. */
export function chordParts(chord: string): string[] {
  if (onMac()) return [...chord];
  const parts: string[] = [];
  for (const [, glyphs, key] of chord.matchAll(CHORD)) parts.push(...named(glyphs!, key ?? "", PRESSED_NAMES));
  return parts.length > 0 ? parts : [chord];
}

/** A chord as pressed and stored, on one line: `⌃⇧O` on the Mac, `Ctrl+Shift+O` elsewhere. */
export function chordLabel(chord: string): string {
  return chordParts(chord).join(onMac() ? "" : "+");
}
