/**
 * The monitoring settings section (issue #443).
 *
 * The seam is the rest of the settings view's: a rendered section in,
 * user-visible text and bridge calls out. What the backend does with the two
 * calls is `crates/knobas-sync/tests/samples.rs`' and the command tests';
 * what is asserted here is that the section draws what is **stored**, posts
 * what was typed, refuses to post a cleared field, and says so when it could
 * not ask.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import MonitoringSection from "./MonitoringSection.svelte";
import type { MonitoringSettings } from "../ipc/assets";

const STORED: MonitoringSettings = {
  sample_retention_days: 90,
  response_time_warn_ms: 1500,
};

let target: HTMLDivElement;
let app: Record<string, unknown> | undefined;

function render(
  ports: {
    monitoringSettings?: () => Promise<MonitoringSettings>;
    setMonitoringSettings?: (settings: MonitoringSettings) => Promise<MonitoringSettings>;
  } = {},
) {
  const sent: MonitoringSettings[] = [];
  app = mount(MonitoringSection, {
    target,
    props: {
      ports: {
        monitoringSettings: () => Promise.resolve({ ...STORED }),
        setMonitoringSettings: (settings: MonitoringSettings) => {
          sent.push(settings);
          return Promise.resolve(settings);
        },
        ...ports,
      },
    },
  });
  flushSync();
  return { sent };
}

/** The input a `<label for>` points at. */
function field(label: string): HTMLInputElement {
  const labelled = [...target.querySelectorAll("label")].find((candidate) =>
    candidate.textContent?.includes(label),
  );
  return target.querySelector<HTMLInputElement>(`#${labelled!.htmlFor}`)!;
}

function button(label: string): HTMLButtonElement | undefined {
  return [...target.querySelectorAll<HTMLButtonElement>("button")].find(
    (candidate) => candidate.textContent?.trim() === label,
  );
}

function type(input: HTMLInputElement, value: string) {
  input.value = value;
  input.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
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

test("the section draws the stored numbers and says what each one costs", async () => {
  render({
    monitoringSettings: () =>
      Promise.resolve({ sample_retention_days: 30, response_time_warn_ms: 800 }),
  });
  await vi.waitFor(() => expect(button("Save")).toBeTruthy());

  expect(field("Keep readings for").value).toBe("30");
  expect(field("Warn above").value).toBe("800");
  expect(text()).toContain("Monitoring");
  expect(text()).toContain("30 days");
  expect(text()).toContain("deletes");
  expect(text()).toContain("800 ms");
});

test("saving posts the edited numbers and redraws the sentences from the answer", async () => {
  const { sent } = render();
  await vi.waitFor(() => expect(button("Save")).toBeTruthy());

  type(field("Keep readings for"), "14");
  type(field("Warn above"), "2500");
  button("Save")!.click();

  await vi.waitFor(() => expect(sent).toHaveLength(1));
  expect(sent[0]).toEqual({ sample_retention_days: 14, response_time_warn_ms: 2500 });
  await vi.waitFor(() => {
    flushSync();
    expect(text()).toContain("14 days");
  });
  expect(text()).toContain("2500 ms");
});

/**
 * The section draws the **stored** answer and not the click: a backend that
 * clamped what was posted has to be what the reader ends up looking at.
 */
test("a value the backend clamped is what the fields end up showing", async () => {
  render({
    setMonitoringSettings: () =>
      Promise.resolve({ sample_retention_days: 1, response_time_warn_ms: 0 }),
  });
  await vi.waitFor(() => expect(button("Save")).toBeTruthy());

  type(field("Keep readings for"), "3");
  button("Save")!.click();

  await vi.waitFor(() => {
    flushSync();
    expect(field("Keep readings for").value).toBe("1");
  });
  expect(text()).toContain("one day");
});

/**
 * An emptied `<input type="number">` reads as `""`, which `JSON.stringify`
 * sends as `null` and the `u32` on the other side refuses. Clearing a field
 * before typing into it is the ordinary way to use one.
 */
test("an emptied field is never posted as a broken setting", async () => {
  const { sent } = render();
  await vi.waitFor(() => expect(button("Save")).toBeTruthy());

  type(field("Keep readings for"), "");
  type(field("Warn above"), "");
  button("Save")!.click();

  await vi.waitFor(() => expect(sent).toHaveLength(1));
  expect(Number.isInteger(sent[0]!.sample_retention_days)).toBe(true);
  expect(Number.isInteger(sent[0]!.response_time_warn_ms)).toBe(true);
  // Never zero: a retention of zero days puts the horizon at *now* and takes
  // the sample the poll a second ago wrote.
  expect(sent[0]!.sample_retention_days).toBeGreaterThanOrEqual(1);
});

/**
 * A read that failed is not "the defaults": *ninety days* is a claim about
 * what is stored, and a section that could not ask has not earned it.
 */
test("a read that failed offers Retry rather than claiming the defaults", async () => {
  let attempts = 0;
  render({
    monitoringSettings: () => {
      attempts += 1;
      return attempts === 1
        ? Promise.reject({ code: "not_ready", message: "the database is still starting" })
        : Promise.resolve({ ...STORED });
    },
  });

  await vi.waitFor(() => {
    flushSync();
    expect(text()).toContain("the database is still starting");
  });
  expect(text()).not.toContain("90 days");
  expect(button("Save")).toBeUndefined();

  button("Retry")!.click();

  await vi.waitFor(() => {
    flushSync();
    expect(text()).toContain("90 days");
  });
});

test("a write that is refused says why and leaves the stored numbers on screen", async () => {
  render({
    setMonitoringSettings: () =>
      Promise.reject({ code: "internal", message: "the write failed" }),
  });
  await vi.waitFor(() => expect(button("Save")).toBeTruthy());

  type(field("Keep readings for"), "14");
  button("Save")!.click();

  await vi.waitFor(() => {
    flushSync();
    expect(text()).toContain("the write failed");
  });
  expect(text()).not.toContain("14 days");
});
