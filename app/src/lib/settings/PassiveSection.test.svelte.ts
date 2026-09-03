/**
 * The passive attribution toggle (issue #282).
 *
 * The seam is the one the rest of the settings view uses: a rendered section
 * in, user-visible text and bridge calls out. What the backend does with the
 * two calls is `crates/knobas-app/tests/time_ipc.rs`'s; what is asserted here
 * is that the section draws what is **stored**, sends what was clicked, and
 * says so when it could not ask.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import PassiveSection from "./PassiveSection.svelte";

let target: HTMLDivElement;
let app: Record<string, unknown> | undefined;

function render(
  ports: {
    passiveAttribution?: () => Promise<boolean>;
    setPassiveAttribution?: (enabled: boolean) => Promise<boolean>;
  } = {},
) {
  const sent: boolean[] = [];
  app = mount(PassiveSection, {
    target,
    props: {
      ports: {
        passiveAttribution: () => Promise.resolve(false),
        setPassiveAttribution: (enabled: boolean) => {
          sent.push(enabled);
          return Promise.resolve(enabled);
        },
        ...ports,
      },
    },
  });
  flushSync();
  return { sent };
}

function toggle(): HTMLInputElement | null {
  return target.querySelector<HTMLInputElement>('input[type="checkbox"]');
}

function text(): string {
  return (target.textContent ?? "").replace(/\s+/g, " ").trim();
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

/**
 * The section says what it will record **before** it offers the switch, and it
 * says the two things a person opting in is entitled to know: that nothing
 * leaves the machine, and that nothing recorded this way is logged on its own.
 */
test("the section says what it records before it offers the switch", async () => {
  render();
  await vi.waitFor(() => expect(toggle()).toBeTruthy());

  expect(text()).toContain("Passive attribution");
  expect(text()).toContain("every thirty seconds");
  expect(text()).toContain("two minutes");
  expect(text()).toContain("leaves your machine");
  expect(text()).toContain("nothing passive is ever logged");
});

/** Off is the stored answer for a profile nobody has switched it on in. */
test("the toggle draws the stored value", async () => {
  render({ passiveAttribution: () => Promise.resolve(true) });
  await vi.waitFor(() => expect(toggle()).toBeTruthy());

  expect(toggle()!.checked).toBe(true);
});

test("a profile nobody has opted in on draws the switch off", async () => {
  render();
  await vi.waitFor(() => expect(toggle()).toBeTruthy());

  expect(toggle()!.checked).toBe(false);
});

test("switching it on sends the click and draws what came back", async () => {
  const { sent } = render();
  await vi.waitFor(() => expect(toggle()).toBeTruthy());

  toggle()!.click();
  await vi.waitFor(() => expect(sent).toEqual([true]));
  flushSync();
  expect(toggle()!.checked).toBe(true);

  toggle()!.click();
  await vi.waitFor(() => expect(sent).toEqual([true, false]));
  flushSync();
  expect(toggle()!.checked).toBe(false);
});

/**
 * **The toggle draws the stored value and not the click.** A write that failed
 * must leave the switch reading what is actually stored — telling somebody
 * they have opted in when nothing was written is the one failure this surface
 * must not have.
 */
test("a write that is refused leaves the switch where it was and says why", async () => {
  render({
    setPassiveAttribution: () =>
      Promise.reject({ code: "not_ready", message: "the database is still starting" }),
  });
  await vi.waitFor(() => expect(toggle()).toBeTruthy());

  toggle()!.click();
  await vi.waitFor(() => {
    flushSync();
    expect(text()).toContain("the database is still starting");
  });
  expect(toggle(), "the switch is behind the failure, not left claiming a state").toBeNull();
});

/**
 * A read that failed is not "off": *off* is a claim about what is stored, and
 * a section that could not ask has not earned it. Retry asks again.
 */
test("a read that failed offers Retry rather than claiming the setting is off", async () => {
  let attempts = 0;
  render({
    passiveAttribution: () => {
      attempts += 1;
      return attempts === 1
        ? Promise.reject({ code: "not_ready", message: "the database is still starting" })
        : Promise.resolve(true);
    },
  });

  await vi.waitFor(() => {
    flushSync();
    expect(text()).toContain("the database is still starting");
  });
  expect(toggle()).toBeNull();

  [...target.querySelectorAll<HTMLButtonElement>("button")]
    .find((candidate) => candidate.textContent?.trim() === "Retry")!
    .click();

  await vi.waitFor(() => {
    flushSync();
    expect(toggle()?.checked).toBe(true);
  });
});
