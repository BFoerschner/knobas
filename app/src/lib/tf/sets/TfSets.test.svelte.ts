/**
 * Link sets (#866, R-7.6, R-7.7), mounted over stand-in ports: the list's
 * rows, filter and switch, *New link set*, and the editor's panels, picker,
 * rules with their live counts, *Shows on* and *Delete*, each change saved
 * as the whole set.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { LinkSetDetail, LinkSetDraft, LinkSetHint, LinkSetRow, LinkSetRule, RuleTicket } from "../../ipc/entity";
import { layers } from "../layers.svelte";
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
    assetTypeNames: async () => ({ vm: "VM" }),
    monitors: async () => [],
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

async function draw(rest: string[], io: SetPorts = ports()) {
  view = mount(TfSets, { target, props: { ready: true, rest, ports: io } });
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
    "2 repositories, 2 pipelines, 1 page, 1 asset",
    "Every ticket under epic PAY-200 Payout reliabilityPAY-228, applied by hand",
    "3 (2 yours)",
  ]);
  expect(text("tbody tr:nth-child(2) td")[2]).toBe("Every ticket in project OPS");
});

test("the filter keeps sets by name or member, and by what they apply by", async () => {
  await draw([]);
  const field = target.querySelector<HTMLInputElement>("input[placeholder='Filter link sets']")!;
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

test("the sub-task switch is R-7.3's, and New link set opens a set with nothing saved yet", async () => {
  await draw([]);
  const toggle = target.querySelector<HTMLInputElement>(".tf-toggle input")!;
  expect(toggle.checked).toBe(true);
  toggle.click();
  await settle();
  expect(inherit).toEqual([false]);
  button("New link set").click();
  await settle();
  expect(saved, "leaving now leaves no row behind").toEqual([]);
  expect(location.hash).toBe("#/sets/~new");
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
  expect(text(".tf-set-panels .tf-panel:first-child .tf-member > .tf-member-name")).toEqual([
    "tidewater/payout-service",
    "tidewater/psp-client",
  ]);
  expect(text(".tf-set-panels .tf-panel:nth-child(5) .tf-member")).toEqual(["No monitors yet."]);
  expect(previews).toEqual([
    { kind: "epic", value: "jira:PAY-200" },
    { kind: "manual", value: "jira:PAY-228" },
  ]);
  expect(text(".tf-rule-n")).toEqual(["2 tickets", "1 ticket"]);
  expect(text(".tf-roll")).toEqual(["2 of them yours, outlined. New tickets join as soon as they match."]);
  expect(text(".tf-keys a.tf-mine")).toEqual(["PAY-231", "PAY-240"]);
  expect(target.querySelector<HTMLAnchorElement>(".tf-keys a")?.getAttribute("href")).toBe("#/ticket/jira%3APAY-231");
});

test("Add… marks what is already in, and adds what was picked as one save", async () => {
  await draw(["payout-stack"]);
  [...target.querySelectorAll<HTMLButtonElement>("article[aria-label='Pages'] .tf-set-add button")].at(-1)!.click();
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
  // One thing is named, and *Undo* takes it back as a *Remove*'s does (F-547).
  const toast = toasts.items.at(-1)!;
  expect(toast.text).toBe("Added Payout batch lifecycle to Payout stack");
  expect(toast.action?.label).toBe("Undo");
  toast.action?.run();
  await settle();
  expect(saved.at(-1)?.members.map((member) => member.entity_id)).not.toContain("confluence:2");
  expect(saved.at(-1)?.members).toHaveLength(4);
});

test("a toast's Undo goes to the set it came from, whichever set the editor has moved on to", async () => {
  await draw(["payout-stack"]);
  [...target.querySelectorAll<HTMLButtonElement>("article[aria-label='Pages'] .tf-set-add button")].at(-1)!.click();
  await settle();
  document.querySelectorAll<HTMLInputElement>(".tf-dialog .tf-picks input")[1]!.click();
  await settle();
  button("Add").click();
  await settle();
  const toast = toasts.items.at(-1)!;
  // The editor is gone: the set is read from the backend, changed and saved.
  void unmount(view!);
  view = null;
  stored = saved.at(-1) as unknown as LinkSetDetail;
  toast.action?.run();
  await settle();
  expect(saved.at(-1)?.id).toBe("payout-stack");
  expect(saved.at(-1)?.members.map((member) => member.entity_id)).not.toContain("confluence:2");
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

test("a Remove acts at once with Undo on its toast, and removals in a row are one toast that puts back all of them", async () => {
  await draw(["payout-stack"]);
  const before = saved.length;
  target.querySelector<HTMLButtonElement>("button[aria-label='Remove tidewater/psp-client']")!.click();
  await settle();
  const first = toasts.items.at(-1)!;
  expect(first.text).toBe("Removed tidewater/psp-client from Payout stack");
  expect(first.action?.label).toBe("Undo");
  [...target.querySelectorAll<HTMLElement>(".tf-rule")].at(0)!.querySelector<HTMLButtonElement>(".tf-x")!.click();
  await settle();
  const undoing = toasts.items.filter((toast) => toast.action?.label === "Undo");
  expect(undoing.map((toast) => toast.text)).toEqual(["Removed 2 from Payout stack"]);
  expect(saved.at(-1)?.rules).toEqual([{ kind: "manual", value: "jira:PAY-228" }]);
  undoing[0]!.action!.run();
  await settle();
  expect(saved).toHaveLength(before + 3);
  expect(saved.at(-1)?.members.map((member) => member.entity_id)).toContain("gitea:tidewater/psp-client");
  expect(saved.at(-1)?.members[1]?.entity_id, "back where it stood").toBe("gitea:tidewater/psp-client");
  expect(saved.at(-1)?.rules).toEqual([
    { kind: "epic", value: "jira:PAY-200" },
    { kind: "manual", value: "jira:PAY-228" },
  ]);
});

test("a rule waiting for its value says so, Add a rule says why it waits, and Esc puts the rule away", async () => {
  await draw(["payout-stack"]);
  button("Add a rule").click();
  await settle();
  expect(button("Add a rule").disabled).toBe(true);
  expect(button("Add a rule").title).toBe("Choose what the rule above applies to first");
  expect(text(".tf-rule-n").at(-1)).toBe("Choose what it applies to.");
  expect([...target.querySelectorAll(".tf-rule")].at(-1)!.querySelector(".tf-x")?.textContent).toBe("Cancel");
  expect(layers.open.map((layer) => layer.kind)).toEqual(["panel"]);
  layers.open[0]!.close();
  await settle();
  expect(layers.open).toEqual([]);
  expect(button("Add a rule").disabled).toBe(false);
  expect(target.querySelectorAll(".tf-rule")).toHaveLength(2);
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
  expect(assets.querySelector(".tf-member-line")?.textContent).toBe("VM, prod");
  const page = target.querySelector("article[aria-label='Pages'] .tf-member")!;
  expect(page.querySelector(".tf-dot")).toBeNull();
  expect(page.querySelector(".tf-member-line")?.textContent).toBe("ENG › Payments");
  const pipelines = target.querySelector("article[aria-label='Pipelines']")!;
  expect(pipelines.querySelector("header .tf-count")?.textContent).toBe("2");
  expect(pipelines.querySelector("summary")?.textContent?.replace(/\s+/g, " ").trim()).toBe(
    "Pattern: project:Payouts name:Payout_* (2 matches now)",
  );
  expect([...pipelines.querySelectorAll("details li")].map((li) => li.textContent)).toEqual(["Payout_Build", "Payout_Lint"]);
  pipelines.querySelector<HTMLButtonElement>("button[aria-label='Remove the pattern project:Payouts name:Payout_*']")!.click();
  await settle();
  expect(saved.at(-1)?.patterns).toEqual([]);
});

test("Add a pattern… writes the fields as a query, shows what it matches now, and saves it", async () => {
  await draw(["payout-stack"]);
  expect(target.querySelector("article[aria-label='Repositories']")?.textContent).not.toContain("Add a pattern…");
  target.querySelector("article[aria-label='Pipelines']")!.querySelectorAll<HTMLButtonElement>(".tf-set-add button")[0]!.click();
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
  const toast = toasts.items.at(-1)!;
  expect(toast.text).toBe("Added a pattern to Payout stack");
  toast.action?.run();
  await settle();
  expect(saved.at(-1)?.patterns).toEqual([{ kind: "pipeline", query: "project:Payouts name:Payout_*" }]);
});

test("a query the backend refuses says why, and Add pattern waits", async () => {
  await draw(["payout-stack"]);
  target.querySelector("article[aria-label='Pages']")!.querySelectorAll<HTMLButtonElement>(".tf-set-add button")[0]!.click();
  await settle();
  const dialog = document.querySelector(".tf-dialog")!;
  const space = dialog.querySelector<HTMLSelectElement>("select[aria-label='Space']")!;
  space.value = space.options[1]!.value;
  space.dispatchEvent(new Event("change", { bubbles: true }));
  await settle();
  const under = dialog.querySelector<HTMLInputElement>("input[aria-label='Under page']")!;
  under.value = "bogus";
  under.dispatchEvent(new Event("input", { bubbles: true }));
  await settle();
  expect(dialog.querySelector(".tf-ifail")?.textContent).toContain("nothing else");
  expect(button("Add pattern").disabled).toBe(true);
});

test("the set editor's Delete starts a destructive action: red text, as Note's (#2288)", async () => {
  await draw(["payout-stack"]);
  expect(button("Delete…").className).toBe("tf-btn tf-danger");
  expect(button("Delete…").closest(".tf-fhead .tf-acts")).not.toBeNull();
});

test("the list's empty states say what fills them, offer the action, and a failed read offers to try again", async () => {
  const base = ports();
  await draw([], { ...base, linkSets: vi.fn(async () => []) });
  const empty = target.querySelector(".tf-sets .tf-empty")!;
  expect(empty.textContent).toContain("No link sets yet.");
  expect(empty.querySelector(".tf-btn.tf-primary")?.textContent).toBe("New link set");
  expect([...target.querySelectorAll(".tf-btn.tf-primary")].length, "one New link set, in the empty block").toBe(1);
  expect(target.querySelector(".tf-fbar"), "nothing to filter").toBeNull();
  unmount(view!);
  view = null;
  target.replaceChildren();

  let fails = true;
  const linkSets = vi.fn(async () => {
    if (fails) throw { code: "db", message: "the database is away" };
    return [];
  });
  await draw([], { ...base, linkSets });
  const failed = target.querySelector(".tf-sets .tf-empty")!;
  expect(failed.textContent).toContain("Could not read the link sets");
  fails = false;
  failed.querySelector<HTMLButtonElement>("button")!.click();
  await settle();
  expect(linkSets).toHaveBeenCalledTimes(2);
  expect(target.querySelector(".tf-sets .tf-empty")?.textContent).toContain("No link sets yet.");
});

test("a filter that leaves nothing offers to clear it", async () => {
  await draw([]);
  const field = target.querySelector<HTMLInputElement>(".tf-fbar input")!;
  field.value = "zzzz";
  field.dispatchEvent(new Event("input", { bubbles: true }));
  await settle();
  const empty = target.querySelector(".tf-sets .tf-empty")!;
  expect(empty.textContent).toContain("No link sets match.");
  empty.querySelector<HTMLButtonElement>("button")!.click();
  await settle();
  expect(target.querySelector(".tf-sets .tf-empty")).toBeNull();
});

test("Delete asks first, in a dialog that says what stays; Cancel keeps the set", async () => {
  await draw(["payout-stack"]);
  button("Delete…").click();
  await settle();
  expect(deleted).toEqual([]);
  const dialog = document.querySelector(".tf-dialog")!;
  expect(dialog.textContent).toContain("Links drawn on tickets by hand stay.");
  const footer = [...dialog.querySelectorAll<HTMLButtonElement>("footer button")];
  expect(footer.map((b) => [b.textContent?.trim(), b.className])).toEqual([
    ["Cancel", "tf-btn tf-ghost"],
    ["Delete link set", "tf-btn tf-primary tf-danger"],
  ]);
  footer[0]!.click();
  await settle();
  expect(document.querySelector(".tf-dialog")).toBeNull();
  expect(deleted).toEqual([]);
});

test("the editor's breadcrumb names the set, and leads back to Link sets", async () => {
  await draw(["payout-stack"]);
  const crumbs = target.querySelector(".tf-crumbs")!;
  expect(crumbs.querySelector("a.tf-crumb")?.getAttribute("href")).toBe("#/sets");
  expect(crumbs.querySelector("strong")?.textContent).toBe("Payout stack");
});

test("a chip for a ticket is neutral: the chosen job's yellow is on no key", async () => {
  await draw(["payout-stack"]);
  expect(target.querySelector(".tf-keys .tf-card")).toBeNull();
});

test("Delete removes the set and goes back to the list", async () => {
  await draw(["payout-stack"]);
  button("Delete…").click();
  await settle();
  [...document.querySelectorAll<HTMLButtonElement>(".tf-dialog footer button")].find((b) => b.textContent?.trim() === "Delete link set")!.click();
  await settle();
  expect(deleted).toEqual(["payout-stack"]);
  expect(location.hash).toBe("#/sets");
  expect(toasts.items.at(-1)?.text).toBe("Deleted Payout stack. Links drawn on tickets by hand stay.");
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
  expect(target.querySelector(".tf-empty")?.textContent).toContain("This link set does not exist.");
});

/** F-527: nothing is made until the first change, so leaving at once leaves no empty row. */
test("a new set saves nothing until its first change, and that change moves the address in place", async () => {
  await draw(["~new"]);
  expect(saved).toEqual([]);
  expect(button("Delete…"), "nothing to delete yet").toBeUndefined();
  const name = target.querySelector<HTMLElement>(".tf-set-title")!;
  expect(document.activeElement, "the name takes focus, to type over").toBe(name);

  name.textContent = "Deploy kit";
  name.dispatchEvent(new FocusEvent("focusout", { bubbles: true }));
  name.blur();
  await settle();
  expect(saved).toHaveLength(1);
  expect(saved[0]).toMatchObject({ id: null, name: "Deploy kit" });
  expect(button("Delete…")).toBeDefined();
});

