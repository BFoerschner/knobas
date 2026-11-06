/**
 * *Link to…*'s picker (F-420): a ticket leads with its key, in the list and once picked, so two near-identical titles
 * differ and a match on the key shows why it matched. Other kinds have no key to lead with.
 */
import { flushSync, mount, tick, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { createLayers } from "../layers.svelte";
import TfLinkTo from "./TfLinkTo.svelte";

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;

beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
});

afterEach(() => {
  if (view) void unmount(view);
  view = null;
  target.remove();
});

const hit = (entity_id: string, kind: string, title: string) => ({ entity_id, kind, title, source_id: "x", rank: 1, snippet: [], excerpts: [], updated_at: null, synced_at: "2026-11-06T09:00:00Z", path: null, state: null });

async function settle(): Promise<void> {
  for (let i = 0; i < 5; i++) {
    await tick();
    await new Promise((resolve) => setTimeout(resolve, 60));
  }
  flushSync();
}

test("a ticket leads with its key in the list and once picked; a page has none", async () => {
  const search = vi.fn(async () => ({
    interpreted: { text: "pay", prefix: null, filters: { sources: [], kinds: [], updated_within_days: null, mine: false, authors: [] }, unknown_tokens: [], left_out_tokens: [] },
    total: 2,
    took_ms: 1,
    groups: [
      { kind: "ticket", label: "Ticket", plural: "Tickets", monogram: "T", total: 1, hits: [hit("jira:PAY-219", "ticket", "Rotate PSP credentials")] },
      { kind: "page", label: "Page", plural: "Pages", monogram: "P", total: 1, hits: [hit("confluence:9", "page", "Payout design")] },
    ],
    coverage: [],
  }));
  const ports = { search, launcherHome: vi.fn(async () => ({ favourites: [], recent: [] })), createLink: vi.fn() };
  view = mount(TfLinkTo, { target, props: { jobKey: "SEPA", entityId: "confluence:1", ports: ports as never, layers: createLayers(), onclose: () => {}, onlinked: () => {} } });
  flushSync();
  const field = target.ownerDocument.querySelector<HTMLInputElement>(".tf-formgrid input[type='text']")!;
  field.value = "pay";
  field.dispatchEvent(new Event("input", { bubbles: true }));
  await settle();
  const rows = () => [...target.ownerDocument.querySelectorAll<HTMLButtonElement>(".tf-picklist button")];
  expect(rows().map((row) => row.querySelector(".tf-key")?.textContent ?? null)).toEqual(["PAY-219", null]);
  expect(rows()[0]!.textContent).toBe("PAY-219Rotate PSP credentialsTicket");

  rows()[0]!.click();
  await settle();
  expect(target.ownerDocument.querySelector(".tf-chosen .tf-key")?.textContent).toBe("PAY-219");
});

test("a key two sources hold is two tickets: both rows name their source, one marked and one offered", async () => {
  const twin = (entity_id: string, source_id: string) => ({ ...hit(entity_id, "ticket", "Retry failed SEPA payouts"), source_id });
  const search = vi.fn(async () => ({
    interpreted: { text: "PAY-231", prefix: null, filters: { sources: [], kinds: [], updated_within_days: null, mine: false, authors: [] }, unknown_tokens: [], left_out_tokens: [] },
    total: 2,
    took_ms: 1,
    groups: [{ kind: "ticket", label: "Ticket", plural: "Tickets", monogram: "T", total: 2, hits: [twin("mock:PAY-231", "mock"), twin("jira:PAY-231", "jira")] }],
    coverage: [],
  }));
  const ports = { search, launcherHome: vi.fn(async () => ({ favourites: [], recent: [] })), createLink: vi.fn() };
  view = mount(TfLinkTo, { target, props: { jobKey: "SEPA", entityId: "confluence:1", linked: ["mock:PAY-231"], ports: ports as never, layers: createLayers(), onclose: () => {}, onlinked: () => {} } });
  flushSync();
  const field = target.ownerDocument.querySelector<HTMLInputElement>(".tf-formgrid input[type='text']")!;
  field.value = "PAY-231";
  field.dispatchEvent(new Event("input", { bubbles: true }));
  await settle();
  const rows = [...target.ownerDocument.querySelectorAll<HTMLButtonElement>(".tf-picklist button")];
  expect(rows.map((row) => [row.querySelector(".tf-dim")?.textContent, row.disabled])).toEqual([["Ticket, jira", false], ["Already linked, mock", true]]);
});
