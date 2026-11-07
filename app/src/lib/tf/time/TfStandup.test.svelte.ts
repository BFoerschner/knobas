/**
 * Time > Standup (#829): what a reader sees, and what each action sends. The
 * bridge is injected; what the backend does with the calls is
 * `crates/knobas-app/tests/it/standup_ipc.rs`'s and `protocol.rs`'s.
 */
import { flushSync, mount, tick, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { ActionItemTicket, DigestLine, LinkEnd, LinkEntry, Protocol, PublishTarget, RefusedWrite, StandupDigest, WriteAsk } from "../../ipc/entity";
import type { QueuedWrite } from "../../ipc/sources";
import { toasts } from "../../shell/toasts.svelte";
import { createLayers } from "../layers.svelte";
import TfStandup from "./TfStandup.svelte";

const NOW = new Date(2026, 7, 21, 9, 30, 0, 0);
const DAY = "2026-08-21";

const BODY = [
  `# Standup ${DAY}`,
  "",
  "## Attendees",
  "",
  "- Mara Lindqvist",
  "- Tomasz Wójcik",
  "",
  "## Notes",
  "",
  "- Mara Lindqvist: Review open.",
  "",
  "## Action items",
  "",
  "- [ ] Decide the manual-review SLA",
  "- [ ] Update the on-call runbook (Tomasz Wójcik)",
  "",
].join("\n");

/** The `related` link the backend draws from the protocol to a ticket it filed for an item (#779). */
function ticketLink(entityId: string, title: string, over: Partial<LinkEnd> = {}): LinkEntry {
  return {
    link: {
      id: `link:${entityId}`,
      from_id: "note:p",
      to_id: entityId,
      relation: "related",
      origin: "manual",
      note: null,
      created_by: "knobas",
      created_at: "",
      confirmed_at: "",
      rule: null,
      rule_class: null,
      reason: null,
    },
    other: { entity_id: entityId, kind: "ticket", title, deleted_at: null, ...over },
  };
}

function line(over: Partial<DigestLine>): DigestLine {
  return {
    entity_id: "jira:PAY-231",
    kind: "ticket",
    title: "Retry failed SEPA payouts",
    source: "jira",
    verb: "tracked",
    reason: "4h 30m",
    at: "2026-08-20T15:00:00Z",
    ...over,
  };
}

const DIGEST: StandupDigest = {
  yesterday_day: "2026-08-20",
  yesterday: [line({}), line({ entity_id: "confluence:98401", kind: "page", title: "SEPA retry design", reason: "Backoff policy" })],
  today: [line({ entity_id: null, kind: null, title: "DB config", reason: "timer running" })],
  blockers: [],
  infrastructure: [
    line({
      entity_id: "kuma:payout-api",
      kind: "monitor",
      title: "payout-api health check",
      source: "kuma",
      verb: "down",
      reason: "was down 12 min",
      at: "2026-08-20T03:10:00",
    }),
  ],
};

let target: HTMLDivElement;
let view: ReturnType<typeof mount> | null = null;

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
  target = document.createElement("div");
  document.body.append(target);
});
afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
  vi.useRealTimers();
});

async function settle() {
  for (let round = 0; round < 6; round++) {
    await tick();
    await Promise.resolve();
  }
  flushSync();
}

/**
 * The save pause out (`saveAfterMs: 0`, a timer all the same) and the save it
 * started settled. `setTimeout` is fake in this file, so the pause ends when
 * the clock is stepped, not after a sleep (#2048).
 */
async function pausedOut() {
  vi.advanceTimersByTime(0);
  await settle();
}

