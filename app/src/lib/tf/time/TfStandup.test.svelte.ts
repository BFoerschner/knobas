/**
 * Time > Standup (#829): what a reader sees, and what each action sends. The
 * bridge is injected; what the backend does with the calls is
 * `crates/knobas-app/tests/it/standup_ipc.rs`'s and `protocol.rs`'s.
 */
import { flushSync, mount, tick, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { ActionItemTicket, DigestLine, LinkEnd, LinkEntry, Protocol, PublishTarget, StandupDigest } from "../../ipc/entity";
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
  target = document.createElement("div");
  document.body.append(target);
});
afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
});

async function settle() {
  for (let round = 0; round < 6; round++) {
    await tick();
    await Promise.resolve();
  }
  flushSync();
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
  } = {
    reads: 0,
    writes: 0,
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
  const protocol = (publication: Protocol["publication"] = over.publication ?? null): Protocol => ({
    day: DAY,
    note_id: "note:p",
    page_title: DAY,
    publication,
    owed_tickets: mirror.held ? [] : (over.owed ?? []),
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
      } as never,
    },
  });
  return { calls, hears, mirror, emit, release: () => release(), stored: () => body };
}

const text = () => target.textContent?.replace(/\s+/g, " ") ?? "";
const button = (name: string) =>
  [...target.querySelectorAll("button")].find((found) => found.textContent?.trim() === name) as HTMLButtonElement;

test("the digest draws each list, every line with an item a link into the new shell", async () => {
  open();
  await settle();
  expect(text()).toContain("Yesterday, Thursday 20");
  const links = [...target.querySelectorAll(".tf-su-dig a")].map((link) => link.getAttribute("href"));
  expect(links).toEqual(["#/job/PAY-231", "#/page/confluence%3A98401", "#/monitors"]);
  expect(text()).toContain("DB config (timer running)");
  expect(text()).toContain("Nothing of yours is blocked.");
});

