/**
 * Time > Standup (#829): what a reader sees, and what each action sends. The
 * bridge is injected; what the backend does with the calls is
 * `crates/knobas-app/tests/it/standup_ipc.rs`'s and `protocol.rs`'s.
 */
import { flushSync, mount, tick, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { DigestLine, Protocol, PublishTarget, StandupDigest } from "../../ipc/entity";
import type { QueuedWrite } from "../../ipc/sources";
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
  "",
].join("\n");

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

function open(over: { stored?: PublishTarget | null; publication?: Protocol["publication"] } = {}) {
  const calls: { saved: string[]; published: (PublishTarget | undefined)[]; copied: string[]; tickets: unknown[][] } = {
    saved: [],
    published: [],
    copied: [],
    tickets: [],
  };
  const hears = new Map<number, (row: QueuedWrite | undefined) => void>();
  let body = BODY;
  const protocol = (publication: Protocol["publication"] = over.publication ?? null): Protocol => ({
    day: DAY,
    note_id: "note:p",
    page_title: DAY,
    publication,
  });
  view = mount(TfStandup, {
    target,
    props: {
      now: () => NOW,
      saveAfterMs: 0,
      layers: createLayers(),
      ports: {
        standupDigest: async () => DIGEST,
        standupProtocol: async () => protocol(),
        getNote: async () => ({ note: { id: "note:p", title: `Standup ${DAY}`, body_md: body, created_at: "", updated_at: "" }, refs: [], links: [] }),
        saveNote: async (_id: string, _title: string, text: string) => {
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
          return { write_id: 9, state: "pending", detail: null, ticket_entity_id: null, linked: false };
        },
        actionItemTicket: async () => ({ write_id: 9, state: "sent", detail: null, ticket_entity_id: "jira:OPS-131", linked: true }),
        follower: { follow: (id: number, hear: (row: QueuedWrite | undefined) => void) => (hears.set(id, hear), () => hears.delete(id)) },
        copy: async (text: string) => {
          calls.copied.push(text);
        },
      } as never,
    },
  });
  return { calls, hears };
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