function open(
  over: {
    stored?: PublishTarget | null;
    publication?: Protocol["publication"];
    /** The protocol note's links, as the backend holds them; a landed create draws one into it. */
    links?: LinkEntry[];
    /** The day the address names (`#/time/standup/<date>`). */
    date?: string;
    /** The first write stalls until `release()`, and `body` is what the store holds when each write lands: a slow database. */
    slowFirstWrite?: boolean;
    /** The first write is refused. */
    failFirstWrite?: boolean;
    /** The creates the protocol's read names as still owing their link (#1951): filed before a reload. */
    owed?: ActionItemTicket[];
    /** Whether the mirror holds the filed ticket yet; flip `mirror.held` to bring it in (#1951). */
    mirrored?: boolean;
    /** What `refused_writes` answers (#2053, #2052); nothing refused when omitted. */
    refused?: (asks: WriteAsk[]) => Promise<RefusedWrite[]>;
    /** The read's `changed_since`, `page_moved` and `again` (F-327). */
    changed?: boolean;
    moved?: boolean;
    again?: Protocol["again"];
    /** *Publish again* is refused with this sentence. */
    againFails?: string;
  } = {},
) {
  const calls: {
    saved: string[];
    published: (PublishTarget | undefined)[];
    copied: string[];
    tickets: unknown[][];
    /** Every `action_item_ticket` asked, by write id. */
    asked: number[];
    days: string[];
    digests: string[];
    reads: number;
    writes: number;
    /** Every *Publish again*. */
    again: number;
  } = {
    reads: 0,
    writes: 0,
    again: 0,
    saved: [],
    published: [],
    copied: [],
    tickets: [],
    asked: [],
    days: [],
    digests: [],
  };
  const links = over.links ?? [];
  const hears = new Map<number, (row: QueuedWrite | undefined) => void>();
  const mirror = { held: over.mirrored ?? true };
  const heard = new Map<string, Set<(event: { payload: unknown }) => void>>();
  /** Send an event to the tab, as the backend would: `sync:state` with `running: false` is a sync that ended. */
  const emit = (event: string, payload: unknown) => {
    for (const handler of heard.get(event) ?? []) handler({ payload });
  };
  let body = BODY;
  let release = () => {};
  const stalled = new Promise<void>((done) => (release = done));
  const standing: Pick<Protocol, "changed_since" | "page_moved" | "again"> = {
    changed_since: over.changed ?? false,
    page_moved: over.moved ?? false,
    again: over.again ?? null,
  };
  const protocol = (publication: Protocol["publication"] = over.publication ?? null): Protocol => ({
    day: DAY,
    note_id: "note:p",
    page_title: DAY,
    publication,
    owed_tickets: mirror.held ? [] : (over.owed ?? []),
    ...standing,
  });
  view = mount(TfStandup, {
    target,
    props: {
      now: () => NOW,
      date: over.date ?? null,
      saveAfterMs: 0,
      layers: createLayers(),
      ports: {
        standupDigest: async (today: { day: string }) => {
          calls.digests.push(today.day);
          return DIGEST;
        },
        standupProtocol: async (day: string) => {
          calls.days.push(day);
          return protocol();
        },
        getNote: async () => {
          calls.reads += 1;
          return {
            note: { id: "note:p", title: `Standup ${DAY}`, body_md: body, created_at: "", updated_at: "" },
            refs: [],
            links: [...links],
          };
        },
        saveNote: async (_id: string, _title: string, text: string) => {
          calls.writes += 1;
          if (over.slowFirstWrite && calls.writes === 1) await stalled;
          if (over.failFirstWrite && calls.writes === 1) throw new Error("disk full");
          body = text;
          calls.saved.push(text);
          return { note: { id: "note:p", title: "", body_md: text, created_at: "", updated_at: "" }, refs: [], links: [] };
        },
        standupPublishTarget: async () => (over.stored === undefined ? { source_id: "wiki", parent: "wiki:900" } : over.stored),
        listSources: async () => [{ id: "wiki", display_name: "Confluence", adapter_kind: "confluence", enabled: true }],
        listAdapters: async () => [{ adapter_kind: "confluence", write_ops: ["create_page"] }],
        getEntity: async (id: string) => ({
          row: { entity_id: id, title: id === "wiki:900" ? "Standups" : "Runbook", state: { text: "To Do", tone: "neutral" } },
        }),
        publishStandupProtocol: async (_day: string, chosen?: PublishTarget) => {
          calls.published.push(chosen);
          return protocol({ write_id: 5, state: "pending", detail: null, page_entity_id: null, linked: false });
        },
        publishStandupProtocolAgain: async () => {
          calls.again += 1;
          if (over.againFails) throw { code: "invalid", message: over.againFails, source_id: null };
          standing.changed_since = false;
          standing.again = { write_id: 7, state: "pending", detail: null };
          return protocol();
        },
        listProjects: async () => [{ source_id: "jira", key: "OPS", name: "Operations" }],
        createActionItemTicket: async (...args: unknown[]) => {
          calls.tickets.push(args);
          return { write_id: 9, title: String(args[3]), state: "pending", may_have_landed: false, detail: null, ticket_entity_id: null, linked: false };
        },
        actionItemTicket: async (_note: string, writeId: number) => {
          calls.asked.push(writeId);
          const words = String(calls.tickets.at(-1)?.[3] ?? over.owed?.find((owed) => owed.write_id === writeId)?.title);
          // Sent, and the sync after it has not brought the ticket in.
          if (!mirror.held) return { write_id: writeId, title: words, state: "sent", may_have_landed: false, detail: null, ticket_entity_id: null, linked: false };
          // Landed: the backend draws the protocol's link to the ticket it found by the item's words.
          if (!links.some((entry) => entry.other.entity_id === "jira:OPS-131")) links.push(ticketLink("jira:OPS-131", words));
          return { write_id: writeId, title: words, state: "sent", may_have_landed: false, detail: null, ticket_entity_id: "jira:OPS-131", linked: true };
        },
        listen: async (event: string, handler: (event: { payload: unknown }) => void) => {
          const handlers = heard.get(event) ?? new Set();
          handlers.add(handler);
          heard.set(event, handlers);
          return () => handlers.delete(handler);
        },
        follower: { follow: (id: number, hear: (row: QueuedWrite | undefined) => void) => (hears.set(id, hear), () => hears.delete(id)) },
        copy: async (text: string) => {
          calls.copied.push(text);
        },
        refusedWrites: async (asks: WriteAsk[]) => (over.refused ? over.refused(asks) : []),
      } as never,
    },
  });
  return { calls, hears, mirror, emit, standing, release: () => release(), stored: () => body };
}

