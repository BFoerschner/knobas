/**
 * The desktop notification listener: the three gates, the click, and the
 * permission (issue #290, spec #272 story 70–74).
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
import type { NotificationClicked, NotificationDraft as WireDraft } from "../ipc/entity";
import { addressOf } from "./address";
import {
  clickChannel,
  createNotifications,
  sendThrough,
  type NotificationDraft,
  type NotifyPorts,
} from "./notify.svelte";

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
    sent: [] as NotificationDraft[],
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
    /** Deliver a click on a desktop notification the OS is showing. */
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

// -- the category gate ------------------------------------------------------

/**
 * **A category that is off is silent, and the same item with its category on
 * is not.**
 *
 * The two halves are one test on purpose. The silence alone would pass against
 * a bench whose `send` port was never wired, against a store that notified for
 * nothing at all, and against a permission this store does not even read —
 * which is the witness question this test exists to answer: nothing in the
 * notify path asks about permission, so the first half's silence can only be
 * the category.
 */
test("an item of a category nobody switched on says nothing, and the same item of a category they did speaks", async () => {
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

/** Only the categories that are on, out of a stream carrying several. */
test("a stream of mixed categories notifies for the switched-on ones only", async () => {
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
 * signal. A desktop notification held back until the blur would be knobas telling
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

/** The same item twice **in one stream** is one desktop notification. */
test("one item repeated inside a single signal notifies once", async () => {
  const b = bench();
  await b.primed(["mention"]);
  b.store.saw([entry("mention", "PAY-231"), entry("mention", "PAY-231")]);
  expect(b.calls.sent).toHaveLength(1);
});

/**
 * The same item **across two signals** is one desktop notification.
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
 * An item that **leaves and comes back** is still one desktop notification this
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
 * arrived while knobas was shut. A desktop notification per line of it is a burst
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

// -- the click --------------------------------------------------------------

/** The address a desktop notification carries is the item's own. */
test("a desktop notification carries the entity's address and its click navigates there", async () => {
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

// -- the real ports (#339) ---------------------------------------------------

/**
 * **A `notification:clicked` event navigates to the address it carries.**
 *
 * This is the real click channel, with Tauri's `listen` faked: the store
 * subscribes to `EVENTS.notificationClicked` and the event's `address` reaches
 * the same navigation the bench's `click` drives above. The event name is
 * asserted because a listener on the wrong name is a door that never opens
 * with nothing failing anywhere; the address is asserted because a channel
 * that handed the store `{ extra: {} }` would pass every other test here.
 */
test("a notification:clicked event navigates to the item it carries", async () => {
  let deliver: ((event: { payload: NotificationClicked }) => void) | null = null;
  const listened: string[] = [];
  const listen = (event: string, handler: (event: { payload: NotificationClicked }) => void) => {
    listened.push(event);
    deliver = handler;
    return Promise.resolve(() => {
      deliver = null;
    });
  };
  const b = bench({ onAction: clickChannel(listen) });
  const stop = b.store.start();
  await vi.waitFor(() => expect(deliver).not.toBeNull());
  expect(listened).toEqual(["notification:clicked"]);

  deliver!({ payload: { address: "#/entity/gitea:acme%2Fpayouts%23144" } });
  expect(b.calls.navigated).toEqual(["#/entity/gitea:acme%2Fpayouts%23144"]);

  // The teardown unlistens, so a late event is not a navigation.
  stop();
  expect(deliver, "the teardown reached Tauri's unlisten").toBeNull();
});

/**
 * The real `send` hands the `notify` command the draft's title, body and the
 * address out of `extra` — the one key the click path is built on. A draft
 * without one is not sent: a waiter keyed to nothing is a click that opens
 * nowhere.
 */
test("a sent desktop notification reaches the notify command with its address", async () => {
  const sent: WireDraft[] = [];
  const send = sendThrough((draft) => {
    sent.push(draft);
    return Promise.resolve();
  });
  const b = bench({ send });
  await b.primed(["review_request"]);
  b.store.saw([entry("review_request", "pr-12")]);
  expect(sent).toEqual([
    {
      title: "Title of pr-12",
      body: "Why pr-12 is here",
      address: "#/entity/gitea:acme%2Fpayouts%23144",
    },
  ]);

  send({ title: "no door", body: "", extra: {} });
  expect(sent, "a draft with no address is not sent").toHaveLength(1);
});

/** A refused send is not a broken window: the next item still goes out. */
test("a notify command that rejects costs nothing", async () => {
  let calls = 0;
  const b = bench({
    send: sendThrough(() => {
      calls += 1;
      return Promise.reject({ code: "internal", message: "No bundle identifier found." });
    }),
  });
  await b.primed(["mention"]);
  b.store.saw([entry("mention", "PAY-1")]);
  b.store.saw([entry("mention", "PAY-1"), entry("mention", "PAY-2")]);
  await vi.waitFor(() => expect(calls).toBe(2));
});

/**
 * A click channel that refuses to open costs nothing — and the window still
 * works.
 *
 * Before #339 this was the ordinary case on macOS: the plugin's
 * `register_listener` is mobile-only, so `onAction` rejected on every desktop
 * start. The channel is knobas' own event now and opens on desktop, but the
 * rule stands for the one case left (`listen` before the IPC is up, under
 * `?fake-ipc`): a store that let the rejection escape would take the shell's
 * `onMount` down with it.
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
 * **Switching the first category on asks the OS, and stores what it is told**
 * (story 72).
 *
 * Both directions, because only one of them is safe by accident: a grant has
 * to reach the stored setting, and a refusal must *not*. A toggle drawn on
 * over a refused permission is a switch that promises something nothing will
 * deliver, and the reader would have no way to tell it from a working one.
 */
test("switching the first category on asks the OS and stores the category once it says yes", async () => {
  const b = bench();
  await b.store.reseed();
  await b.store.choose("mention", true);

  expect(b.calls.asked).toBe(1);
  expect(b.calls.stored).toEqual([["mention"]]);
  expect(b.store.kinds).toEqual(["mention"]);
  expect(b.store.permission).toBe("granted");
});

test("an OS that refuses stores nothing and leaves the category off", async () => {
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
 * **The first category asks even where the OS has already said yes**, and that is
 * not pedantry about the criterion's wording.
 *
 * `tauri-plugin-notification`'s desktop implementation answers
 * `permission_state()` with `Granted` unconditionally, so a guard that only
 * asked when the answer was *no* would leave `requestPermission` unreached on
 * macOS — and with it the line that tells the reader what their operating
 * system said. Story 72 asks for the prompt on the switch, not on a refusal.
 */
test("switching the first category on asks even when the OS already says yes", async () => {
  const b = bench();
  b.alreadyGranted(true);
  await b.store.reseed();
  await b.store.choose("mention", true);

  expect(b.calls.asked).toBe(1);
  expect(b.store.permission).toBe("granted");
  expect(b.calls.stored).toEqual([["mention"]]);
});

/**
 * A *second* category does not ask again — the OS has already said yes, and
 * story 72 asks for the prompt when a category is switched on, not on every
 * click.
 */
test("switching a second category on does not ask the OS again", async () => {
  const b = bench();
  await b.store.reseed();
  await b.store.choose("mention", true);
  b.alreadyGranted(true);
  await b.store.choose("failed_build", true);

  expect(b.calls.asked).toBe(1);
  expect(b.calls.stored.at(-1)).toEqual(["mention", "failed_build"]);
});

/** Switching one **off** never prompts anybody. */
test("switching a category off asks the OS nothing", async () => {
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

/** A refused write says why and leaves the categories where they were. */
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
 * A read that failed is not "every category off": *off* is a claim about what is
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

// -- the alert (#447) -------------------------------------------------------

/**
 * One alert line, as the backend derives it (`knobas_core::inbox`'s `alert!`):
 * the **monitor** is the subject half of the key, the **asset** it watches is
 * the entity, and the reason names the asset's whole path.
 *
 * The same fixture `inbox.test.svelte.ts` reads, deliberately — the two
 * surfaces this category has are the inbox row and the desktop notification,
 * and a fixture that differed between them is how the two addresses drifted
 * apart in the first place.
 */
function alertEntry(monitor = "kuma:7", asset = "asset:hel1"): InboxEntry {
  return {
    item: {
      key: `alert:${monitor}`,
      category: "alert",
      source_id: "kuma",
      entity_id: asset,
      kind: "asset",
      title: "jira (tunnel)",
      reason: "hel / hel1 is down",
      occurred_at: "2026-09-07T09:00:00Z",
      web_url: null,
      snoozed_until: null,
    },
    actions: [],
  };
}

/**
 * **An alert's desktop notification opens the Tree at the affected asset**
 * (spec #427 story 61), which is the address the inbox's own *Open* uses and
 * not the `#/entity/<id>` room detail every other category opens.
 *
 * The click is asserted through as well as the draft: an address the store
 * writes and never navigates to would be half a door.
 */
test("an alert's desktop notification opens the Tree at the affected asset", async () => {
  const b = bench();
  cleanup = b.store.start();
  await b.primed(["alert"]);
  b.store.saw([alertEntry()]);

  const sent = b.calls.sent[0]!;
  expect(sent.extra["address"], "an alert opens the Tree, not a room detail").toBe(
    "#/asset/asset:hel1",
  );

  b.click(sent);
  expect(b.calls.navigated).toEqual(["#/asset/asset:hel1"]);
});

/**
 * **The sixth category is off until somebody switches it on, and the same
 * alert with it on speaks** (spec #427 story 63, and story 71's default).
 *
 * The negative is the half that matters: five categories switched on and the
 * alert not among them is the profile of somebody who opted in before this
 * build existed, and an alert that notified them anyway would be knobas
 * switching a category on by shipping it. The second half is what makes the
 * silence attributable to the category rather than to a bench that never
 * wired `send` -- the rule this whole file follows.
 */
test("an alert says nothing until its own category is switched on", async () => {
  const off = bench();
  await off.primed([
    "review_request",
    "mention",
    "failed_build",
    "new_assignment",
    "credential_expiry",
  ]);
  off.store.saw([alertEntry()]);
  expect(off.calls.sent, "the alert category is off, five others are on").toEqual([]);

  const on = bench();
  await on.primed(["alert"]);
  on.store.saw([alertEntry()]);
  expect(on.calls.sent.map((notification) => notification.body)).toEqual(["hel / hel1 is down"]);
});

/**
 * **One desktop notification per alert, and neither the ack nor the recovery
 * is another one** (stories 62 and 64).
 *
 * At this seam an ack and a recovery are the *same* event and the test says so
 * rather than pretending to tell them apart: nobody deletes an inbox item, the
 * rule stops matching -- `acked_at` for the one and `closed_at` for the other
 * -- and both reach this store as an item that is no longer in the stream.
 * Which departure is which is `crates/knobas-app/tests/inbox_ipc.rs`'
 * `an_alert_is_acked_from_the_inbox_and_recovery_takes_the_item_and_leaves_a_line`,
 * against the database, and it is the *only* place that distinction is
 * witnessed.
 *
 * The positive control is a second monitor arriving in the same stream after
 * both departures: without it, "nothing was sent" would also be true of a
 * store that had stopped sending anything at all.
 *
 * The last two lines are the session memory applied to this category, and the
 * consequence is worth reading twice: the key is `alert:<monitor>`, so a
 * monitor that goes down, recovers and goes down again announces **once per
 * session**, not once per outage. That is the rule every category has had
 * since #290 -- a restart is where the same demand gets to speak up again --
 * and a flapping monitor is where it costs the most.
 */
test("an alert notifies once, and neither its ack nor its recovery notifies again", async () => {
  const b = bench();
  await b.primed(["alert"]);

  b.store.saw([alertEntry()]);
  expect(b.calls.sent).toHaveLength(1);
  b.store.saw([alertEntry()]);
  expect(b.calls.sent, "the same open alert on the next read").toHaveLength(1);

  // Acked: the item leaves the stream while the alert stays open.
  b.store.saw([]);
  expect(b.calls.sent, "an ack is not an arrival").toHaveLength(1);

  // Recovered: the alert closes and takes the item with it, by construction.
  b.store.saw([]);
  expect(b.calls.sent, "a recovery is not an arrival").toHaveLength(1);

  // ...and the same monitor going down again is the same key, so it is still
  // the one desktop notification this session.
  b.store.saw([alertEntry()]);
  expect(b.calls.sent, "one key, one desktop notification a session").toHaveLength(1);

  // The control: a different monitor is a different key and does speak.
  b.store.saw([alertEntry(), alertEntry("kuma:9", "asset:hel2")]);
  expect(b.calls.sent).toHaveLength(2);
});