test("saving a name keeps focus in the field the reader moved to", async () => {
  await draw(["payout-stack"]);
  const name = target.querySelector<HTMLElement>(".tf-set-title")!;
  const description = target.querySelector<HTMLElement>(".tf-set-desc")!;
  name.focus();
  name.textContent = "Payout stack two";
  description.focus();
  await settle();
  expect(saved.at(-1)?.name).toBe("Payout stack two");
  expect(document.activeElement).toBe(description);
  expect(target.querySelector(".tf-set-desc")).toBe(description);
});

/** F-521: a press that disables *Add a rule* takes focus on to what it asks for, and comes back. */
test("Add a rule takes focus to the rule's value, and Esc brings it back to the button", async () => {
  await draw(["payout-stack"]);
  button("Add a rule").focus();
  button("Add a rule").click();
  await settle();
  const value = [...target.querySelectorAll<HTMLSelectElement>("select[aria-label='Applies to which']")].at(-1)!;
  expect(document.activeElement).toBe(value);

  value.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }));
  await settle();
  expect(target.querySelectorAll(".tf-rule")).toHaveLength(2);
  expect(document.activeElement).toBe(button("Add a rule"));
  expect(layers.open).toEqual([]);
});

test("changing a waiting rule's kind offers that kind's values", async () => {
  await draw(["payout-stack"]);
  button("Add a rule").click();
  await settle();
  const kind = [...target.querySelectorAll<HTMLSelectElement>("select[aria-label='Rule']")].at(-1)!;
  kind.value = "project";
  kind.dispatchEvent(new Event("change", { bubbles: true }));
  await settle();
  const value = [...target.querySelectorAll<HTMLSelectElement>("select[aria-label='Applies to which']")].at(-1)!;
  expect([...value.options].map((option) => option.value)).toContain("jira:OPS");
});