const text = () => target.textContent?.replace(/\s+/g, " ") ?? "";
const button = (name: string) =>
  [...target.querySelectorAll("button")].find((found) => found.textContent?.trim() === name) as HTMLButtonElement;

test("the digest draws each list, every line with an item a link into the new shell", async () => {
  open();
  await settle();
  expect(text()).toContain("Yesterday, Thursday 20");
  const links = [...target.querySelectorAll(".tf-su-dig a")].map((link) => link.getAttribute("href"));
  expect(links).toEqual(["#/ticket/jira%3APAY-231", "#/page/confluence%3A98401", "#/monitors/at/kuma%3Apayout-api"]);
  expect(text()).toContain("DB config (timer running)");
  expect(text()).toContain("Nothing of yours is blocked.");
});

test("Infrastructure sits under Blockers, every alert a link to its monitor", async () => {
  open();
  await settle();
  const headings = [...target.querySelectorAll(".tf-su-dig h4")].map((h) => h.textContent);
  expect(headings.slice(-2)).toEqual(["Blockers", "Infrastructure"]);
  const alert = [...target.querySelectorAll(".tf-su-dig a")].at(-1)!;
  expect(alert.getAttribute("href")).toBe("#/monitors/at/kuma%3Apayout-api");
  expect(alert.textContent?.replace(/\s+/g, " ")).toBe("payout-api health check was down 12 min, from 03:10");
});

test("Copy as text copies the digest", async () => {
  const { calls } = open();
  await settle();
  button("Copy as text").click();
  await settle();
  expect(calls.copied[0]).toContain("- PAY-231 Retry failed SEPA payouts: 4h 30m");
  expect(calls.copied[0]).toContain("Infrastructure\n- payout-api health check was down 12 min, from 03:10");
});

test("each panel heading's action is a plain small button on the heading line, not dressed as a link (#2288)", async () => {
  open();
  await settle();
  const headed = [...target.querySelectorAll(".tf-su .tf-panel > header .tf-btn")].map((b) => [b.textContent?.trim(), b.className]);
  expect(headed).toEqual([
    ["Copy as text", "tf-btn tf-sm tf-ghost"],
    ["Publish to Confluence", "tf-btn tf-sm tf-primary"],
  ]);
});

test("the protocol draws who was there, their notes, and the action items, from the note's body", async () => {
  open();
  await settle();
  const pressed = [...target.querySelectorAll<HTMLButtonElement>(".tf-su-people .tf-fchips button")].map((chip) => [
    chip.textContent?.trim(),
    chip.getAttribute("aria-pressed"),
  ]);
  expect(pressed).toEqual([
    ["Mara Lindqvist", "true"],
    ["Tomasz Wójcik", "true"],
  ]);
  const notes = [...target.querySelectorAll<HTMLTextAreaElement>(".tf-su-pn textarea")].map((box) => box.value);
  expect(notes).toEqual(["Review open.", ""]);
  expect(text()).toContain("Decide the manual-review SLA");
  expect(text()).toContain("Publishes under Confluence › Standups.");
});

test("pressing someone off saves the body without them, and they stay a chip to press on again", async () => {
  const { calls } = open();
  await settle();
  button("Tomasz Wójcik").click();
  await pausedOut();
  expect(calls.saved.at(-1)).toContain("## Attendees\n\n- Mara Lindqvist\n\n## Notes");
  expect(button("Tomasz Wójcik").getAttribute("aria-pressed")).toBe("false");
});

