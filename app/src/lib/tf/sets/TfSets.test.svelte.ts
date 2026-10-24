/**
 * Link sets (#866, R-7.6, R-7.7), mounted over stand-in ports: the list's
 * rows, filter and switch, *New link set*, and the editor's panels, picker,
 * rules with their live counts, *Shows on* and *Delete*, each change saved
 * as the whole set.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { LinkSetDetail, LinkSetDraft, LinkSetHint, LinkSetRow, LinkSetRule, RuleTicket } from "../../ipc/entity";
import { toasts } from "../../shell/toasts.svelte";
import type { Candidate, SetPorts } from "./ports";
import TfSets from "./TfSets.svelte";

const STACK: LinkSetDetail = {
  id: "payout-stack",
  name: "Payout stack",
  description: "What a change on the payout path touches.",
  members: [
    { kind: "repo", entity_id: "gitea:tidewater/payout-service", title: "tidewater/payout-service", line: null, state: null },
    { kind: "repo", entity_id: "gitea:tidewater/psp-client", title: "tidewater/psp-client", line: null, state: null },
    { kind: "page", entity_id: "confluence:1", title: "PSP error codes", line: "ENG › Payments", state: null },
    {
      kind: "asset",
      entity_id: "asset:pve-02",
      title: "pve-02",
      line: "vm, prod",
      state: { text: "down", tone: "err" },
    },
  ],
  patterns: [
    {
      kind: "pipeline",
      query: "project:Payouts name:Payout_*",
      count: 2,
      matches: [
        { kind: "pipeline", entity_id: "tc:Payout_Build", title: "Payout_Build", line: "Payments › Payouts", state: { text: "FAILURE", tone: "err" } },
        { kind: "pipeline", entity_id: "tc:Payout_Lint", title: "Payout_Lint", line: "Payments › Payouts", state: null },
      ],
    },
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
  members: [{ kind: "page", entity_id: "confluence:9", title: "On-call handbook", line: null, state: null }],
  patterns: [],
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
let patternPreviews: string[];
let inherit: boolean[];
let stored: LinkSetDetail;
let hints: LinkSetHint[];

function ports(): SetPorts {
  return {
    linkSets: async () => [STACK, ONCALL],
    linkSetHints: async () => hints,
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
        members: draft.members.map((member) => ({ ...member, line: null, state: null, title:
            stored.members.find((held) => held.entity_id === member.entity_id)?.title ??
            PAGES.find((page) => page.entity_id === member.entity_id)?.name ??
            null,
        })),
        patterns: draft.patterns.map((pattern) => ({ ...pattern, count: 1, matches: [] })),
        rules: draft.rules.map((rule) => ({ ...rule, title: null })),
      };
      return stored;
    },
    deleteLinkSet: async (id) => void deleted.push(id),
    previewLinkSetRule: async (rule): Promise<RuleTicket[]> => {
      previews.push(rule);
      return rule.kind === "epic" ? STACK.shows_on.slice(0, 2) : STACK.shows_on.slice(2);
    },
    previewLinkSetPattern: async (pattern) => {
      patternPreviews.push(pattern.query);
      const projects = [
        { source_id: "tc", key: "Payouts", name: "Payouts", path: ["Payments", "Payouts"] },
        { source_id: "tc", key: "Payments", name: "Payments", path: ["Payments"] },
      ];
      if (pattern.query.includes("bogus")) return { count: 0, matches: [], projects, refusal: "a pattern reads source:, project:, under: and name: and nothing else" };
      const matches = STACK.patterns[0]!.matches.filter((row) => !pattern.query.includes("Lint") || row.title === "Payout_Lint");
      return { count: matches.length, matches, projects, refusal: null };
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
  toasts.items.splice(0);
  saved = [];
  deleted = [];
  previews = [];
  patternPreviews = [];
  inherit = [];
  stored = structuredClone(STACK);
  hints = [];
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
    "2 repositories2 pipelines1 page1 asset",
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
  expect(saved).toEqual([{ id: null, name: "New link set", description: "", members: [], patterns: [], rules: [] }]);
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
  expect(text(".tf-set-panels .tf-panel:first-child .tf-member > span:nth-child(2)")).toEqual([
    "tidewater/payout-service",
    "tidewater/psp-client",
  ]);
  expect(text(".tf-set-panels .tf-panel:nth-child(5) .tf-member")).toEqual(["Nothing yet."]);
  expect(previews).toEqual([
    { kind: "epic", value: "jira:PAY-200" },
    { kind: "manual", value: "jira:PAY-228" },
  ]);
  expect(text(".tf-rule-n")).toEqual(["2 tickets", "1 ticket"]);
  expect(text(".tf-roll")).toEqual(["2 of them yours. New tickets join as soon as they match."]);
  expect(text(".tf-keys a.tf-mine")).toEqual(["PAY-231", "PAY-240"]);
  expect(target.querySelector<HTMLAnchorElement>(".tf-keys a")?.getAttribute("href")).toBe("#/job/jira%3APAY-231");
});

test("Add… marks what is already in, and adds what was picked as one save", async () => {
  await draw(["payout-stack"]);
  target.querySelector<HTMLButtonElement>("article[aria-label='Pages'] .tf-set-add button")!.click();
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
    "asset:pve-02",
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
  expect(saved.at(-1)?.members.map((member) => member.entity_id)).toEqual(["gitea:tidewater/payout-service", "asset:pve-02"]);
});

test("a member row draws its dot and its line, and a pattern its query and what it matches now (#868, #1008)", async () => {
  await draw(["payout-stack"]);
  const assets = target.querySelector("article[aria-label='Assets'] .tf-member")!;
  expect(assets.querySelector(".tf-dot")?.className).toBe("tf-dot tf-err");
  expect(assets.querySelector(".tf-member-line")?.textContent).toBe("vm, prod");
  const page = target.querySelector("article[aria-label='Pages'] .tf-member")!;
  expect(page.querySelector(".tf-dot")?.className.trim()).toBe("tf-dot");
  expect(page.querySelector(".tf-member-line")?.textContent).toBe("ENG › Payments");
  const pipelines = target.querySelector("article[aria-label='Pipelines']")!;
  expect(pipelines.querySelector("header .tf-dim")?.textContent).toBe("2");
  expect(pipelines.querySelector("summary")?.textContent?.replace(/\s+/g, " ").trim()).toBe(
    "Pattern: project:Payouts name:Payout_* (2 match now)",
  );
  expect([...pipelines.querySelectorAll("details li")].map((li) => li.textContent)).toEqual(["Payout_Build", "Payout_Lint"]);
  pipelines.querySelector<HTMLButtonElement>("button[aria-label='Remove the pattern project:Payouts name:Payout_*']")!.click();
  await settle();
  expect(saved.at(-1)?.patterns).toEqual([]);
});

test("Add a pattern… writes the fields as a query, shows what it matches now, and saves it", async () => {
  await draw(["payout-stack"]);
  expect(target.querySelector("article[aria-label='Repositories']")?.textContent).not.toContain("Add a pattern…");
  target.querySelector("article[aria-label='Pipelines']")!.querySelectorAll<HTMLButtonElement>(".tf-set-add button")[1]!.click();
  await settle();
  const dialog = document.querySelector(".tf-dialog")!;
  expect(dialog.querySelector("h2")?.textContent).toBe("Add a pattern to Payout stack");
  expect(patternPreviews.at(-1)).toBe("name:*");
  const project = dialog.querySelector<HTMLSelectElement>("select[aria-label='Project']")!;
  // Each project by its path, a parent before its subprojects (#1019).
  expect([...project.options].map((option) => option.textContent)).toEqual(["Any", "Payments", "Payments › Payouts"]);
  project.value = "Payouts";
  project.dispatchEvent(new Event("change", { bubbles: true }));
  const name = dialog.querySelector<HTMLInputElement>("input[aria-label='Name matches']")!;
  name.value = "Payout_Lint";
  name.dispatchEvent(new Event("input", { bubbles: true }));
  await settle();
  expect(patternPreviews.at(-1)).toBe("project:Payouts name:Payout_Lint");
  expect(dialog.querySelector(".tf-pane-h")?.textContent).toBe("Matches now: 1");
  expect([...dialog.querySelectorAll(".tf-pat-match")].map((row) => row.textContent)).toEqual(["Payout_LintPayments › Payouts"]);
  expect(dialog.textContent).toContain("Link sets stay in knobas and are never written to a source.");
  button("Add pattern").click();
  await settle();
  expect(saved.at(-1)?.patterns).toEqual([
    { kind: "pipeline", query: "project:Payouts name:Payout_*" },
    { kind: "pipeline", query: "project:Payouts name:Payout_Lint" },
  ]);
  expect(toasts.items.at(-1)?.text).toBe("Added a pattern to Payout stack");
});

test("a query the backend refuses says why, and Add pattern waits", async () => {
  await draw(["payout-stack"]);
  target.querySelector("article[aria-label='Pages']")!.querySelectorAll<HTMLButtonElement>(".tf-set-add button")[1]!.click();
  await settle();
  const dialog = document.querySelector(".tf-dialog")!;
  expect(dialog.querySelector("select[aria-label='Space']")).not.toBeNull();
  const under = dialog.querySelector<HTMLInputElement>("input[aria-label='Under page']")!;
  under.value = "bogus";
  under.dispatchEvent(new Event("input", { bubbles: true }));
  await settle();
  expect(dialog.querySelector(".tf-ifail")?.textContent).toContain("nothing else");
  expect(button("Add pattern").disabled).toBe(true);
});

test("Delete removes the set and goes back to the list", async () => {
  await draw(["payout-stack"]);
  button("Delete").click();
  await settle();
  expect(deleted).toEqual(["payout-stack"]);
  expect(location.hash).toBe("#/sets");
  expect(toasts.items.at(-1)?.text).toBe("Deleted the link set. Links drawn on tickets by hand stay.");
});

const DASHBOARD: LinkSetHint = {
  set_id: "payout-stack",
  set_name: "Payout stack",
  epic: "jira:PAY-200",
  member: { kind: "pipeline", entity_id: "tc:Dashboard_Build" },
  title: "Dashboard_Build",
  tickets: ["jira:PAY-240", "jira:PAY-241", "jira:PAY-245"],
};

test("no hint draws no card", async () => {
  await draw([]);
  expect(target.querySelector(".tf-hint-card")).toBeNull();
});

test("the hint card offers an item linked by hand under one epic to its set, in prototype-6's words (#870)", async () => {
  hints = [DASHBOARD, { ...DASHBOARD, title: "Other_Build", member: { kind: "pipeline", entity_id: "tc:Other" } }];
  await draw([]);
  const cards = target.querySelectorAll(".tf-hint-card");
  expect(cards).toHaveLength(1);
  expect(cards[0]!.querySelector("span")?.textContent?.replace(/\s+/g, " ").trim()).toBe(
    "You linked Dashboard_Build by hand to PAY-240 and 2 more tickets under PAY-200. Add it to Payout stack?",
  );
  expect([...cards[0]!.querySelectorAll("b")].map((b) => b.textContent)).toEqual(["Dashboard_Build", "Payout stack"]);
});

test("Add to … saves the set with the item added and nothing else changed", async () => {
  hints = [DASHBOARD];
  await draw([]);
  hints = [];
  button("Add to Payout stack").click();
  await settle();
  expect(saved).toHaveLength(1);
  expect(saved[0]!.id).toBe("payout-stack");
  expect(saved[0]!.members).toEqual([
    ...STACK.members.map((member) => ({ kind: member.kind, entity_id: member.entity_id })),
    { kind: "pipeline", entity_id: "tc:Dashboard_Build" },
  ]);
  expect(saved[0]!.patterns).toEqual([{ kind: "pipeline", query: "project:Payouts name:Payout_*" }]);
  expect(saved[0]!.rules).toEqual(STACK.rules.map((rule) => ({ kind: rule.kind, value: rule.value })));
  expect(toasts.items.at(-1)?.text).toBe("Added Dashboard_Build to Payout stack");
  expect(target.querySelector(".tf-hint-card")).toBeNull();
});

test("Not now puts the hint away for the session and shows the next one", async () => {
  const other = { ...DASHBOARD, title: "Other_Build", member: { kind: "pipeline" as const, entity_id: "tc:Other" } };
  hints = [DASHBOARD, other];
  await draw([]);
  button("Not now").click();
  await settle();
  expect(target.querySelector(".tf-hint-card b")?.textContent).toBe("Other_Build");
  unmount(view!);
  await draw([]);
  expect(target.querySelector(".tf-hint-card b")?.textContent).toBe("Other_Build");
  expect(saved).toEqual([]);
});

test("an id nobody holds says so", async () => {
  await draw(["gone"]);
  expect(target.querySelector(".tf-empty")?.textContent).toContain("No link set is called gone.");
});
