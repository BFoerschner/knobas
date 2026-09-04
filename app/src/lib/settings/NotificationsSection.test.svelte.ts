/**
 * The desktop-notification toggles (issue #290).
 *
 * The seam is the one the rest of the settings view uses: a rendered section
 * in, user-visible text and store calls out. What the *rules* do with those
 * calls is `inbox/notify.test.svelte.ts`'s — the gates, the permission, the
 * once-per-item memory. What is asserted here is that the section draws what
 * is **stored**, sends what was clicked, and says out loud what the reader is
 * entitled to know before they switch anything on.
 *
 * The store is a real `createNotifications` over fake ports rather than a
 * hand-written stand-in, so the checkbox this file clicks reaches the same
 * rules the listener obeys. A stub here would let the section and the store
 * disagree about what "on" means and both stay green.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { createNotifications, type NotifyPorts } from "../inbox/notify.svelte";
import type { InboxCategory } from "../ipc/entity";
import NotificationsSection from "./NotificationsSection.svelte";

let target: HTMLDivElement;
let app: Record<string, unknown> | undefined;

function render(
  options: {
    stored?: InboxCategory[];
    granted?: boolean;
    answer?: string;
    ports?: Partial<NotifyPorts>;
  } = {},
) {
  const calls = { stored: [] as InboxCategory[][], asked: 0 };
  const store = createNotifications({
    notificationKinds: () => Promise.resolve(options.stored ?? []),
    setNotificationKinds: (kinds) => {
      calls.stored.push(kinds);
      return Promise.resolve(kinds);
    },
    isPermissionGranted: () => Promise.resolve(options.granted ?? false),
    requestPermission: () => {
      calls.asked += 1;
      return Promise.resolve(options.answer ?? "granted");
    },
    send: () => {},
    onAction: () => Promise.resolve(() => {}),
    focused: () => false,
    navigate: () => {},
    ...options.ports,
  });

  app = mount(NotificationsSection, { target, props: { store } });
  flushSync();
  return { calls, store };
}

/** The checkbox whose label reads exactly this. */
function box(label: string): HTMLInputElement {
  const found = [...target.querySelectorAll<HTMLLabelElement>("label.chk")].find(
    (candidate) => candidate.textContent?.trim() === label,
  );
  if (!found) {
    throw new Error(
      `no toggle reading ${label}; the section offers ${[
        ...target.querySelectorAll("label.chk"),
      ]
        .map((candidate) => candidate.textContent?.trim())
        .join(" | ")}`,
    );
  }
  return found.querySelector<HTMLInputElement>('input[type="checkbox"]')!;
}