test("a note typed under someone's name is saved as their bullet", async () => {
  const { calls } = open();
  await settle();
  const box = target.querySelectorAll<HTMLTextAreaElement>(".tf-su-pn textarea")[1]!;
  box.value = "Runbook today";
  box.dispatchEvent(new Event("input", { bubbles: true }));
  await pausedOut();
  expect(calls.saved.at(-1)).toContain("- Mara Lindqvist: Review open.\n- Tomasz Wójcik: Runbook today\n");
});

test("Create ticket files the action item, shows it queued, then its key and status once it lands", async () => {
  const { calls, hears } = open();
  await settle();
  button("Create ticket…").click();
  await settle();
  const select = document.querySelector<HTMLSelectElement>("select[aria-label='Project the ticket goes in']")!;
  select.value = "jira:OPS";
  select.dispatchEvent(new Event("change", { bubbles: true }));
  flushSync();
  [...document.querySelectorAll<HTMLButtonElement>(".tf-dialog button")].find((found) => found.textContent?.trim() === "Create ticket")!.click();
  await settle();
  expect(calls.tickets[0]).toEqual(["note:p", "jira:OPS", "Task", "Decide the manual-review SLA", `From the standup protocol of ${DAY}.`]);
  expect(target.querySelector(".tf-su-ai .tf-chip")?.textContent).toBe("Queued");
  hears.get(9)!(undefined);
  await settle();
  expect(target.querySelector(".tf-su-ai a")?.getAttribute("href")).toBe("#/ticket/jira%3AOPS-131");
  expect(target.querySelector(".tf-su-ai .tf-chip")?.textContent).toBe("To do");
});

test("Create ticket in a project the source's credential cannot file in is drawn disabled saying why, and files nothing (#2052)", async () => {
  const reason = "This Jira user cannot create tickets in OPS: it lacks the Create Issues permission";
  const { calls } = open({ refused: () => Promise.resolve([{ entity: "jira:OPS", op: "create_ticket", reason }]) });
  await settle();
  button("Create ticket…").click();
  await settle();
  const select = document.querySelector<HTMLSelectElement>("select[aria-label='Project the ticket goes in']")!;
  select.value = "jira:OPS";
  select.dispatchEvent(new Event("change", { bubbles: true }));
  flushSync();
  const create = [...document.querySelectorAll<HTMLButtonElement>(".tf-dialog button")].find((found) => found.textContent?.trim() === "Create ticket")!;
  expect(create.getAttribute("aria-disabled")).toBe("true");
  expect(create.title).toBe(reason);
  create.click();
  await settle();
  expect(calls.tickets).toEqual([]);
});

test("Publish goes to the stored page without asking, after saving", async () => {
  const { calls } = open();
  await settle();
  button("Publish to Confluence").click();
  await settle();
  expect(calls.saved.length).toBe(1);
  expect(calls.published).toEqual([undefined]);
  // One button in one place: it says what its write is doing, and a second press lands on it.
  const press = button("Queued");
  expect(press.getAttribute("aria-disabled")).toBe("true");
  press.click();
  await settle();
  expect(calls.published).toEqual([undefined]);
});

test("Publish under a page the credential cannot add pages to is drawn disabled saying why, and publishes nothing (#2053)", async () => {
  const reason = "This Confluence user cannot add pages to Engineering: it lacks the Add page permission";
  const asked: WriteAsk[][] = [];
  const { calls } = open({
    refused: async (asks) => {
      asked.push(asks);
      return asks.map((ask) => ({ ...ask, reason }));
    },
  });
  await settle();
  expect(asked).toEqual([[{ entity: "wiki:900", op: "create_page" }]]);
  const publish = button("Publish to Confluence");
  expect(publish.getAttribute("aria-disabled")).toBe("true");
  expect(publish.title).toBe(reason);
  publish.click();
  await settle();
  expect(calls.published).toEqual([]);
  expect(calls.saved).toEqual([]);
});

test("Publish stays enabled where the source refuses nothing, or cannot tell (#2053)", async () => {
  open({ refused: async () => Promise.reject({ code: "not_ready", message: "no", source_id: null }) });
  await settle();
  const publish = button("Publish to Confluence");
  expect(publish.getAttribute("aria-disabled")).toBeNull();
  expect(publish.title).toBe("");
});

test("the first Publish asks where protocols go", async () => {
  const { calls } = open({ stored: null });
  await settle();
  expect(text()).toContain("The first publish asks where.");
  button("Publish to Confluence…").click();
  await settle();
  expect(document.querySelector(".tf-dialog h2")?.textContent).toBe("Where do standup protocols go?");
  expect(calls.published).toEqual([]);
});

