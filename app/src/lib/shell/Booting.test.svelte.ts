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
