/**
 * The boot screen says which of four things is happening.
 *
 * The whole value of the screen is that "downloading PostgreSQL" and "port
 * already in use" do not look the same, so what it renders per state is the
 * behaviour worth pinning.
 */
import { flushSync, mount, unmount } from "svelte";
import { expect, test, vi } from "vitest";

import Booting from "./Booting.svelte";
import type { DbState } from "../ipc/app";

function render(db: DbState, onretry = () => {}) {
  const target = document.createElement("div");
  document.body.append(target);
  const app = mount(Booting, {
    target,
    props: { db, version: "0.1.0", demo: true, onretry },
  });
  flushSync();
  return {
    target,
    text: () => target.textContent ?? "",
    done: () => {
      unmount(app);
      target.remove();
    },
  };
}

test("a slow first run says what is taking the time", () => {
  const screen = render({
    state: "starting",
    detail: "first run: downloading and initialising PostgreSQL",
  });
  expect(screen.text()).toContain("Starting the local database");
  expect(screen.text()).toContain("first run: downloading and initialising PostgreSQL");
  screen.done();
});

test("a plain start makes no claim it cannot back up", () => {
  const screen = render({ state: "starting", detail: null });
  expect(screen.text()).toContain("Starting the local database");
  // No invented reassurance where the backend had nothing to report.
  expect(screen.target.querySelectorAll("p")).toHaveLength(0);
  screen.done();
});

test("migrating is its own screen, not a second kind of starting", () => {
  const screen = render({ state: "migrating" });
  expect(screen.text()).toContain("Bringing the schema up to date");
  expect(screen.text()).not.toContain("Starting the local database");
  screen.done();
});

/**
 * The failure message is whatever PostgreSQL or the OS said — text, never
 * markup — and the copyable line is what a bug report needs.
 */
test("a failure shows the reason, the diagnostic line, and a way out", () => {
  const onretry = vi.fn();
  const screen = render(
    { state: "failed", message: "<b>port 50861</b> is in use" },
    onretry,
  );

  expect(screen.text()).toContain("<b>port 50861</b> is in use");
  expect(screen.target.querySelector("b")).toBeNull();
  expect(screen.text()).toContain("knobas 0.1.0");
  expect(screen.text()).toContain("profile demo");

  const retry = [...screen.target.querySelectorAll("button")].find(
    (button) => button.textContent === "Retry",
  );
  expect(retry).toBeDefined();
  retry?.click();
  expect(onretry).toHaveBeenCalledOnce();

  screen.done();
});

/** Only the failure screen offers a retry: there is nothing to retry yet. */
test("no retry button while it is still coming up", () => {
  for (const db of [
    { state: "starting", detail: null } as const,
    { state: "migrating" } as const,
  ]) {
    const screen = render(db);
    expect(screen.target.querySelectorAll("button")).toHaveLength(0);
    screen.done();
  }
});

/**
 * The design system keeps the code face for real code values (#1402): the
 * raw failure message is one, the sentences around it are not. *Retry* is
 * the design system's primary button.
 */
/** Whether any of the element's classes is a mono one. */
function mono(element: Element): boolean {
  return [...element.classList].some((name) => name.includes("mono"));
}

test("only the raw failure message is set in mono, and Retry is the primary button", () => {
  const screen = render({ state: "failed", message: "port 50861 is in use" });

  const lines = [...screen.target.querySelectorAll("p")];
  const message = lines.find((p) => p.textContent === "port 50861 is in use");
  expect(mono(message!)).toBe(true);
  const prose = lines.filter((p) => p !== message);
  expect(prose.map((p) => p.textContent?.trim())).toEqual(["knobas 0.1.0 · profile demo"]);
  expect(prose.filter(mono)).toEqual([]);

  const retry = screen.target.querySelector("button");
  expect([...(retry?.classList ?? [])]).toEqual(expect.arrayContaining(["tf-btn", "tf-primary"]));
  expect(retry?.classList.contains("btn")).toBe(false);
  screen.done();
});

test("the sentences while it comes up are prose, not mono", () => {
  for (const db of [
    { state: "starting", detail: "first run: downloading and initialising PostgreSQL" } as const,
    { state: "migrating" } as const,
  ]) {
    const screen = render(db);
    const lines = [...screen.target.querySelectorAll("p")];
    expect(lines).toHaveLength(1);
    expect(mono(lines[0]!)).toBe(false);
    screen.done();
  }
});

/** The start screen's update line (ADR-0052, decisions D14): its room is there with an offer and without one. */
test("a failed start offers a newer knobas in a line whose room is always kept", async () => {
  const { fakeUpdate } = await import("./dev/fake/update");
  const pressed: string[] = [];
  const rendered = (state: ReturnType<typeof fakeUpdate> | null) => {
    const target = document.createElement("div");
    document.body.append(target);
    const box = $state({ state });
    const update = {
      get state() {
        return box.state;
      },
      check: async () => void pressed.push("check"),
      download: async () => {
        pressed.push("download");
        box.state = { ...box.state!, phase: { phase: "ready" } };
      },
      install: async () => void pressed.push("install"),
    };
    const app = mount(Booting, { target, props: { db: { state: "failed", message: "migration 102 failed" }, version: "3.2.9-ux28", demo: false, onretry: () => {}, update } });
    flushSync();
    return { target, done: () => (unmount(app), target.remove()) };
  };

  const none = rendered(fakeUpdate("up-to-date"));
  expect(none.target.querySelector("[data-testid=boot-update]")?.textContent).toContain("No newer version yet.");
  none.done();
  // Mutant: draw the line only with an offer, and this room is gone.
  const unknown = rendered(null);
  expect(unknown.target.querySelector("[data-testid=boot-update]")).not.toBeNull();
  unknown.done();

  const offered = rendered(fakeUpdate("available"));
  const line = offered.target.querySelector("[data-testid=boot-update]")!;
  expect(line.textContent).toContain("knobas 3.2.9-ux28 is out and may fix this.");
  line.querySelector("button")!.click();
  for (let i = 0; i < 20; i += 1) await Promise.resolve();
  flushSync();
  for (let i = 0; i < 20; i += 1) await Promise.resolve();
  expect(pressed).toEqual(["download", "install"]);
  offered.done();
});