test("a published protocol links its page", async () => {
  open({ publication: { write_id: 5, state: "sent", detail: null, page_entity_id: "wiki:901", linked: true } });
  await settle();
  const link = [...target.querySelectorAll("a")].find((found) => found.textContent?.trim() === "Open the published page");
  expect(link?.getAttribute("href")).toBe("#/page/wiki%3A901");
});

const PUBLISHED: Protocol["publication"] = { write_id: 5, state: "sent", detail: null, page_entity_id: "wiki:901", linked: true };
const pubLine = () => target.querySelector(".tf-su-publine span:not(.tf-off)")?.textContent?.trim();
const typeNote = async (words: string) => {
  const box = target.querySelector<HTMLTextAreaElement>(".tf-su-pn textarea")!;
  box.value = words;
  box.dispatchEvent(new Event("input", { bubbles: true }));
  await settle();
};

test("once published it says so, and a change offers Publish again in the same box (F-327)", async () => {
  const { standing } = open({ publication: PUBLISHED });
  await settle();
  expect(pubLine()).toContain("Published.");
  expect(button("Publish again")).toBeUndefined();
  expect(target.querySelector(".tf-su-foot")?.textContent).toContain("New protocols publish under");
  const box = target.querySelector(".tf-su-publine");
  standing.changed_since = true;
  await typeNote("Something new");
  expect(pubLine()).toContain("Changed since it was published");
  expect(target.querySelector(".tf-su-publine"), "the same box: nothing moves under the caret").toBe(box);
  expect(target.querySelectorAll(".tf-su-publine > span")).toHaveLength(3);
  expect(button("Publish again").getAttribute("aria-disabled")).toBeNull();
});

test("the read says the note moved on, so it survives leaving the tab and coming back (F-327)", async () => {
  open({ publication: PUBLISHED, changed: true });
  await settle();
  expect(pubLine()).toContain("Changed since it was published");
  expect(button("Publish again")).toBeDefined();
});

test("words typed and taken back read as the page again, once the save says so (F-327)", async () => {
  const { standing } = open({ publication: PUBLISHED });
  await settle();
  standing.changed_since = false;
  await typeNote("Back as it was");
  await vi.advanceTimersByTimeAsync(1);
  await settle();
  expect(pubLine()).toContain("Published.");
  expect(button("Publish again")).toBeUndefined();
});

test("Publish again saves, sends once, holds as Queued, and pressed again does nothing (F-327)", async () => {
  const { calls, hears, standing } = open({ publication: PUBLISHED, changed: true });
  await settle();
  const press = button("Publish again");
  press.focus();
  press.click();
  await settle();
  expect(calls.saved.length).toBe(1);
  expect(calls.again).toBe(1);
  expect(toasts.items.at(-1)?.text).toBe("Queued: publish the protocol again.");
  expect(button("Queued"), "the same button, so focus stays on it").toBe(press);
  expect(document.activeElement).toBe(press);
  press.click();
  await settle();
  expect(calls.again).toBe(1);
  // It lands: the press has nothing left to do, so focus is the keeper's, and a second Enter doesn't open the page (F-340).
  standing.again = null;
  hears.get(7)?.(undefined);
  await settle();
  expect(button("Queued")).toBeUndefined();
  expect(pubLine()).toContain("Published.");
  expect(document.activeElement?.textContent?.trim()).not.toBe("Open the published page");
});

test("a page edited in the source since says so, and offers no Publish again (F-327)", async () => {
  open({ publication: PUBLISHED, changed: true, moved: true });
  await settle();
  expect(target.querySelector(".tf-su-pub.tf-stale")?.textContent).toContain("Edited in Confluence since it was published");
  expect(button("Publish again")).toBeUndefined();
  expect([...target.querySelectorAll("a")].some((found) => found.textContent?.trim() === "Open the published page")).toBe(true);
});

test("a refused Publish again says why and may be pressed again (F-327)", async () => {
  open({ publication: PUBLISHED, changed: true, again: { write_id: 7, state: "refused", detail: "version is out of date" } });
  await settle();
  expect(target.querySelector(".tf-su-pub.tf-stale")?.textContent).toContain("Confluence refused it: version is out of date");
  expect(button("Publish again").getAttribute("aria-disabled")).toBeNull();
  expect(button("Show queue")).toBeDefined();
});

test("a held Publish again says so, and holds the press (F-327)", async () => {
  open({ publication: PUBLISHED, again: { write_id: 7, state: "held", detail: "the page moved" } });
  await settle();
  expect(target.querySelector(".tf-su-pub.tf-stale")?.textContent).toContain("Held in the queue: the page moved");
  expect(button("Held in the queue").getAttribute("aria-disabled")).toBe("true");
});