function boxes(): string[] {
  return [...target.querySelectorAll<HTMLLabelElement>("label.chk")].map(
    (candidate) => candidate.textContent?.trim() ?? "",
  );
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
 * One switch per inbox category, and the reader is told the two rules that
 * decide whether anything ever fires: only while the window is not focused,
 * and only once per item.
 */
test("the section offers one switch per category and says when a desktop notification fires", async () => {
  render();
  await vi.waitFor(() => expect(boxes()).toHaveLength(5));

  expect(boxes()).toEqual([
    "Review requests",
    "Mentions",
    "Failed builds",
    "New assignments",
    "Credentials about to expire",
  ]);
  expect(text()).toContain("not");
  expect(text()).toContain("focused");
  expect(text()).toContain("already been told about");
  expect(text(), "the copy spells desktop notification in full").toContain("desktop notification");
  expect(text(), "the copy says category, not kind, for the switch").toContain("Every category");
});

/** All off is what a profile nobody has opted in on draws (story 71). */
test("a profile nobody has opted in on draws every switch off", async () => {
  render();
  await vi.waitFor(() => expect(boxes()).toHaveLength(5));

  for (const label of boxes()) {
    expect(box(label).checked, `${label} is on for a profile nobody opted in on`).toBe(false);
  }
});

test("the switches draw the stored kinds", async () => {
  render({ stored: ["mention", "failed_build"] });
  await vi.waitFor(() => expect(boxes()).toHaveLength(5));

  expect(box("Mentions").checked).toBe(true);
  expect(box("Failed builds").checked).toBe(true);
  expect(box("Review requests").checked).toBe(false);
});

/**
 * **Switching the first one on asks the operating system and says what it
 * answered** (story 72).
 */
test("switching the first category on asks the OS, stores it, and reports the outcome", async () => {
  const { calls } = render();
  await vi.waitFor(() => expect(boxes()).toHaveLength(5));

  box("Failed builds").click();
  await vi.waitFor(() => {
    flushSync();
    expect(calls.stored).toEqual([["failed_build"]]);
  });
  expect(calls.asked).toBe(1);
  expect(box("Failed builds").checked).toBe(true);
  expect(text()).toContain("allows knobas to notify you");
});

/**
 * **A refusal must not leave the switch reading on.**
 *
 * This is the direction that is only safe on purpose. A checkbox drawn on over
 * a permission the OS refused is a switch that promises something nothing will
 * deliver, and the reader has no way to tell it from a working one — they
 * would find out by not being told about a failed build.
 */
test("an OS that refuses leaves every switch off and says so", async () => {
  const { calls } = render({ answer: "denied" });
  await vi.waitFor(() => expect(boxes()).toHaveLength(5));

  box("Mentions").click();
  await vi.waitFor(() => {
    flushSync();
    expect(text()).toContain("refused notifications for knobas");
  });
  expect(calls.asked).toBe(1);
  expect(calls.stored, "a refused permission wrote a setting").toEqual([]);
  expect(box("Mentions").checked, "the switch reads on over a refused permission").toBe(false);
});

/**
 * **The outcome line is reachable on the platform knobas ships**, which is the
 * half of story 72 a stubbed test can most easily fake.
 *
 * The desktop plugin answers `permission_state()` with `Granted` whatever the
 * OS thinks, so a first switch-on against an already-granted permission is the
 * *ordinary* macOS case — and it still has to say what happened, or the
 * criterion's "shows the outcome" is a paragraph nobody ever sees.
 */
test("the first switch-on against an already-granted permission still says so", async () => {
  const { calls } = render({ granted: true });
  await vi.waitFor(() => expect(boxes()).toHaveLength(5));

  box("Review requests").click();
  await vi.waitFor(() => {
    flushSync();
    expect(text()).toContain("allows knobas to notify you");
  });
  expect(calls.asked).toBe(1);
  expect(calls.stored).toEqual([["review_request"]]);
});

/** Switching one off sends the rest and asks the OS nothing. */
test("switching a category off sends what is left and prompts nobody", async () => {
  const { calls } = render({ stored: ["review_request", "mention"], granted: true });
  await vi.waitFor(() => expect(boxes()).toHaveLength(5));

  box("Mentions").click();
  await vi.waitFor(() => {
    flushSync();
    expect(calls.stored).toEqual([["review_request"]]);
  });
  expect(calls.asked).toBe(0);
  expect(box("Mentions").checked).toBe(false);
});

/**
 * A write that failed leaves the switches reading what is stored and says why
 * — the rule `PassiveSection` records for the same screen.
 */
test("a write that is refused says why rather than claiming the category is on", async () => {
  render({
    granted: true,
    ports: {
      setNotificationKinds: () =>
        Promise.reject({ code: "not_ready", message: "the database is still starting" }),
    },
  });
  await vi.waitFor(() => expect(boxes()).toHaveLength(5));

  box("Mentions").click();
  await vi.waitFor(() => {
    flushSync();
    expect(text()).toContain("the database is still starting");
  });
  expect(boxes(), "the switches are behind the failure, not left claiming a state").toEqual([]);
});

/**
 * A read that failed is not "every category off": *off* is a claim about what is
 * stored, and a section that could not ask has not earned it. Retry asks
 * again.
 */
test("a read that failed offers Retry rather than drawing five switches off", async () => {
  let attempts = 0;
  render({
    ports: {
      notificationKinds: () => {
        attempts += 1;
        return attempts === 1
          ? Promise.reject({ code: "not_ready", message: "the database is still starting" })
          : Promise.resolve(["mention"] as InboxCategory[]);
      },
    },
  });

  await vi.waitFor(() => {
    flushSync();
    expect(text()).toContain("the database is still starting");
  });
  expect(boxes()).toEqual([]);

  [...target.querySelectorAll<HTMLButtonElement>("button")]
    .find((candidate) => candidate.textContent?.trim() === "Retry")!
    .click();

  await vi.waitFor(() => {
    flushSync();
    expect(box("Mentions").checked).toBe(true);
  });
});
