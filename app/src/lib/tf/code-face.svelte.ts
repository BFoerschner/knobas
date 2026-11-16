/**
 * The face code is drawn in (Settings › Appearance, 2026-11-16, Björn: "let me set the font generally in the settings
 * pane"): one family and one ligature switch for every place code is drawn, through `--mono`.
 *
 * `--mono` (`css/tokens.css`) is `var(--mono-chosen, "JetBrains Mono")` first, then the bundled JetBrains Mono, its
 * metric-matched stand-ins and the system's monospace, so a family missing on this machine never draws anything in a
 * proportional face. Every rule that sets `--mono` sets `font-variant-ligatures: var(--mono-ligatures)` beside it
 * (`code-face.test.ts` holds both), so the two properties this writes on `<html>` reach all of it at once: a choice
 * applies with no reload, and turning ligatures on moves nothing, since a coding font's ligature keeps the width of
 * the characters it joins.
 *
 * Kept in the webview's `localStorage` (`knobas.code.font`, `knobas.code.ligatures`), as the diff layout is, and
 * applied before the shell mounts (`main.ts`): read from the database, it would land after the first draw, and every
 * line of code on the page the app opens on would draw in one face and then in another. It is this machine's choice,
 * too: it names a font installed here, so a backup restored on another machine does not carry it.
 */
import { keptValue, windowStorage, type KeptStorage } from "./kept-value";

export const FONT_KEY = "knobas.code.font";
export const LIGATURES_KEY = "knobas.code.ligatures";
/** The face knobas brings along (`tf.css` imports it), drawn when nothing else is chosen. */
export const BUNDLED = "JetBrains Mono";
/** A longer name is not a font's. */
const LONGEST = 200;

/** What the choice writes to: `<html>`'s inline style, through the CSSOM (CSP refuses inline styles in markup). */
export type RootStyle = Pick<CSSStyleDeclaration, "setProperty" | "removeProperty">;

export interface CodeFace {
  /** The chosen family, or `null` for the bundled JetBrains Mono. */
  readonly family: string | null;
  /** Whether code draws its font's ligatures (`=>`, `!=`, `->` as one sign); off until set. */
  readonly ligatures: boolean;
  /** Choose a family; an empty name or JetBrains Mono's goes back to the bundled face. */
  setFamily(name: string | null): void;
  setLigatures(on: boolean): void;
  /** Tests only: the bundled face, no ligatures, nothing stored. */
  reset(): void;
}

/**
 * What a person typed or picked, as a family name: trimmed, the quotes of a CSS string dropped, and `null` for
 * nothing or for the bundled face itself.
 */
export function familyName(typed: string | null): string | null {
  if (typed === null) return null;
  let name = typed.replace(/[\u0000-\u001f\u007f]/g, "").trim();
  if (/^(["']).*\1$/.test(name)) name = name.slice(1, -1).trim();
  if (name === "" || name.length > LONGEST || name.toLowerCase() === BUNDLED.toLowerCase()) return null;
  return name;
}

/** `name` as a CSS string, which `font-family` takes whatever the name holds. */
export function cssFamily(name: string): string {
  return `"${name.replace(/\\/g, "\\\\").replace(/"/g, '\\"')}"`;
}

export function createCodeFace(storage: () => KeptStorage | null, root: () => RootStyle | null): CodeFace {
  const keptFamily = keptValue(storage, FONT_KEY);
  const keptLigatures = keptValue(storage, LIGATURES_KEY);
  let family = $state<string | null>(familyName(keptFamily.read()));
  let ligatures = $state(keptLigatures.read() === "on");

  function apply(): void {
    const style = root();
    if (!style) return;
    if (family === null) style.removeProperty("--mono-chosen");
    else style.setProperty("--mono-chosen", cssFamily(family));
    if (ligatures) style.setProperty("--mono-ligatures", "normal");
    else style.removeProperty("--mono-ligatures");
  }
  apply();

  return {
    get family() {
      return family;
    },
    get ligatures() {
      return ligatures;
    },
    setFamily(name) {
      family = familyName(name);
      if (family === null) keptFamily.clear();
      else keptFamily.write(family);
      apply();
    },
    setLigatures(on) {
      ligatures = on;
      if (on) keptLigatures.write("on");
      else keptLigatures.clear();
      apply();
    },
    reset() {
      family = null;
      ligatures = false;
      keptFamily.clear();
      keptLigatures.clear();
      apply();
    },
  };
}

/** The window's choice, written on `<html>` as soon as this module loads (`main.ts` loads it before mounting). */
export const codeFace: CodeFace = createCodeFace(windowStorage, () => (typeof document === "undefined" ? null : document.documentElement.style));
