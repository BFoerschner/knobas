/**
 * The notification listener: the three gates, the click, and the permission
 * (issue #290, spec #272 story 70–74).
 *
 * The seam is the store, because that is where the rules live — the settings
 * section draws checkboxes and `App.svelte` hands it the stream, and neither
 * decides anything. Every port is a fake, so nothing here needs
 * `window.__TAURI_INTERNALS__` and the focus is dictated rather than waited
 * for: the plugin is stubbed exactly as the ticket asks.
 *
 * **Every silence in this file is asserted against a positive control.** A
 * test that only says "nothing was sent" cannot tell a working gate from a
 * broken bench, and three of the four gates here are silences — so each one
 * runs the same fixture twice, once with the gate closed and once with it
 * open, and the second half is what makes the first half mean something.
 */
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { InboxCategory, InboxEntry } from "../ipc/entity";
import { addressOf, createNotifications, type Notification, type NotifyPorts } from "./notify.svelte";

/** One line of the stream, of `category`, about `entityId`. */
function entry(
  category: InboxCategory,
  key: string,
  entityId: string | null = "gitea:acme/payouts#144",
): InboxEntry {
  return {
    item: {
      key: `${category}:${key}`,
      category,
      source_id: "gitea",
      entity_id: entityId,
      kind: entityId === null ? null : "pr",
      title: `Title of ${key}`,
      reason: `Why ${key} is here`,
      occurred_at: "2026-09-03T09:00:00Z",
      web_url: null,
      snoozed_until: null,
    },
    actions: [],
  };
}

/**
 * A bench with every port recorded and nothing behind any of them.
 *
 * `focused` starts **false**, because the interesting half of this store is
 * what an unfocused window does; the focus test sets it the other way round
 * and says so.
 */
function bench(overrides: Partial<NotifyPorts> = {}) {
  const calls = {
    sent: [] as Notification[],
    stored: [] as InboxCategory[][],
    asked: 0,
    checked: 0,
    navigated: [] as string[],
  };
  let focused = false;
  let granted = false;
  let answer = "granted";
  let stored: InboxCategory[] = [];
  let action: ((notification: { extra?: Record<string, unknown> }) => void) | null = null;

  const ports: Partial<NotifyPorts> = {
    notificationKinds: () => Promise.resolve(stored),
    setNotificationKinds: (kinds) => {
      calls.stored.push(kinds);
      return Promise.resolve(kinds);
    },
    isPermissionGranted: () => {
      calls.checked += 1;
      return Promise.resolve(granted);
    },
    requestPermission: () => {
      calls.asked += 1;
      return Promise.resolve(answer);
    },
    send: (notification) => calls.sent.push(notification),
    onAction: (handler) => {
      action = handler;
      return Promise.resolve(() => {
        action = null;
      });
    },
    focused: () => focused,
    navigate: (hash) => calls.navigated.push(hash),
    ...overrides,
  };

  const store = createNotifications(ports);
  return {
    store,
    calls,
    focus: (to: boolean) => {
      focused = to;
    },
    alreadyGranted: (to: boolean) => {
      granted = to;
    },
    answers: (to: string) => {
      answer = to;
    },
    /** Deliver a click on a notification the OS is showing. */
    click: (notification: { extra?: Record<string, unknown> }) => action?.(notification),
    /** Prime the store past the backlog, the way the first read does. */
    async primed(kinds: InboxCategory[] = [], seed: InboxEntry[] = []) {
      stored = kinds;
      await store.reseed();
      store.saw(seed);
    },
  };
}

let cleanup: (() => void) | undefined;

beforeEach(() => {
  cleanup = undefined;
});

afterEach(() => {
  cleanup?.();
});

// -- the kind gate ----------------------------------------------------------

/**
 * **A kind that is off is silent, and the same item with its kind on is not.**
 *
 * The two halves are one test on purpose. The silence alone would pass against
 * a bench whose `send` port was never wired, against a store that notified for
 * nothing at all, and against a permission this store does not even read —
 * which is the witness question this test exists to answer: nothing in the
 * notify path asks about permission, so the first half's silence can only be
 * the kind.
 */
test("an item of a kind nobody switched on says nothing, and the same item of a kind they did speaks", async () => {
  const off = bench();
  await off.primed(["mention"]);
  off.store.saw([entry("failed_build", "build-9")]);
  expect(off.calls.sent, "failed builds are switched off").toEqual([]);
  expect(off.calls.checked + off.calls.asked, "the notify path never asks the OS").toBe(0);

  const on = bench();
  await on.primed(["failed_build"]);
  on.store.saw([entry("failed_build", "build-9")]);
  expect(on.calls.sent.map((notification) => notification.title)).toEqual(["Title of build-9"]);
});