/** F-521: a *Remove* takes its row, and focus goes to the row that took its place, else the way to add one. */
test("removing a member moves focus to the next row, then to the panel's Add…", async () => {
  await draw(["payout-stack"]);
  const panel = target.querySelector("article[aria-label='Repositories']")!;
  const first = panel.querySelector<HTMLButtonElement>("button[aria-label='Remove tidewater/payout-service']")!;
  first.focus();
  first.click();
  await settle();
  await settle();
  const second = panel.querySelector<HTMLButtonElement>("button[aria-label='Remove tidewater/psp-client']")!;
  expect(document.activeElement).toBe(second);
  second.click();
  await settle();
  await settle();
  expect(document.activeElement?.textContent).toBe("Add…");
});

/** F-528: pages wait for a space as pipelines wait for a project, and it says *space* wherever it says it. */
test("a page pattern waits for a space, and reads space where it stood as project", async () => {
  await draw(["payout-stack"]);
  target.querySelector("article[aria-label='Pages']")!.querySelectorAll<HTMLButtonElement>(".tf-set-add button")[0]!.click();
  await settle();
  const dialog = document.querySelector(".tf-dialog")!;
  const space = dialog.querySelector<HTMLSelectElement>("select[aria-label='Space']")!;
  expect(space.value).toBe("");
  expect(button("Add pattern").disabled).toBe(true);
  expect(dialog.querySelector(".tf-dwhy")?.textContent).toBe("Pick a space.");
  expect(dialog.querySelector(".tf-pat-query")).toBeNull();

  space.value = "Payouts";
  space.dispatchEvent(new Event("change", { bubbles: true }));
  await settle();
  expect(button("Add pattern").disabled).toBe(false);
  expect(dialog.querySelector(".tf-pat-query")?.textContent).toBe("Pattern: space:Payouts");

  button("Add pattern").click();
  await settle();
  expect(saved.at(-1)?.patterns.at(-1), "the stored query stays the backend's").toEqual({ kind: "page", query: "project:Payouts" });
});

