/**
 * The controls specimen (#2280): `#/controls` draws one example of every
 * control the design system's Controls section names. The list is read out of
 * the section's specimen table, so a control added to the section without its
 * example here fails.
 */
import { readFileSync } from "node:fs";
import { join } from "node:path";

import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import { until } from "../../shell/test-wait";
import { pageFor } from "../pages";
import { parseTfHash } from "../route";

const spec = readFileSync(join(process.cwd(), "../docs/specs/2026-10-15-ticket-first-design-system.md"), "utf8");
const section = spec.slice(spec.indexOf("## Controls"), spec.indexOf("\n## ", spec.indexOf("## Controls") + 1));
/** [control, selector] for each row of the section's specimen table. */
const NAMED = [...section.matchAll(/^\| ([^|]+?) \| `([^`]+)` \|$/gm)].map((row) => [row[1]!, row[2]!] as const);

let target: HTMLElement;
let view: ReturnType<typeof mount> | undefined;

beforeEach(async () => {
  target = document.createElement("div");
  document.body.append(target);
  const page = pageFor(parseTfHash("#/controls"));
  if (page === undefined) throw new Error("nothing draws #/controls");
  view = mount(page.component, { target, props: page.props({} as never) });
  flushSync();
  await until(() => target.querySelector("h1") !== null, "the specimen loads");
});

afterEach(() => {
  if (view) unmount(view);
  view = undefined;
  target.remove();
});

const text = (el: Element) => el.textContent?.replace(/\s+/g, " ").trim();

test("the section's specimen table was read, so the next test checks something", () => {
  expect(NAMED.length).toBeGreaterThanOrEqual(16);
});

test.each(NAMED)("the specimen draws the %s (%s)", (_, selector) => {
  expect(target.querySelector(selector)).not.toBeNull();
});

test("the header's actions come primary first, then the others, then a ghost Open in, then Sync now last", () => {
  const acts = [...target.querySelectorAll(".tf-fhead .tf-acts > *")];
  expect(acts.length).toBeGreaterThanOrEqual(4);
  expect(acts[0]!.classList.contains("tf-primary")).toBe(true);
  const openIn = acts.at(-2)!;
  expect(text(openIn)).toMatch(/^Open in /);
  expect(openIn.classList.contains("tf-ghost")).toBe(true);
  expect(text(acts.at(-1)!)).toBe("Sync now");
  // Every header action is at the default size.
  expect(acts.filter((act) => act.matches(".tf-sm, .tf-lg"))).toEqual([]);
});

test("the dialog footer is the extra action, then a ghost Cancel, then the primary", () => {
  const footer = [...target.querySelectorAll(".tf-dialog footer")[0]!.querySelectorAll(":scope > .tf-btn")];
  expect(footer.map((button) => text(button))).toEqual(["Delete", "Cancel", "Save"]);
  expect(footer[0]!.classList.contains("tf-first")).toBe(true);
  expect(footer[1]!.classList.contains("tf-ghost")).toBe(true);
  expect(footer[2]!.classList.contains("tf-primary")).toBe(true);
});

test("every icon-only button has a name, and the specimen sets no style of its own", () => {
  for (const icon of target.querySelectorAll(".tf-iconbtn")) expect(icon.getAttribute("aria-label")).toBeTruthy();
  expect(target.querySelector("[style]")).toBeNull();
  expect(target.querySelector("style")).toBeNull();
});
