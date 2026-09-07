/**
 * The clones-root settings section (issue #499).
 *
 * The seam every other settings section is tested at: a rendered section in,
 * user-visible text and bridge calls out. What the backend stores is
 * `crates/knobas-app/tests/checkout_ipc.rs`'s; what is asserted here is that
 * the field draws what is **stored**, that Save and Clear send what was asked
 * for, and that a read it could not make says so instead of drawing *not set*.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import CheckoutsSection from "./CheckoutsSection.svelte";

let target: HTMLDivElement;
let app: Record<string, unknown> | undefined;

/**
 * Mount the section over a bridge that **remembers** what it was sent, so a
 * write followed by the section's own re-read draws what is now stored. A
 * fixture that answered the same thing after a write could not tell a section
 * that re-reads from one that keeps what was typed, which is the property the
 * Save test is about.
 */
function render(
  ports: {
    clonesRoot?: () => Promise<string | null>;
    setClonesRoot?: (path: string | null) => Promise<void>;
  } = {},
  initial: string | null = "/Users/mara/src",
) {
  const sent: (string | null)[] = [];
  let stored: string | null = initial;
  app = mount(CheckoutsSection, {
    target,
    props: {
      ports: {
        clonesRoot: () => Promise.resolve(stored),
        setClonesRoot: (path: string | null) => {
          sent.push(path);
          stored = path;
          return Promise.resolve();
        },
        ...ports,
      },
    },
  });
  flushSync();
  return { sent };
}

async function settle() {
  for (let i = 0; i < 4; i += 1) await Promise.resolve();
  flushSync();
}

function text(): string {
  return (target.textContent ?? "").replace(/\s+/g, " ").trim();
}

function field(): HTMLInputElement {
  const input = target.querySelector<HTMLInputElement>("#clones-root");
  if (!input) throw new Error(`no clones-root field: ${text()}`);
  return input;
}

function button(label: string): HTMLButtonElement {
  const found = [...target.querySelectorAll("button")].find(
    (element) => (element.textContent ?? "").trim() === label,
  );
  if (!found) throw new Error(`no button reading ${label}: ${text()}`);
  return found as HTMLButtonElement;
}

beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
});

afterEach(() => {
  if (app) unmount(app);
  app = undefined;
  target.remove();
});

test("the field draws the stored root", async () => {
  render();
  await settle();
  expect(field().value).toBe("/Users/mara/src");
  // The section says what knobas will not do with the directory, which is the
  // question a person has about handing one over (ADR-0016).
  expect(text()).toContain("never clones, checks out or fetches");
});

test("an unset root says so, and offers no Clear", async () => {
  render({}, null);
  await settle();
  expect(field().value).toBe("");
  expect(text()).toContain("Not set");
  expect(() => button("Clear")).toThrow();
});

test("Save sends the typed path and the field then draws what is stored", async () => {
  const { sent } = render({}, null);
  await settle();

  const input = field();
  input.value = "/Users/mara/code";
  input.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  button("Save").click();
  await settle();

  expect(sent).toEqual(["/Users/mara/code"]);
  expect(field().value).toBe("/Users/mara/code");
  expect(text()).not.toContain("Not set");
});

test("Clear unsets it, and the section goes back to saying nothing is looked for", async () => {
  const { sent } = render();
  await settle();

  button("Clear").click();
  await settle();

  expect(sent).toEqual([null]);
  expect(field().value).toBe("");
  expect(text()).toContain("Not set");
});

test("a read that failed says so rather than drawing an empty field", async () => {
  render({ clonesRoot: () => Promise.reject(new Error("the database is still starting")) });
  await settle();

  expect(text()).toContain("the database is still starting");
  // Not *Not set*: "no root" is a claim about what is stored, and a section
  // that could not ask has not earned it.
  expect(text()).not.toContain("Not set");
});
