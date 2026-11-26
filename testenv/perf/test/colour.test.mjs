// The probes' colour reader (`colour.mjs`), run by `watchdog-test.sh` with the other probe tests. The strings are the
// ones Chromium's getComputedStyle gives: the ux30 end gate's false `LAYER OVER A LIST` was the open launcher's scrim,
// `color-mix(in srgb, var(--scrim) 40%, transparent)`, read as transparent.
import { test } from "node:test";
import assert from "node:assert/strict";
import { rgba } from "../colour.mjs";

const alpha = (s) => rgba(s)[3];

test("rgb() and rgba(), with commas or spaces", () => {
  assert.deepEqual(rgba("rgb(10, 15, 20)"), [10, 15, 20, 1]);
  assert.deepEqual(rgba("rgba(10, 15, 20, 0.4)"), [10, 15, 20, 0.4]);
  assert.deepEqual(rgba("rgb(10 15 20 / 0.4)"), [10, 15, 20, 0.4]);
  assert.deepEqual(rgba("rgba(0, 0, 0, 0)"), [0, 0, 0, 0]);
});

test("a color-mix in srgb (the dialog's scrim) paints: color(srgb ... / a)", () => {
  const [r, g, b, a] = rgba("color(srgb 0.0392157 0.0588235 0.0784314 / 0.4)");
  assert.equal(a, 0.4);
  assert.equal(Math.round(r), 10);
  assert.equal(Math.round(g), 15);
  assert.equal(Math.round(b), 20);
  assert.equal(alpha("color(srgb 0.5 0.5 0.5)"), 1);
  assert.equal(alpha("color(srgb 0 0 0 / 0)"), 0);
});

test("a color-mix in oklab and other spaces: the alpha, channels unknown", () => {
  const [r, , , a] = rgba("oklab(0.2 0.01 -0.02 / 0.45)");
  assert.equal(a, 0.45);
  assert.ok(Number.isNaN(r));
  assert.equal(alpha("oklab(0.9 0 0)"), 1);
  assert.equal(alpha("oklch(0.5 0.1 200 / 25%)"), 0.25);
  assert.equal(alpha("color(display-p3 1 0 0 / 0.5)"), 0.5);
});

test("what is not a colour function reads transparent", () => {
  assert.deepEqual(rgba(""), [0, 0, 0, 0]);
  assert.deepEqual(rgba("none"), [0, 0, 0, 0]);
  assert.deepEqual(rgba(undefined), [0, 0, 0, 0]);
  assert.deepEqual(rgba("color(srgb)"), [0, 0, 0, 0]);
});

test("the function runs as page source, with nothing from its module", () => {
  const again = new Function(`return (${rgba})`)();
  assert.equal(again("color(srgb 0 0 0 / 0.4)")[3], 0.4);
  // scrollers.mjs writes it into AUDIT as a method: `rgba: function rgba(s) {...}`.
  const page = new Function(`const __a = { rgba: ${rgba} }; return __a;`)();
  assert.equal(page.rgba("rgba(1, 2, 3, 0.5)")[3], 0.5);
});