/** Only the kinds that are on, out of a stream carrying several. */
test("a stream of mixed kinds notifies for the switched-on ones only", async () => {
  const b = bench();
  await b.primed(["mention", "credential_expiry"]);
  b.store.saw([
    entry("failed_build", "build-9"),
    entry("mention", "PAY-231"),
    entry("review_request", "pr-12"),
    entry("credential_expiry", "gitea", null),
  ]);
  expect(b.calls.sent.map((notification) => notification.title)).toEqual([
    "Title of PAY-231",
    "Title of gitea",
  ]);
});

// -- the focus gate ---------------------------------------------------------

/**
 * **A focused window is told nothing, and the same arrival on an unfocused one
 * is announced** (story 74).
 *
 * The silence is asserted, not merely un-asserted: `sent` is compared against
 * the empty list rather than left unread, and the second half runs the same
 * fixture with the focus the other way round so that the first half cannot be
 * green because the bench was broken.
 */
test("nothing is sent while the window is focused, and the same item is sent while it is not", async () => {
  const looking = bench();
  looking.focus(true);
  await looking.primed(["mention"]);
  looking.store.saw([entry("mention", "PAY-231")]);
  expect(looking.calls.sent, "the reader is looking at the window").toEqual([]);

  const away = bench();
  away.focus(false);
  await away.primed(["mention"]);
  away.store.saw([entry("mention", "PAY-231")]);
  expect(away.calls.sent).toHaveLength(1);
});

/**
 * An item that arrived while the reader was looking is **not** announced when
 * they later switch away.
 *
 * The gate is about the moment of arrival, not about the moment of the next
 * signal. A notification held back until the blur would be knobas telling
 * somebody about something they have already read — the failure story 74 is
 * about, arriving one step later than the naive test would look for it.
 */
test("an item seen while focused is not announced by the next signal after a blur", async () => {
  const b = bench();
  b.focus(true);
  await b.primed(["mention"]);
  b.store.saw([entry("mention", "PAY-231")]);

  b.focus(false);
  b.store.saw([entry("mention", "PAY-231")]);
  expect(b.calls.sent).toEqual([]);

  // ...and the window is still working: a genuinely new item speaks.
  b.store.saw([entry("mention", "PAY-231"), entry("mention", "PAY-999")]);
  expect(b.calls.sent.map((notification) => notification.title)).toEqual(["Title of PAY-999"]);
});

// -- once per item ----------------------------------------------------------

/** The same item twice **in one stream** is one notification. */
test("one item repeated inside a single signal notifies once", async () => {
  const b = bench();
  await b.primed(["mention"]);
  b.store.saw([entry("mention", "PAY-231"), entry("mention", "PAY-231")]);
  expect(b.calls.sent).toHaveLength(1);
});

/**
 * The same item **across two signals** is one notification.
 *
 * This is the direction that actually happens: the inbox re-reads the whole
 * stream on every `activity:new` and every finished sync, so an item that
 * needs the reader for a week is in every one of those reads. A store that
 * remembered nothing between calls would announce it on each.
 */
test("one item arriving in two signals notifies once", async () => {
  const b = bench();
  await b.primed(["review_request"]);
  const stream = [entry("review_request", "pr-12")];
  b.store.saw(stream);
  b.store.saw(stream);
  b.store.saw([...stream, entry("review_request", "pr-12")]);
  expect(b.calls.sent).toHaveLength(1);
});

/**
 * An item that **leaves and comes back** is still one notification this
 * session.
 *
 * The key is the demand (`<category>:<subject>`) and not the occurrence, so a
 * mention marked done that returns when somebody writes again is the same key.
 * Announcing it twice is what "once per item" forbids; a restart is where it
 * gets to speak up again.
 */
test("an item that leaves the stream and returns is not announced twice", async () => {
  const b = bench();
  await b.primed(["mention"]);
  b.store.saw([entry("mention", "PAY-231")]);
  b.store.saw([]);
  b.store.saw([entry("mention", "PAY-231")]);
  expect(b.calls.sent).toHaveLength(1);
});

/**
 * **The backlog is recorded, not announced.**
 *
 * The first stream is everything that needs the reader now, including what
 * arrived while knobas was shut. A notification per line of it is a burst
 * about nothing new — and it would land on a reader who has just opened the
 * app, which is the one moment they are least in need of being told.
 */
test("the first stream primes and says nothing, and the next arrival speaks", async () => {
  const b = bench();
  await b.store.reseed();
  b.store.saw([]);
  const first = bench();
  await first.primed(["mention"], [entry("mention", "PAY-1"), entry("mention", "PAY-2")]);
  expect(first.calls.sent, "the backlog is not news").toEqual([]);

  first.store.saw([entry("mention", "PAY-3"), entry("mention", "PAY-1")]);
  expect(first.calls.sent.map((notification) => notification.title)).toEqual(["Title of PAY-3"]);
});