test("a press that fails says why and reads the page's standing again (F-327)", async () => {
  const { calls, standing } = open({ publication: PUBLISHED, changed: true, againFails: "The page was edited in the source" });
  await settle();
  standing.page_moved = true;
  button("Publish again").click();
  await settle();
  expect(calls.again).toBe(1);
  expect(target.querySelector(".tf-tm-fail")?.textContent).toContain("The page was edited in the source");
  expect(target.querySelector(".tf-su-pub.tf-stale")?.textContent).toContain("Edited in Confluence");
});

test("someone pressed off and on again keeps their place among the notes (F-348)", async () => {
  open();
  await settle();
  const notes = () => [...target.querySelectorAll(".tf-su-pn > span")].map((name) => name.textContent);
  const before = notes();
  expect(before.length).toBeGreaterThan(1);
  const first = [...target.querySelectorAll<HTMLButtonElement>(".tf-su-people [aria-pressed]")][0]!;
  first.click();
  await settle();
  expect(notes()).toEqual(before.slice(1));
  first.click();
  await settle();
  expect(notes()).toEqual(before);
});

test("Publish's toast is the queued write's, which says Sent in place once it has gone", async () => {
  open();
  await settle();
  button("Publish to Confluence").click();
  await settle();
  expect(toasts.items.at(-1)?.text).toMatch(/^Queued: publish the protocol under Confluence/);
});

test("Enter adds and leaves the field open for the next, and Enter on the empty field gives focus back to the button", async () => {
  open();
  await settle();
  button("Add someone").click();
  await settle();
  const field = document.activeElement as HTMLInputElement;
  field.value = "Ana";
  field.dispatchEvent(new Event("input", { bubbles: true }));
  field.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  await settle();
  expect(document.activeElement).toBe(field);
  expect(field.value).toBe("");
  expect(target.querySelector('.tf-su-people [aria-pressed="true"]:last-of-type')?.textContent?.trim()).toBe("Ana");
  field.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  await settle();
  expect(document.activeElement?.textContent?.trim()).toBe("Add someone");
});

test("an item whose ticket the protocol already links shows its key and status, and no Create ticket (#1433)", async () => {
  open({ links: [ticketLink("jira:OPS-131", "Update the on-call runbook (Tomasz Wójcik)")] });
  await settle();
  const rows = [...target.querySelectorAll(".tf-su-ai")];
  expect(rows).toHaveLength(2);
  // The item with no link keeps its button.
  expect(rows[0]!.querySelector("button")?.textContent?.trim()).toBe("Create ticket…");
  // The linked one reads the ticket's key and status, as a create that landed in this session does.
  expect(rows[1]!.querySelector("button")).toBeNull();
  expect(rows[1]!.querySelector("a")?.getAttribute("href")).toBe("#/ticket/jira%3AOPS-131");
  expect(rows[1]!.querySelector("a")?.textContent).toBe("OPS-131");
  expect(rows[1]!.querySelector(".tf-chip")?.textContent).toBe("To do");
});

test("only a ticket titled exactly as the item answers for it: another kind, another title or a withdrawn ticket does not (#1433)", async () => {
  open({
    links: [
      ticketLink("wiki:77", "Update the on-call runbook (Tomasz Wójcik)", { kind: "page" }),
      ticketLink("jira:OPS-9", "Update the on-call runbook"),
      ticketLink("jira:OPS-10", "Decide the manual-review SLA", { deleted_at: "2026-08-20T10:00:00Z" }),
    ],
  });
  await settle();
  const buttons = [...target.querySelectorAll(".tf-su-ai button")].map((found) => found.textContent?.trim());
  expect(buttons).toEqual(["Create ticket…", "Create ticket…"]);
  expect(target.querySelector(".tf-su-ai a")).toBeNull();
});

test("after a create, leaving the tab and coming back shows the ticket's key, not Create ticket (#1433)", async () => {
  const links: LinkEntry[] = [];
  const { hears } = open({ links });
  await settle();
  button("Create ticket…").click();
  await settle();
  const select = document.querySelector<HTMLSelectElement>("select[aria-label='Project the ticket goes in']")!;
  select.value = "jira:OPS";
  select.dispatchEvent(new Event("change", { bubbles: true }));
  flushSync();
  [...document.querySelectorAll<HTMLButtonElement>(".tf-dialog button")].find((found) => found.textContent?.trim() === "Create ticket")!.click();
  await settle();
  hears.get(9)!(undefined);
  await settle();
  expect(target.querySelector(".tf-su-ai a")?.textContent).toBe("OPS-131");

  unmount(view!);
  view = null;
  open({ links });
  await settle();
  const first = target.querySelector(".tf-su-ai")!;
  expect(first.querySelector("button")).toBeNull();
  expect(first.querySelector("a")?.textContent).toBe("OPS-131");
  expect(first.querySelector(".tf-chip")?.textContent).toBe("To do");
});