test("Infrastructure sits under Blockers, every alert a link to its monitor", async () => {
  open();
  await settle();
  const headings = [...target.querySelectorAll(".tf-su-dig h4")].map((h) => h.textContent);
  expect(headings.slice(-2)).toEqual(["Blockers", "Infrastructure"]);
  const alert = [...target.querySelectorAll(".tf-su-dig a")].at(-1)!;
  expect(alert.getAttribute("href")).toBe("#/monitors");
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

test("the protocol draws who was there, their notes, and the action items, from the note's body", async () => {
  open();
  await settle();
  const pressed = [...target.querySelectorAll<HTMLButtonElement>(".tf-su-val")].map((chip) => [
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
  await settle();
  await new Promise((done) => setTimeout(done, 5));
  await settle();
  expect(calls.saved.at(-1)).toContain("## Attendees\n\n- Mara Lindqvist\n\n## Notes");
  expect(button("Tomasz Wójcik").getAttribute("aria-pressed")).toBe("false");
});

test("a note typed under someone's name is saved as their bullet", async () => {
  const { calls } = open();
  await settle();
  const box = target.querySelectorAll<HTMLTextAreaElement>(".tf-su-pn textarea")[1]!;
  box.value = "Runbook today";
  box.dispatchEvent(new Event("input", { bubbles: true }));
  await new Promise((done) => setTimeout(done, 5));
  await settle();
  expect(calls.saved.at(-1)).toContain("- Mara Lindqvist: Review open.\n- Tomasz Wójcik: Runbook today\n");
});

test("Create ticket files the action item, shows it queued, then its key and status once it lands", async () => {
  const { calls, hears } = open();
  await settle();
  button("Create ticket").click();
  await settle();
  const select = document.querySelector<HTMLSelectElement>("select[aria-label='Project the ticket goes in']")!;
  select.value = "jira:OPS";
  select.dispatchEvent(new Event("change", { bubbles: true }));
  flushSync();
  [...document.querySelectorAll<HTMLButtonElement>(".tf-dialog button")].find((found) => found.textContent?.trim() === "Create ticket")!.click();
  await settle();
  expect(calls.tickets[0]).toEqual(["note:p", "jira:OPS", "Task", "Decide the manual-review SLA", `From the standup protocol of ${DAY}.`]);
  expect(target.querySelector(".tf-su-ai .tf-chip")?.textContent).toBe("queued");
  hears.get(9)!(undefined);
  await settle();
  expect(target.querySelector(".tf-su-ai a")?.getAttribute("href")).toBe("#/job/OPS-131");
  expect(target.querySelector(".tf-su-ai .tf-chip")?.textContent).toBe("To Do");
});

test("Publish goes to the stored page without asking, after saving", async () => {
  const { calls } = open();
  await settle();
  button("Publish to Confluence").click();
  await settle();
  expect(calls.saved.length).toBe(1);
  expect(calls.published).toEqual([undefined]);
  expect(text()).toContain("publishing");
});

test("the first Publish asks where protocols go", async () => {
  const { calls } = open({ stored: null });
  await settle();
  expect(text()).toContain("The first publish asks where.");
  button("Publish to Confluence").click();
  await settle();
  expect(document.querySelector(".tf-dialog h2")?.textContent).toBe("Where do standup protocols go?");
  expect(calls.published).toEqual([]);
});

test("a published protocol links its page", async () => {
  open({ publication: { write_id: 5, state: "sent", detail: null, page_entity_id: "wiki:901", linked: true } });
  await settle();
  const link = [...target.querySelectorAll("a")].find((found) => found.textContent?.trim() === "Published");
  expect(link?.getAttribute("href")).toBe("#/page/wiki%3A901");
});

test("an item whose ticket the protocol already links shows its key and status, and no Create ticket (#1433)", async () => {
  open({ links: [ticketLink("jira:OPS-131", "Update the on-call runbook (Tomasz Wójcik)")] });
  await settle();
  const rows = [...target.querySelectorAll(".tf-su-ai")];
  expect(rows).toHaveLength(2);
  // The item with no link keeps its button.
  expect(rows[0]!.querySelector("button")?.textContent?.trim()).toBe("Create ticket");
  // The linked one reads the ticket's key and status, as a create that landed in this session does.
  expect(rows[1]!.querySelector("button")).toBeNull();
  expect(rows[1]!.querySelector("a")?.getAttribute("href")).toBe("#/job/OPS-131");
  expect(rows[1]!.querySelector("a")?.textContent).toBe("OPS-131");
  expect(rows[1]!.querySelector(".tf-chip")?.textContent).toBe("To Do");
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
  expect(buttons).toEqual(["Create ticket", "Create ticket"]);
  expect(target.querySelector(".tf-su-ai a")).toBeNull();
});

test("after a create, leaving the tab and coming back shows the ticket's key, not Create ticket (#1433)", async () => {
  const links: LinkEntry[] = [];
  const { hears } = open({ links });
  await settle();
  button("Create ticket").click();
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
  expect(first.querySelector(".tf-chip")?.textContent).toBe("To Do");
});

/** A create the protocol's read names as owing its link: filed before the reload, its ticket not mirrored yet. */
function owed(over: Partial<ActionItemTicket> = {}): ActionItemTicket {
  return { write_id: 9, title: "Decide the manual-review SLA", state: "sent", may_have_landed: false, detail: null, ticket_entity_id: null, linked: false, ...over };
}

/** A sync of `source` ended, as `sync:state` says it. */
function synced(emit: (event: string, payload: unknown) => void, source = "jira") {
  emit("sync:state", { source_id: source, running: false });
}

test("a reload before the sync shows a filed item queued, not Create ticket (#1951)", async () => {
  open({ owed: [owed()], mirrored: false });
  await settle();
  const rows = [...target.querySelectorAll(".tf-su-ai")];
  expect(rows[0]!.querySelector("button")).toBeNull();
  expect(rows[0]!.querySelector(".tf-chip")?.textContent).toBe("queued");
  // The other item filed nothing and keeps its button.
  expect(rows[1]!.querySelector("button")?.textContent?.trim()).toBe("Create ticket");
});

test("after a reload, the sync that brings the ticket in replaces queued with its key (#1951)", async () => {
  const { mirror, emit, calls } = open({ owed: [owed()], mirrored: false });
  await settle();
  synced(emit);
  await settle();
  expect(target.querySelector(".tf-su-ai .tf-chip")?.textContent).toBe("queued");
  mirror.held = true;
  synced(emit);
  await settle();
  const first = target.querySelector(".tf-su-ai")!;
  expect(first.querySelector("button")).toBeNull();
  expect(first.querySelector("a")?.textContent).toBe("OPS-131");
  expect(first.querySelector(".tf-chip")?.textContent).toBe("To Do");
  // Named, so the next sync asks nothing more.
  const asked = calls.asked.length;
  synced(emit);
  await settle();
  expect(calls.asked).toHaveLength(asked);
});

test("a create whose ticket the post-write read missed is named by a later sync, not left queued (#1951)", async () => {
  const { hears, mirror, emit } = open({ mirrored: false });
  await settle();
  button("Create ticket").click();
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
  expect(target.querySelector(".tf-su-ai .tf-chip")?.textContent).toBe("queued");
  mirror.held = true;
  synced(emit);
  await settle();
  expect(target.querySelector(".tf-su-ai a")?.textContent).toBe("OPS-131");
});

test("after a reload, a create held on drift or refused in the queue offers Create ticket, as the live tab does once told (#1951)", async () => {
  open({ owed: [owed({ state: "held" }), owed({ write_id: 10, state: "refused", title: "Update the on-call runbook (Tomasz Wójcik)" })], mirrored: false });
  await settle();
  const buttons = [...target.querySelectorAll(".tf-su-ai button")].map((found) => found.textContent?.trim());
  expect(buttons).toEqual(["Create ticket", "Create ticket"]);
});

test("after a reload, a create held as unconfirmed is on its way and followed, and one held on drift is offered again (#1966)", async () => {
  const { hears } = open({
    owed: [owed({ state: "held", may_have_landed: true }), owed({ write_id: 10, state: "held", title: "Update the on-call runbook (Tomasz Wójcik)" })],
    mirrored: false,
  });
  await settle();
  const rows = [...target.querySelectorAll(".tf-su-ai")];
  expect(rows[0]!.querySelector("button")).toBeNull();
  expect(rows[0]!.querySelector(".tf-chip")?.textContent).toBe("queued");
  // Held on drift: never sent, so the item offers the create again.
  expect(rows[1]!.querySelector("button")?.textContent?.trim()).toBe("Create ticket");
  // Followed, so the reader's discard in the queue is heard.
  expect(hears.has(9)).toBe(true);
});

test("a filed ticket the queue holds as unconfirmed stays queued, told once, with no Edit and resend (#1966)", async () => {
  const { hears } = open({ mirrored: false });
  await settle();
  button("Create ticket").click();
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
  expect(target.querySelector(".tf-su-ai .tf-chip")?.textContent).toBe("queued");
  expect(toasts.items.map((t) => [t.text, (t.more ?? []).length])).toEqual([
    ["Ticket may have arrived: Jira did not answer. Check Jira, then send it again or discard it.", 0],
  ]);
  expect(hears.has(9)).toBe(true);
});

for (const [name, field] of [
  ["+ Add someone", "Who else was there"],
  ["+ Add an action item", "New action item"],
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
    await new Promise((done) => setTimeout(done, 5));
  }
  release();
  await settle();
  await new Promise((done) => setTimeout(done, 5));
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
    await new Promise((done) => setTimeout(done, 5));
  }
  await new Promise((done) => setTimeout(done, 20));
  await settle();
  expect(stored()).toContain("Tomasz Wójcik: second");
});