/**
 * **Nothing is seen before the setting has been read**, and the item that
 * arrived while the read was in flight is still announced afterwards.
 *
 * `seen` is a one-way memory, so this is the one direction that cannot be
 * undone: a store that recorded the stream against an unknown setting would
 * cost the reader every item that arrived during a slow read — or, if the read
 * failed and is sitting behind the section's *Retry*, every item until they
 * notice.
 */
test("a stream offered before the setting has been read costs the reader nothing", async () => {
  let answer: (kinds: InboxCategory[]) => void = () => {};
  const b = bench({
    notificationKinds: () =>
      new Promise<InboxCategory[]>((resolve) => {
        answer = resolve;
      }),
  });

  void b.store.reseed();
  b.store.saw([entry("mention", "PAY-231")]);
  expect(b.store.loaded).toBe(false);

  answer(["mention"]);
  await vi.waitFor(() => expect(b.store.loaded).toBe(true));

  // The first stream after the read is the backlog and primes; the item is
  // still in it, so it is announced by the next one rather than lost.
  b.store.saw([entry("mention", "PAY-231")]);
  expect(b.calls.sent).toEqual([]);
  b.store.saw([entry("mention", "PAY-231"), entry("mention", "PAY-999")]);
  expect(b.calls.sent.map((notification) => notification.title)).toEqual(["Title of PAY-999"]);
});

/** A read that failed leaves the same door shut, for the same reason. */
test("a stream offered while the setting is unreadable is not burned", async () => {
  const b = bench({
    notificationKinds: () =>
      Promise.reject({ code: "not_ready", message: "the database is still starting" }),
  });
  await b.store.reseed();
  b.store.saw([entry("mention", "PAY-231")]);
  expect(b.calls.sent).toEqual([]);
  expect(b.store.loaded).toBe(false);
});

// -- the click --------------------------------------------------------------

/** The address a notification carries is the item's own. */
test("a notification carries the entity's address and its click navigates there", async () => {
  const b = bench();
  cleanup = b.store.start();
  await b.primed(["review_request"]);
  b.store.saw([entry("review_request", "pr-12")]);

  const sent = b.calls.sent[0]!;
  expect(sent.extra["address"]).toBe("#/entity/gitea:acme%2Fpayouts%23144");

  b.click(sent);
  expect(b.calls.navigated).toEqual(["#/entity/gitea:acme%2Fpayouts%23144"]);
});

/**
 * A credential expiry has no entity, so its door is the inbox.
 *
 * The alternative — an `#/entity/<source id>` address — is a detail slide-over
 * over a row `get_entity` has never heard of.
 */
test("an item with no entity navigates to the inbox rather than to a made-up address", async () => {
  const b = bench();
  cleanup = b.store.start();
  await b.primed(["credential_expiry"]);
  b.store.saw([entry("credential_expiry", "gitea", null)]);

  b.click(b.calls.sent[0]!);
  expect(b.calls.navigated).toEqual(["#/inbox"]);
});

/** A click carrying nothing knobas can read navigates nowhere. */
test("a click with no address on it moves the window nowhere", async () => {
  const b = bench();
  cleanup = b.store.start();
  b.click({});
  b.click({ extra: {} });
  b.click({ extra: { address: 7 } });
  expect(b.calls.navigated).toEqual([]);
});

/** After the teardown, a late click is not a navigation. */
test("a click after the listener is torn down moves the window nowhere", async () => {
  const b = bench();
  const stop = b.store.start();
  await b.primed(["mention"]);
  b.store.saw([entry("mention", "PAY-231")]);
  const sent = b.calls.sent[0]!;
  stop();
  b.click(sent);
  expect(b.calls.navigated).toEqual([]);
});

/**
 * The desktop plugin has **no click channel**, so the subscription rejects —
 * and the window still works.
 *
 * `tauri-plugin-notification` 2.4.0 registers three commands on desktop and
 * `register_listener` is not one of them, so this is the ordinary case on
 * macOS rather than an exotic failure. A store that let it escape would take
 * the shell's `onMount` down with it.
 */
test("a click channel that refuses to open costs nothing", async () => {
  const b = bench({ onAction: () => Promise.reject(new Error("command not found")) });
  cleanup = b.store.start();
  await b.primed(["mention"]);
  b.store.saw([entry("mention", "PAY-231")]);
  await vi.waitFor(() => expect(b.calls.sent).toHaveLength(1));
});

// -- the permission ---------------------------------------------------------

/**
 * **Switching the first kind on asks the OS, and stores what it is told**
 * (story 72).
 *
 * Both directions, because only one of them is safe by accident: a grant has
 * to reach the stored setting, and a refusal must *not*. A toggle drawn on
 * over a refused permission is a switch that promises something nothing will
 * deliver, and the reader would have no way to tell it from a working one.
 */