test("Add to … takes back through Undo: the item goes and the offer comes back (F-536)", async () => {
  hints = [DASHBOARD];
  await draw([]);
  hints = [];
  button("Add to Payout stack").click();
  await settle();
  const toast = toasts.items.at(-1)!;
  expect(toast.action?.label).toBe("Undo");
  hints = [DASHBOARD];
  toast.action?.run();
  await settle();
  expect(saved.at(-1)!.members.map((member) => member.entity_id)).not.toContain("tc:Dashboard_Build");
  expect(target.querySelector(".tf-hint-card")).not.toBeNull();
});

test("a set's title keeps focus: Enter saves it, Esc on changed words puts them back, Esc on nothing changed goes on (F-539)", async () => {
  await draw(["payout-stack"]);
  const name = target.querySelector<HTMLElement>(".tf-set-title")!;
  name.focus();
  const key = (which: string) => {
    const event = new KeyboardEvent("keydown", { key: which, bubbles: true, cancelable: true });
    const onwards = vi.fn();
    window.addEventListener("keydown", onwards, { once: true });
    name.dispatchEvent(event);
    window.removeEventListener("keydown", onwards);
    return onwards.mock.calls.length > 0;
  };
  name.textContent = "Renamed";
  key("Enter");
  await settle();
  expect(document.activeElement).toBe(name);
  expect(saved.at(-1)).toMatchObject({ name: "Renamed" });

  name.textContent = "Typed over";
  expect(key("Escape"), "the field keeps the press").toBe(false);
  expect(name.textContent).toBe("Renamed");
  expect(document.activeElement).toBe(name);

  expect(key("Escape"), "nothing to undo: it reaches the shell").toBe(true);
  expect(document.activeElement).not.toBe(name);
});

test("a new rule opens on the first kind with something to choose (F-531)", async () => {
  await draw(["~new"]);
  button("Add a rule").click();
  await settle();
  const kind = target.querySelector<HTMLSelectElement>("select[aria-label='Rule']")!;
  const value = target.querySelector<HTMLSelectElement>("select[aria-label='Applies to which']")!;
  expect([...value.options].length, kind.value).toBeGreaterThan(1);
});