/** A create the protocol's read names as owing its link: filed before the reload, its ticket not mirrored yet. */
function owed(over: Partial<ActionItemTicket> = {}): ActionItemTicket {
  return { write_id: 9, title: "Decide the manual-review SLA", state: "sent", may_have_landed: false, detail: null, ticket_entity_id: null, linked: false, ...over };
}

/** A sync of `source` ended, as `sync:state` says it. */
function synced(emit: (event: string, payload: unknown) => void, source = "jira") {
  emit("sync:state", { source_id: source, running: false });
}

test("a reload before the sync shows a filed item sent, not Create ticket (#1951, #2104)", async () => {
  open({ owed: [owed()], mirrored: false });
  await settle();
  const rows = [...target.querySelectorAll(".tf-su-ai")];
  expect(rows[0]!.querySelector("button")).toBeNull();
  expect(rows[0]!.querySelector(".tf-chip")?.textContent).toBe("Sent");
  // The other item filed nothing and keeps its button.
  expect(rows[1]!.querySelector("button")?.textContent?.trim()).toBe("Create ticket…");
});

test("after a reload, the sync that brings the ticket in replaces sent with its key (#1951, #2104)", async () => {
  const { mirror, emit, calls } = open({ owed: [owed()], mirrored: false });
  await settle();
  synced(emit);
  await settle();
  expect(target.querySelector(".tf-su-ai .tf-chip")?.textContent).toBe("Sent");
  mirror.held = true;
  synced(emit);
  await settle();
  const first = target.querySelector(".tf-su-ai")!;
  expect(first.querySelector("button")).toBeNull();
  expect(first.querySelector("a")?.textContent).toBe("OPS-131");
  expect(first.querySelector(".tf-chip")?.textContent).toBe("To do");
  // Named, so the next sync asks nothing more.
  const asked = calls.asked.length;
  synced(emit);
  await settle();
  expect(calls.asked).toHaveLength(asked);
});

test("a create whose ticket the post-write read missed reads sent and is named by a later sync (#1951, #2104)", async () => {
  const { hears, mirror, emit } = open({ mirrored: false });
  await settle();
  button("Create ticket…").click();
  await settle();
  const select = document.querySelector<HTMLSelectElement>("select[aria-label='Project the ticket goes in']")!;
  select.value = "jira:OPS";
  select.dispatchEvent(new Event("change", { bubbles: true }));
  flushSync();
  [...document.querySelectorAll<HTMLButtonElement>(".tf-dialog button")].find((found) => found.textContent?.trim() === "Create ticket")!.click();
  await settle();
  // The row left the queue before any sync mirrored the ticket.
  hears.get(9)!(undefined);
  await settle();
  expect(target.querySelector(".tf-su-ai .tf-chip")?.textContent).toBe("Sent");
  mirror.held = true;
  synced(emit);
  await settle();
  expect(target.querySelector(".tf-su-ai a")?.textContent).toBe("OPS-131");
});

test("after a reload, a create held on drift or refused in the queue offers Create ticket, as the live tab does once told (#1951)", async () => {
  open({ owed: [owed({ state: "held" }), owed({ write_id: 10, state: "refused", title: "Update the on-call runbook (Tomasz Wójcik)" })], mirrored: false });
  await settle();
  const buttons = [...target.querySelectorAll(".tf-su-ai button")].map((found) => found.textContent?.trim());
  expect(buttons).toEqual(["Create ticket…", "Create ticket…"]);
});

test("after a reload, a create held as unconfirmed is on its way and followed, and one held on drift is offered again (#1966)", async () => {
  const { hears } = open({
    owed: [owed({ state: "held", may_have_landed: true }), owed({ write_id: 10, state: "held", title: "Update the on-call runbook (Tomasz Wójcik)" })],
    mirrored: false,
  });
  await settle();
  const rows = [...target.querySelectorAll(".tf-su-ai")];
  expect(rows[0]!.querySelector("button")).toBeNull();
  expect(rows[0]!.querySelector(".tf-chip")?.textContent).toBe("Queued");
  // Held on drift: never sent, so the item offers the create again.
  expect(rows[1]!.querySelector("button")?.textContent?.trim()).toBe("Create ticket…");
  // Followed, so the reader's discard in the queue is heard.
  expect(hears.has(9)).toBe(true);
});