test("switching the first kind on asks the OS and stores the kind once it says yes", async () => {
  const b = bench();
  await b.store.reseed();
  await b.store.choose("mention", true);

  expect(b.calls.asked).toBe(1);
  expect(b.calls.stored).toEqual([["mention"]]);
  expect(b.store.kinds).toEqual(["mention"]);
  expect(b.store.permission).toBe("granted");
});

test("an OS that refuses stores nothing and leaves the kind off", async () => {
  const b = bench();
  b.answers("denied");
  await b.store.reseed();
  await b.store.choose("mention", true);

  expect(b.calls.asked).toBe(1);
  expect(b.calls.stored, "a refused permission wrote a setting").toEqual([]);
  expect(b.store.kinds).toEqual([]);
  expect(b.store.permission).toBe("refused");
});

/** And a refusal is silent for real: nothing may notify afterwards. */
test("nothing notifies after the permission was refused", async () => {
  const b = bench();
  b.answers("denied");
  await b.store.reseed();
  await b.store.choose("failed_build", true);
  b.store.saw([]);
  b.store.saw([entry("failed_build", "build-9")]);
  expect(b.calls.sent).toEqual([]);
});

/**
 * **The first kind asks even where the OS has already said yes**, and that is
 * not pedantry about the criterion's wording.
 *
 * `tauri-plugin-notification`'s desktop implementation answers
 * `permission_state()` with `Granted` unconditionally, so a guard that only
 * asked when the answer was *no* would leave `requestPermission` unreached on
 * macOS — and with it the line that tells the reader what their operating
 * system said. Story 72 asks for the prompt on the switch, not on a refusal.
 */
test("switching the first kind on asks even when the OS already says yes", async () => {
  const b = bench();
  b.alreadyGranted(true);
  await b.store.reseed();
  await b.store.choose("mention", true);

  expect(b.calls.asked).toBe(1);
  expect(b.store.permission).toBe("granted");
  expect(b.calls.stored).toEqual([["mention"]]);
});

/**
 * A *second* kind does not ask again — the OS has already said yes, and story
 * 72 asks for the prompt when a kind is switched on, not on every click.
 */
test("switching a second kind on does not ask the OS again", async () => {
  const b = bench();
  await b.store.reseed();
  await b.store.choose("mention", true);
  b.alreadyGranted(true);
  await b.store.choose("failed_build", true);

  expect(b.calls.asked).toBe(1);
  expect(b.calls.stored.at(-1)).toEqual(["mention", "failed_build"]);
});

/** Switching one **off** never prompts anybody. */
test("switching a kind off asks the OS nothing", async () => {
  const b = bench();
  b.alreadyGranted(true);
  await b.primed(["mention", "failed_build"]);
  await b.store.choose("mention", false);

  expect(b.calls.asked).toBe(0);
  expect(b.calls.checked, "switching off does not even look").toBe(0);
  expect(b.calls.stored).toEqual([["failed_build"]]);
});

/** The store draws what the backend says is stored, not what was clicked. */
test("the stored answer is what the store holds, not the click", async () => {
  const b = bench({
    setNotificationKinds: () => Promise.resolve(["failed_build", "mention"]),
  });
  b.alreadyGranted(true);
  await b.store.reseed();
  await b.store.choose("mention", true);
  expect(b.store.kinds).toEqual(["failed_build", "mention"]);
});

/** A refused write says why and leaves the kinds where they were. */
test("a write that is refused says why and changes nothing", async () => {
  const b = bench({
    setNotificationKinds: () =>
      Promise.reject({ code: "not_ready", message: "the database is still starting" }),
  });
  b.alreadyGranted(true);
  await b.store.reseed();
  await b.store.choose("mention", true);

  expect(b.store.kinds).toEqual([]);
  expect(b.store.error).toBe("the database is still starting");
});

/**
 * A read that failed is not "every kind off": *off* is a claim about what is
 * stored, and a store that could not ask has not earned it.
 */
test("a read that failed leaves the setting unread rather than claiming it is empty", async () => {
  const b = bench({
    notificationKinds: () =>
      Promise.reject({ code: "not_ready", message: "the database is still starting" }),
  });
  await b.store.reseed();

  expect(b.store.loaded).toBe(false);
  expect(b.store.error).toBe("the database is still starting");
});

// -- the address ------------------------------------------------------------

/**
 * The address is built by the router's own `hashFor`, which is what makes an
 * entity key carrying `#` and `/` survive: unencoded, the first truncates the
 * fragment at the browser level and the second reads as another path segment.
 */
test("an entity key with a hash and a slash in it survives the address", () => {
  expect(addressOf(entry("review_request", "pr-12").item)).toBe(
    "#/entity/gitea:acme%2Fpayouts%23144",
  );
  expect(addressOf(entry("credential_expiry", "gitea", null).item)).toBe("#/inbox");
});
