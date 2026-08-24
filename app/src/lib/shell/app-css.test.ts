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
