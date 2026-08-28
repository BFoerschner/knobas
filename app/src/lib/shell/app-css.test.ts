/**
 * What the ported Signal stylesheet has to keep.
 *
 * The sheet itself is a design artefact — a diff against the mockup is the way
 * to review it, not a test. What is worth pinning is the handful of things a
 * later edit could quietly drop and nobody would see until a bundle: the
 * tokens every component references, the promise that no font is fetched over
 * the network, and the reduced-motion escape hatch.
 */
import { readFileSync } from "node:fs";
import { join } from "node:path";

import { expect, test } from "vitest";

const css = readFileSync(join(process.cwd(), "src/app.css"), "utf8");

/**
 * Every component in `lib/shell/**` reads its colours and type out of these.
 * A dropped token is not a compile error anywhere: it is a rule that resolves
 * to nothing and an element that renders in the browser's defaults.
 */
test("carries the Signal tokens the components reference", () => {
  const missing = [
    "--bg",
    "--panel",
    "--raised",
    "--hair",
    "--hair2",
    "--text",
    "--muted",
    "--faint",
    "--amber",
    "--fail",
    "--ok",
    "--link",
    "--disp",
    "--sans",
    "--mono",
    "--row",
  ].filter((token) => !css.includes(`${token}:`));
  expect(missing).toEqual([]);
});

/**
 * The mockup linked Google Fonts. `default-src 'self'` forbids it, and a
 * desktop app that needs the network to render its own type is wrong even
 * where the policy would allow it. The fonts are `@fontsource` packages, so
 * they are `@import`ed from `node_modules` and bundled.
 */
test("fetches no font over the network", () => {
  expect(css).not.toMatch(/@import\s+(url\()?\s*["']?https?:/);
  expect(css).not.toMatch(/https?:\/\/fonts\.(googleapis|gstatic)\.com/);
});

/** The fonts are actually vendored, rather than simply absent. */
test("vendors the three families the tokens name", () => {
  for (const family of ["ibm-plex-sans", "ibm-plex-mono", "barlow-condensed"]) {
    expect(css).toContain(`@fontsource/${family}/`);
  }
});

/**
 * Spec §2 and §14: the split-flap leaves and the slide-over fade are motion a
 * reader can switch off. Dropping this block is the accessibility regression
 * with the least visible symptom.
 */
test("disables the flap animation under reduced motion", () => {
  const block = css.match(/@media\s*\(prefers-reduced-motion:\s*reduce\)\s*\{[\s\S]*?\n\}/);
  expect(block, "no prefers-reduced-motion block").not.toBeNull();
  expect(block?.[0]).toContain(".flap .leaf");
});

/**
 * The shell is a fixed three-row grid. A scrolling body would double-scroll
 * every pane inside it, which is the mockup's one non-obvious document rule.
 */
test("keeps the document from scrolling behind the app frame", () => {
  expect(css).toMatch(/html,\s*body\s*\{[^}]*overflow:\s*hidden/);
});

/**
 * Spec §14's own bar, computed rather than asserted about.
 *
 * The mockup shipped `--faint` at 2.86:1 and it was used for small secondary
 * text throughout, so this is the one accessibility requirement the port could
 * not simply inherit. The decision — lift the token, override the three rules
 * that put it on a hover surface — is written into `app.css` beside the
 * palette; this is what stops it being quietly undone by an editor pulling the
 * colour back towards the mockup.
 */

/** One channel of sRGB, linearised. WCAG 2.x relative luminance. */
function channel(value: number): number {
  const c = value / 255;
  return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
}

function luminance(hex: string): number {
  const digits = hex.replace("#", "");
  const [r, g, b] = [0, 2, 4].map((at) => channel(Number.parseInt(digits.slice(at, at + 2), 16)));
  return 0.2126 * r! + 0.7152 * g! + 0.0722 * b!;
}

function contrast(a: string, b: string): number {
  const [high, low] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (high! + 0.05) / (low! + 0.05);
}

/** A token's value, read out of `:root` rather than restated here. */
function token(name: string): string {
  const match = new RegExp(`--${name}\\s*:\\s*(#[0-9A-Fa-f]{6})`).exec(css);
  expect(match, `--${name} is not declared as a hex colour in app.css`).toBeTruthy();
  return match![1]!;
}

test("the contrast maths agrees with the WCAG reference pairs", () => {
  // Black on white is 21:1 and a colour on itself is 1:1. Without these the
  // function could be wrong in a way that made every assertion below pass.
  expect(contrast("#000000", "#FFFFFF")).toBeCloseTo(21, 5);
  expect(contrast("#15171A", "#15171A")).toBeCloseTo(1, 5);
  // A known failure, so the threshold assertions are known to be reachable.
  expect(contrast("#5A6169", "#15171A")).toBeLessThan(4.5);
});

test("every text token clears 4.5:1 on the surfaces it is read on", () => {
  const surfaces = { bg: token("bg"), panel: token("panel") };
  const failures: string[] = [];

  for (const name of ["text", "muted", "faint", "amber", "fail", "ok", "link"]) {
    for (const [surface, colour] of Object.entries(surfaces)) {
      const ratio = contrast(token(name), colour);
      if (ratio < 4.5) failures.push(`--${name} on --${surface}: ${ratio.toFixed(2)}`);
    }
  }

  expect(failures, "spec §14 sets 4.5:1 for text").toEqual([]);
});

test("the four rules that put faint text on a hover surface are lifted", () => {
  // `--raised` is the one background `--faint` does not clear, and it is a
  // hover or selected state rather than a resting surface. Deleting these
  // overrides would put small text at 4.09:1 under a pointer, which is exactly
  // the kind of regression a palette assertion alone cannot see.
  expect(contrast(token("faint"), token("raised"))).toBeLessThan(4.5);
  expect(contrast(token("muted"), token("raised"))).toBeGreaterThanOrEqual(4.5);

  // All four, not three: `.pop .it.on small` is the selected-row case and was
  // unpinned, so deleting it would have lifted nothing and failed nothing.
  for (const selector of [
    ".pop .it:hover small",
    ".pop .it.on small",
    ".mod:hover .sub",
    ".card.sel .k .pr",
  ]) {
    expect(css, `${selector} is no longer lifted off --faint`).toContain(selector);
  }
});