test("a filed ticket the queue holds as unconfirmed stays queued, told once, with no Edit and resend (#1966)", async () => {
  const { hears } = open({ mirrored: false });
  await settle();
  button("Create ticket…").click();
  await settle();
  const select = document.querySelector<HTMLSelectElement>("select[aria-label='Project the ticket goes in']")!;
  select.value = "jira:OPS";
  select.dispatchEvent(new Event("change", { bubbles: true }));
  flushSync();
  [...document.querySelectorAll<HTMLButtonElement>(".tf-dialog button")].find((found) => found.textContent?.trim() === "Create ticket")!.click();
  await settle();
  toasts.items = [];
  const unconfirmed = {
    id: 9,
    state: "held",
    detail: "Jira did not answer",
    held_snapshot: { op: "create_ticket", unconfirmed: { source: "Jira", detail: "the connection closed" } },
  } as unknown as QueuedWrite;
  hears.get(9)!(unconfirmed);
  hears.get(9)!(unconfirmed);
  await settle();
  expect(target.querySelector(".tf-su-ai .tf-chip")?.textContent).toBe("Queued");
  expect(toasts.items.map((t) => [t.text, (t.more ?? []).length])).toEqual([
    ["Ticket may have arrived: Jira did not answer. Check Jira, then send it again or discard it.", 0],
  ]);
  expect(hears.has(9)).toBe(true);
});

for (const [name, field] of [
  ["Add someone", "Who else was there"],
  ["Add an action item", "New action item"],
] as const) {
  test(`${name} puts the cursor in its field even while a notes box holds it (#1511)`, async () => {
    open();
    await settle();
    target.querySelector<HTMLTextAreaElement>(".tf-su-pn textarea")!.focus();
    button(name).click();
    await settle();
    expect(document.activeElement?.getAttribute("aria-label")).toBe(field);
  });
}

test("a date in the address opens that day's protocol and digest, not today's (#1504)", async () => {
  const { calls } = open({ date: "2026-08-03" });
  await settle();
  expect(calls.days).toEqual(["2026-08-03"]);
  expect(calls.digests).toEqual(["2026-08-03"]);
  expect(target.querySelector(".tf-su-day")?.textContent).toBe("Monday 3 August");
});

test("two overlapping saves land in order, the newer body last, and no save reads the note back (#1679)", async () => {
  const { calls, release, stored } = open({ slowFirstWrite: true });
  await settle();
  const reads = calls.reads;
  const box = () => target.querySelectorAll<HTMLTextAreaElement>(".tf-su-pn textarea")[1]!;
  for (const typed of ["first", "second"]) {
    box().value = typed;
    box().dispatchEvent(new Event("input", { bubbles: true }));
    await pausedOut();
  }
  release();
  // Both pauses are out; what is left is the chain, which settles without a timer.
  await settle();
  expect(stored()).toContain("Tomasz Wójcik: second");
  expect(calls.reads).toBe(reads);
});

test("a refused save does not break the chain: the next save still lands (#1679)", async () => {
  const { stored } = open({ failFirstWrite: true });
  await settle();
  const box = () => target.querySelectorAll<HTMLTextAreaElement>(".tf-su-pn textarea")[1]!;
  for (const typed of ["first", "second"]) {
    box().value = typed;
    box().dispatchEvent(new Event("input", { bubbles: true }));
    await pausedOut();
  }
  await settle();
  expect(stored()).toContain("Tomasz Wójcik: second");
});

// -- the echo's one lifecycle: queued, sent, gone on the refresh's read (#2104) --

test("an action item's ticket echo says queued, then sent, and goes once the refresh event's read names the ticket (#2104)", async () => {
  const { hears, mirror, emit } = open({ mirrored: false });
  await settle();
  button("Create ticket…").click();
  await settle();
  const select = document.querySelector<HTMLSelectElement>("select[aria-label='Project the ticket goes in']")!;
  select.value = "jira:OPS";
  select.dispatchEvent(new Event("change", { bubbles: true }));
  flushSync();
  [...document.querySelectorAll<HTMLButtonElement>(".tf-dialog button")].find((found) => found.textContent?.trim() === "Create ticket")!.click();
  await settle();
  const first = () => target.querySelector(".tf-su-ai")!;
  expect(first().querySelector(".tf-chip")?.textContent).toBe("Queued");
  hears.get(9)!(undefined);
  await settle();
  expect(first().querySelector(".tf-chip")?.textContent).toBe("Sent");
  // The refresh after the create brought the ticket in: the write names it.
  mirror.held = true;
  emit("item:refreshed", { entity_id: "jira:OPS-131", write_id: 9, ended: "refreshed" });
  await settle();
  expect(first().querySelector("a")?.textContent).toBe("OPS-131");
  expect(first().querySelector(".tf-chip")?.textContent).toBe("To do");
});
