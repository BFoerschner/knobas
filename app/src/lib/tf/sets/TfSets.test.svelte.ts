/**
 * Link sets (#866, R-7.6, R-7.7), mounted over stand-in ports: the list's
 * rows, filter and switch, *New link set*, and the editor's panels, picker,
 * rules with their live counts, *Shows on* and *Delete*, each change saved
 * as the whole set.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { LinkSetDetail, LinkSetDraft, LinkSetRow, LinkSetRule, RuleTicket } from "../../ipc/entity";
import { toasts } from "../../shell/toasts.svelte";
import { forgetFilters } from "../filter/filter-state.svelte";
import type { Candidate, SetPorts } from "./ports";
import TfSets from "./TfSets.svelte";

const STACK: LinkSetDetail = {
  id: "payout-stack",
  name: "Payout stack",
  description: "What a change on the payout path touches.",
  members: [
    { kind: "repo", entity_id: "gitea:tidewater/payout-service", title: "tidewater/payout-service" },
    { kind: "repo", entity_id: "gitea:tidewater/psp-client", title: "tidewater/psp-client" },
    { kind: "page", entity_id: "confluence:1", title: "PSP error codes" },
  ],
  rules: [
    { kind: "epic", value: "jira:PAY-200", title: "Payout reliability" },
    { kind: "manual", value: "jira:PAY-228", title: "PAY-228" },
  ],
  tickets: 3,
  mine: 2,
  shows_on: [
    { entity_id: "jira:PAY-231", title: "Retry failed SEPA payouts", mine: true },
    { entity_id: "jira:PAY-240", title: "Payout dashboard latency", mine: true },
    { entity_id: "jira:PAY-228", title: "Idempotency key", mine: false },
  ],
};

const ONCALL: LinkSetRow = {
  id: "oncall",
  name: "On-call kit",
  description: "",
  members: [{ kind: "page", entity_id: "confluence:9", title: "On-call handbook" }],
  rules: [{ kind: "project", value: "jira:OPS", title: "Operations" }],
  tickets: 4,
  mine: 0,
};

const PAGES: Candidate[] = [
  { entity_id: "confluence:1", name: "PSP error codes", sub: "ENG › Payments" },
  { entity_id: "confluence:2", name: "Payout batch lifecycle", sub: "ENG › Payments" },
];

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;
let saved: LinkSetDraft[];
let deleted: string[];
let previews: LinkSetRule[];
let inherit: boolean[];
let stored: LinkSetDetail;

function ports(): SetPorts {
  return {
    linkSets: async () => [STACK, ONCALL],
    linkSet: async (id) => {
      if (id !== stored.id) throw { code: "not_found", message: `no link set is called ${id}`, source_id: null };
      return stored;
    },
    saveLinkSet: async (draft) => {
      saved.push(structuredClone(draft));
      stored = {
        ...stored,
        id: draft.id ?? "new-link-set",
        name: draft.name,
        description: draft.description,
        members: draft.members.map((member) => ({ ...member, title:
            stored.members.find((held) => held.entity_id === member.entity_id)?.title ??
            PAGES.find((page) => page.entity_id === member.entity_id)?.name ??
            null,
        })),
        rules: draft.rules.map((rule) => ({ ...rule, title: null })),
      };
      return stored;
    },
    deleteLinkSet: async (id) => void deleted.push(id),
    previewLinkSetRule: async (rule): Promise<RuleTicket[]> => {
      previews.push(rule);
      return rule.kind === "epic" ? STACK.shows_on.slice(0, 2) : STACK.shows_on.slice(2);
    },
    parentLinks: async () => true,
    setParentLinks: async (enabled) => {
      inherit.push(enabled);
      return enabled;
    },
    candidates: async (kind) => (kind === "page" ? PAGES : []),
    ruleChoices: async () => ({
      tickets: [
        { entity_id: "jira:PAY-200", title: "Payout reliability" },
        { entity_id: "jira:PAY-228", title: "Idempotency key" },
        { entity_id: "jira:PAY-300", title: "PSP migration" },
      ],
      parents: ["PAY-200", "PAY-300"],
      projects: [{ source_id: "jira", key: "OPS", name: "Operations" }],
    }),
  };
}

async function settle() {
  for (let i = 0; i < 6; i += 1) await Promise.resolve();
  flushSync();
}

async function draw(rest: string[]) {
  view = mount(TfSets, { target, props: { ready: true, rest, ports: ports() } });
  await settle();
}

const text = (selector: string) => [...target.querySelectorAll(selector)].map((node) => node.textContent?.replace(/\s+/g, " ").trim());
const button = (name: string) =>
  [...target.ownerDocument.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent?.trim() === name)!;

beforeEach(() => {
  forgetFilters();
  toasts.items.splice(0);
  saved = [];
  deleted = [];
  previews = [];
  inherit = [];
  stored = structuredClone(STACK);
  target = document.createElement("div");
  document.body.appendChild(target);
  location.hash = "#/sets";
});

afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
  document.querySelectorAll(".tf-modal").forEach((node) => node.remove());
});

test("the list draws each set's contents per kind, a line per rule and its tickets now", async () => {
  await draw([]);
  expect(target.querySelector("h1")?.textContent).toBe("Link sets");
  expect(target.querySelectorAll("tbody tr")).toHaveLength(2);
  expect(text("tbody tr:first-child td")).toEqual([
    "Payout stackWhat a change on the payout path touches.",
    "2 repositories1 pages",
    "Every ticket under epic PAY-200 Payout reliabilityPAY-228, applied by hand",
    "3 (2 yours)",
  ]);
  expect(text("tbody tr:nth-child(2) td")[2]).toBe("Every ticket in project OPS");
});

test("the filter keeps sets by name or member, and by what they apply by", async () => {
  await draw([]);
  const field = target.querySelector<HTMLInputElement>("input[placeholder='Filter link sets by name or member']")!;
  field.value = "handbook";
  field.dispatchEvent(new Event("input", { bubbles: true }));
  await settle();
  expect(text("tbody .tf-set-name")).toEqual(["On-call kit"]);
  field.value = "";
  field.dispatchEvent(new Event("input", { bubbles: true }));
  button("By hand").click();
  await settle();
  expect(text("tbody .tf-set-name")).toEqual(["Payout stack"]);
});

test("the sub-task switch is R-7.3's, and New link set saves a set and opens it", async () => {
  await draw([]);
  const toggle = target.querySelector<HTMLInputElement>(".tf-toggle input")!;
  expect(toggle.checked).toBe(true);
  toggle.click();
  await settle();
  expect(inherit).toEqual([false]);
  button("New link set").click();
  await settle();
  expect(saved).toEqual([{ id: null, name: "New link set", description: "", members: [], rules: [] }]);
  expect(location.hash).toBe("#/sets/new-link-set");
});

test("the editor draws a panel per kind, the rules with live counts and the tickets it shows on", async () => {
  await draw(["payout-stack"]);
  expect(text(".tf-panel > header h3")).toEqual([
    "Repositories",
    "Pipelines",
    "Pages",
    "Assets",
    "Monitors",
    "Applies to",
    "Shows on 3 tickets now",
  ]);
  expect(text(".tf-set-panels .tf-panel:first-child .tf-member > span:first-child")).toEqual([
    "tidewater/payout-service",
    "tidewater/psp-client",
  ]);
  expect(text(".tf-set-panels .tf-panel:nth-child(2) .tf-member")).toEqual(["Nothing yet."]);
  expect(previews).toEqual([
    { kind: "epic", value: "jira:PAY-200" },
    { kind: "manual", value: "jira:PAY-228" },
  ]);
  expect(text(".tf-rule-n")).toEqual(["2 tickets", "1 tickets"]);
  expect(text(".tf-roll")).toEqual(["2 of them yours. New tickets join as soon as they match."]);
  expect(text(".tf-keys a.tf-mine")).toEqual(["PAY-231", "PAY-240"]);
  expect(target.querySelector<HTMLAnchorElement>(".tf-keys a")?.getAttribute("href")).toBe("#/job/PAY-231");
});

test("Add… marks what is already in, and adds what was picked as one save", async () => {
  await draw(["payout-stack"]);
  target.querySelectorAll<HTMLButtonElement>(".tf-set-add button")[2]!.click();
  await settle();
  const dialog = document.querySelector(".tf-dialog")!;
  expect(dialog.querySelector("h2")?.textContent).toBe("Add pages to Payout stack");
  const boxes = [...dialog.querySelectorAll<HTMLInputElement>(".tf-picks input")];
  expect(boxes.map((box) => [box.checked, box.disabled])).toEqual([
    [true, true],
    [false, false],
  ]);
  expect(dialog.querySelector(".tf-picks label")?.textContent).toContain("already in");
  boxes[1]!.click();
  await settle();
  button("Add").click();
  await settle();
  expect(saved.at(-1)?.members.map((member) => member.entity_id)).toEqual([
    "gitea:tidewater/payout-service",
    "gitea:tidewater/psp-client",
    "confluence:1",
    "confluence:2",
  ]);
  expect(saved.at(-1)?.members.at(-1)?.kind).toBe("page");
  expect(toasts.items.at(-1)?.text).toBe("Added 1 to Payout stack");
});

test("a rule is added once its value is chosen, and Remove drops a member as one save", async () => {
  await draw(["payout-stack"]);
  button("Add a rule").click();
  await settle();
  const selects = [...[...target.querySelectorAll(".tf-rule")].at(-1)!.querySelectorAll<HTMLSelectElement>("select")];
  expect([...selects[1]!.options].map((option) => option.textContent)).toEqual([
    "Choose…",
    "PAY-200 Payout reliability",
    "PAY-300 PSP migration",
  ]);
  selects[1]!.value = "jira:PAY-300";
  selects[1]!.dispatchEvent(new Event("change", { bubbles: true }));
  await settle();
  expect(saved.at(-1)?.rules).toEqual([
    { kind: "epic", value: "jira:PAY-200" },
    { kind: "manual", value: "jira:PAY-228" },
    { kind: "epic", value: "jira:PAY-300" },
  ]);
  target.querySelector<HTMLButtonElement>("button[aria-label='Remove tidewater/psp-client']")!.click();
  await settle();
  expect(saved.at(-1)?.members.map((member) => member.entity_id)).not.toContain("gitea:tidewater/psp-client");
});

test("a change made while a save is in flight is saved after it, not dropped", async () => {
  await draw(["payout-stack"]);
  target.querySelector<HTMLButtonElement>("button[aria-label='Remove tidewater/psp-client']")!.click();
  target.querySelector<HTMLButtonElement>("button[aria-label='Remove PSP error codes']")!.click();
  await settle();
  await settle();
  expect(saved).toHaveLength(2);
  expect(saved.at(-1)?.members.map((member) => member.entity_id)).toEqual(["gitea:tidewater/payout-service"]);
});

test("Delete removes the set and goes back to the list", async () => {
  await draw(["payout-stack"]);
  button("Delete").click();
  await settle();
  expect(deleted).toEqual(["payout-stack"]);
  expect(location.hash).toBe("#/sets");
  expect(toasts.items.at(-1)?.text).toBe("Deleted the link set. Links drawn on tickets by hand stay.");
});

test("an id nobody holds says so", async () => {
  await draw(["gone"]);
  expect(target.querySelector(".tf-empty")?.textContent).toContain("No link set is called gone.");
});
