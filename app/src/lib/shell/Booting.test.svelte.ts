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

function render(db: DbState, onretry = () => {}, copy = (_text: string) => Promise.resolve()) {
  const target = document.createElement("div");
  document.body.append(target);
  const app = mount(Booting, {
    target,
    props: { db, version: "0.1.0", demo: true, onretry, copy },
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
  const { updateSample: fakeUpdate } = await import("../tf/settings/update-samples");
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

  // Mutant: draw the line only with an offer, and these rooms are gone.
  const shown = (target: HTMLElement) => {
    const room = target.querySelector<HTMLElement>("[data-testid=boot-update]");
    return room !== null && !room.hidden && room.closest("[hidden]") === null && room.classList.contains("boot-upd");
  };
  const none = rendered(fakeUpdate("up-to-date"));
  expect(shown(none.target)).toBe(true);
  expect(none.target.querySelector("[data-testid=boot-update]")?.textContent).toContain("No newer version yet.");
  none.done();
  const unknown = rendered(null);
  expect(shown(unknown.target)).toBe(true);
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

/** The button whose text is `label`. */
function button(target: HTMLElement, label: string): HTMLButtonElement | undefined {
  return [...target.querySelectorAll("button")].find((b) => b.textContent?.trim() === label);
}

/**
 * ux28: a machine whose firewall or security software blocks knobas from its
 * own database. The screen names the cause in words, then what knobas tried
 * and which programs an administrator can allow, as paths; the raw message
 * stays under them.
 */
test("a blocked start says so in words, with what was tried and what to allow", () => {
  const screen = render({
    state: "failed",
    message: "knobas could not connect to its own database server (tcp 127.0.0.1:54321): os error 10013",
    details: "knobas 0.1.0 · profile demo · Windows 11\nfailed at: knobas connecting to its server",
    blocked: {
      cause: "Windows or security software on this computer blocked knobas from reaching its own database.",
      tried: ["tcp 127.0.0.1:54321"],
      allow: ["C:\\Program Files\\knobas\\knobas-app.exe", "C:\\Users\\b\\.theseus\\postgresql\\18.6.0\\bin\\postgres.exe"],
    },
  });
  const text = screen.text();
  expect(text).toContain("Windows or security software on this computer blocked knobas from reaching its own database.");
  expect(text).toContain("knobas tried");
  expect(text).toContain("An administrator can allow these programs:");
  const items = [...screen.target.querySelectorAll("li")].map((li) => li.textContent);
  expect(items).toEqual([
    "tcp 127.0.0.1:54321",
    "C:\\Program Files\\knobas\\knobas-app.exe",
    "C:\\Users\\b\\.theseus\\postgresql\\18.6.0\\bin\\postgres.exe",
  ]);
  // The paths are values to copy, so they are in the code face; the cause is prose.
  for (const list of screen.target.querySelectorAll("ul")) expect(mono(list)).toBe(true);
  const cause = [...screen.target.querySelectorAll("p")].find((p) => p.textContent?.startsWith("Windows or"));
  expect(mono(cause!)).toBe(false);
  expect(text).toContain("os error 10013");
  expect(button(screen.target, "Retry")).toBeDefined();
  screen.done();
});

/** Any other failure keeps the screen it had: no cause block. */
test("a failure that is not a block shows no cause block", () => {
  const screen = render({ state: "failed", message: "port 50861 is in use", details: "x" });
  expect(screen.target.querySelectorAll("ul")).toHaveLength(0);
  expect(screen.text()).not.toContain("administrator");
  screen.done();
});

/** ux28's Windows pass: the server's own last lines, under the message, the last six only, as text in mono; none without them. */
test("what PostgreSQL said is drawn under the message, its last six lines", () => {
  const said = Array.from({ length: 8 }, (_, n) => `LOG: line ${n} <i>x</i>`);
  said.push('FATAL: configuration file "C:/x/postgresql.conf" contains errors');
  const screen = render({ state: "failed", message: "did not start", said });
  const list = screen.target.querySelector('[data-testid="boot-said"]');
  expect(list).not.toBeNull();
  const items = [...list!.querySelectorAll("li")].map((li) => li.textContent);
  expect(items).toEqual([...said.slice(-6)]);
  expect(list!.querySelector("i")).toBeNull();
  expect(mono(list as HTMLElement)).toBe(true);
  const lead = list!.previousElementSibling;
  expect(lead?.textContent).toBe("PostgreSQL said");
  expect(lead?.previousElementSibling?.textContent).toBe("did not start");
  screen.done();

  const none = render({ state: "failed", message: "did not start" });
  expect(none.target.querySelector('[data-testid="boot-said"]')).toBeNull();
  expect(none.text()).not.toContain("PostgreSQL said");
  none.done();
});

/** *Copy details* copies the backend's report, and says it did. */
test("Copy details copies the start's report and says Copied", async () => {
  const copy = vi.fn((_text: string) => Promise.resolve());
  const details = "knobas 0.1.0 · profile demo · Windows 11\nfailed at: knobas connecting to its server";
  const screen = render({ state: "failed", message: "boom", details }, () => {}, copy);
  button(screen.target, "Copy details")!.click();
  await vi.waitFor(() => expect(button(screen.target, "Copied")).toBeDefined());
  expect(copy).toHaveBeenCalledWith(details);
  screen.done();
});

/** With no report behind the failure, it copies the message and the version line. */
test("Copy details without a report copies the message and the version line", async () => {
  const copy = vi.fn((_text: string) => Promise.resolve());
  const screen = render({ state: "failed", message: "the server stopped" }, () => {}, copy);
  button(screen.target, "Copy details")!.click();
  await vi.waitFor(() => expect(copy).toHaveBeenCalledOnce());
  expect(copy).toHaveBeenCalledWith("the server stopped\nknobas 0.1.0 · profile demo");
  screen.done();
});

/** A clipboard that refuses says so on the button rather than claiming success. */
test("a refused copy says Could not copy", async () => {
  const copy = vi.fn((_text: string) => Promise.reject(new Error("denied")));
  const screen = render({ state: "failed", message: "boom", details: "d" }, () => {}, copy);
  button(screen.target, "Copy details")!.click();
  await vi.waitFor(() => expect(button(screen.target, "Could not copy")).toBeDefined());
  screen.done();
});
