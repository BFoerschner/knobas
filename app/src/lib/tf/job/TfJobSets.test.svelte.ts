/**
 * *Apply a link set…* and *Save as a link set…* on the job (#869, R-7.8,
 * R-7.9), mounted with stand-ins for every read and write. What the backend
 * does with a saved set is `crates/knobas-app/tests/it/link_sets_ipc.rs`'s.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { EntityDetail, JobLinkSets, LinkEntry, LinkSetDetail, LinkSetDraft, LinkSetRow, LinkSetRule } from "../../ipc/entity";
import { toasts } from "../../shell/toasts.svelte";
import { createLayers } from "../layers.svelte";
import type { JobPorts, JobSetPorts } from "./ports";
import TfJob from "./TfJob.svelte";
import { sourceFacts } from "./source-facts.fixture";

const ID = "jira:PAY-231";
let target: HTMLElement;
let job: ReturnType<typeof mount> | null = null;
const saved: LinkSetDraft[] = [];
let setReads = 0;

beforeEach(() => {
  saved.length = 0;
  setReads = 0;
  toasts.items.length = 0;
  target = document.createElement("div");
  document.body.appendChild(target);
});

afterEach(() => {
  extraLinks = [];
  if (job) unmount(job);
  job = null;
  target.remove();
});

async function settle() {
  for (let turn = 0; turn < 8; turn += 1) await new Promise((resolve) => setTimeout(resolve, 0));
  flushSync();
}

const squash = (node: Element | null | undefined) => node?.textContent?.replace(/\s+/g, " ").trim() ?? "";

function link(otherId: string, kind: string, title: string): LinkEntry {
  return {
    link: {
      id: `link:${otherId}`,
      from_id: otherId,
      to_id: ID,
      relation: "related",
      origin: "manual",
      note: null,
      created_by: "mara",
      created_at: "2026-10-01T10:00:00Z",
      confirmed_at: "2026-10-01T10:00:00Z",
      rule: null,
      rule_class: null,
      reason: null,
    },
    other: { entity_id: otherId, kind, title, deleted_at: null },
  };
}

function detail(id: string, title: string, path: string | null, links: LinkEntry[] = []): EntityDetail {
  return {
    row: { entity_id: id, kind: "ticket", source_id: "jira", title, updated_at: null, synced_at: "2026-10-15T09:00:00Z", path, state: null },
    source: { id: "jira", display_name: "Jira", adapter_kind: "jira", enabled: true },
    kind_info: null,
    refreshes: true,
    body_text: "",
    author: null,
    payload: {},
    web_url: null,
    deleted_at: null,
    links,
    activity: [],
    builds: [],
    status: "In Progress",
    issue_type: null,
    priority: null,
    estimate_seconds: null,
  };
}

/** Links a test adds to the job's own, beside LINKS. */
let extraLinks: LinkEntry[] = [];

const LINKS = [
  link("gitea:tidewater/payout-service#142", "pr", "Idempotent retry window"),
  link("teamcity:Payout_Build", "build_config", "Payout_Build"),
  link("confluence:ENG-SEPA", "page", "SEPA payout retry design"),
];

const row = (id: string, name: string, rules: LinkSetRule[]): LinkSetRow => ({
  id,
  name,
  description: `${name} things`,
  members: [
    { kind: "repo", entity_id: "gitea:tidewater/ledger", title: "tidewater/ledger", line: null, state: null },
    { kind: "page", entity_id: "confluence:RUN", title: "Runbook", line: null, state: null },
  ],
  patterns: [],
  rules: rules.map((rule) => ({ ...rule, title: null })),
  tickets: 3,
  mine: 1,
});

const SETS = [row("ledger", "Ledger stack", [{ kind: "epic", value: "jira:PAY-150" }]), row("payout-stack", "Payout stack", [])];

const answer = (draft: LinkSetDraft): LinkSetDetail => ({
  ...row(draft.id ?? "saved", draft.name, draft.rules),
  description: draft.description,
  shows_on: [],
});

function ports(applying: JobLinkSets["sets"] = []): JobPorts {
  const reject = () => Promise.reject(new Error("not in this test"));
  return {
    listSources: () => Promise.resolve([sourceFacts("jira")]),
    getEntity: (id) =>
      id === ID
        ? Promise.resolve(detail(ID, "Retry failed SEPA payouts", "PAY-200", [...LINKS, ...extraLinks]))
        : id === "jira:PAY-232"
          ? Promise.resolve(detail(id, "Retry the SEPA batch", "PAY-231", LINKS))
          : id === "jira:PAY-200"
            ? Promise.resolve(detail(id, "Payout reliability", null))
            : Promise.reject({ code: "not_found", message: "", source_id: null }),
    promoteContext: (id) => Promise.resolve({ id: "ctx:1", kind: "ticket", title: "", anchor_id: id, created_at: "", archived_at: null }),
    listEntities: () => Promise.resolve({ rows: [], total: 0, tally: null, groups: null }),
    reachableTransitions: () => Promise.resolve([]),
    submitWrite: reject,
    entityCheckout: reject,
    weekTimesheet: () => Promise.resolve({ days: [], rows: [], past_horizon: [] }),
    visitEntity: () => Promise.resolve(null),
    entityThread: () => Promise.resolve({ comments: [], me: [] }),
    createLink: reject,
    unlink: reject,
    roomSuggestions: () => Promise.resolve({ rows: [], total: 0, more: [] }),
    search: reject,
    launcherHome: reject,
    jobReach: () => Promise.resolve([]),
    jobLinkSets: () => {
      setReads += 1;
      return Promise.resolve({ sets: applying, parent: null });
    },
    hideLinkSetMember: (_t, _m, hidden) => Promise.resolve(hidden),
    getAsset: reject,
    credentialHealth: () => Promise.resolve([]),
    syncStatus: () => Promise.resolve([]),
    startWork: { flow: reject, run: reject, retry: reject, skip: reject, amend: reject },
  };
}

const sets: JobSetPorts = {
  linkSets: () => Promise.resolve(SETS),
  saveLinkSet: (draft) => {
    saved.push(draft);
    return Promise.resolve(answer(draft));
  },
  previewLinkSetRule: (rule) =>
    Promise.resolve(
      Array.from({ length: rule.kind === "epic" ? 6 : 2 }, (_, n) => ({ entity_id: `jira:PAY-${n}`, title: "", mine: false })),
    ),
};

function open(applying: JobLinkSets["sets"] = [], segment = "PAY-231") {
  job = mount(TfJob, {
    target,
    props: {
      segment,
      tab: null,
      timer: { current: null, elapsed: null },
      layers: createLayers(),
      ontimer: () => {},
      onstarttimer: () => {},
      ports: ports(applying),
      sets,
      today: () => new Date(2026, 9, 15, 14, 0),
    },
  });
  flushSync();
}

const button = (scope: ParentNode, name: string) =>
  [...scope.querySelectorAll<HTMLButtonElement>("button")].find((b) => squash(b) === name || b.getAttribute("aria-label") === name)!;

const dialog = () => document.querySelector<HTMLElement>(".tf-dialog")!;

const LEDGER_APPLIES: JobLinkSets["sets"] = [
  { id: "ledger", name: "Ledger stack", via: { kind: "manual", value: ID, title: null }, members: [] },
];

test("Apply a link set… lists the sets with what they hold, marks the one that already applies, and filters", async () => {
  open(LEDGER_APPLIES);
  await settle();
  button(target.querySelector(".tf-setsbar")!, "Apply a link set…").click();
  await settle();
  expect(squash(dialog().querySelector("h2"))).toBe("Apply a link set to PAY-231");
  expect([...dialog().querySelectorAll(".tf-pickrow")].map(squash)).toEqual([
    "Ledger stack1 repository, 1 page already applies",
    "Payout stack1 repository, 1 page",
  ]);
  expect(squash(dialog())).toContain("Link sets stay in knobas and are never written to a source.");
  const field = dialog().querySelector<HTMLInputElement>("input[aria-label='Filter link sets']")!;
  field.value = "payout";
  field.dispatchEvent(new Event("input"));
  flushSync();
  expect([...dialog().querySelectorAll(".tf-pickrow b")].map(squash)).toEqual(["Payout stack"]);
});

test("Apply adds a by-hand rule, re-reads the job, and Undo takes the rule out again", async () => {
  open();
  await settle();
  const before = setReads;
  button(target.querySelector(".tf-setsbar")!, "Apply a link set…").click();
  await settle();
  pick("Payout stack");
  button(dialog(), "Apply Payout stack").click();
  await settle();
  expect(saved.at(-1)?.rules).toEqual([{ kind: "manual", value: ID }]);
  expect(saved.at(-1)?.id).toBe("payout-stack");
  expect(document.querySelector(".tf-dialog")).toBeNull();
  expect(setReads).toBeGreaterThan(before);
  const toast = toasts.items.find((t) => t.text === "Payout stack applied to PAY-231. 2 items show on the job.")!;
  expect(toast.action?.label).toBe("Undo");
  toast.action!.run();
  await settle();
  expect(saved.at(-1)?.rules).toEqual([]);
  expect(saved.at(-1)?.id).toBe("payout-stack");
});

/** Picks a set in the Apply dialog by its name, as a click on its row does. */
function pick(name: string) {
  const row = [...dialog().querySelectorAll<HTMLElement>(".tf-pickrow")].find((r) => squash(r.querySelector("b")) === name)!;
  row.querySelector<HTMLInputElement>("input[type=radio]")!.click();
  flushSync();
}

const footer = () => [...dialog().querySelectorAll<HTMLElement>("footer .tf-btn")].map((b) => [squash(b), b.className]);

/** #2289: the dialog's primary says what Enter does, so the first set that can apply is picked from the start. */
test("Apply a link set… has a primary that applies the picked set, the first one that can apply until another is picked", async () => {
  open(LEDGER_APPLIES);
  await settle();
  button(target.querySelector(".tf-setsbar")!, "Apply a link set…").click();
  await settle();
  const radios = [...dialog().querySelectorAll<HTMLInputElement>("[role=radiogroup][aria-label='Link sets'] input[type=radio]")];
  expect(radios.map((r) => [r.checked, r.disabled])).toEqual([
    [false, true],
    [true, false],
  ]);
  expect(footer()).toEqual([
    ["Manage link sets", "tf-btn tf-first"],
    ["Cancel", "tf-btn tf-ghost"],
    ["Apply Payout stack", "tf-btn tf-primary"],
  ]);
});

test("Enter in the filter applies the first set the filter leaves", async () => {
  open();
  await settle();
  button(target.querySelector(".tf-setsbar")!, "Apply a link set…").click();
  await settle();
  expect(footer().at(-1)?.[0]).toBe("Apply Ledger stack");
  const field = dialog().querySelector<HTMLInputElement>("input[aria-label='Filter link sets']")!;
  field.value = "payout";
  field.dispatchEvent(new Event("input"));
  flushSync();
  expect(footer().at(-1)?.[0]).toBe("Apply Payout stack");
  // A text field's Enter is its form's implicit submission.
  field.form!.requestSubmit();
  await settle();
  expect(saved.at(-1)?.id).toBe("payout-stack");
  expect(document.querySelector(".tf-dialog")).toBeNull();
});

test("Apply is disabled when no set the filter leaves can apply", async () => {
  open(LEDGER_APPLIES);
  await settle();
  button(target.querySelector(".tf-setsbar")!, "Apply a link set…").click();
  await settle();
  const field = dialog().querySelector<HTMLInputElement>("input[aria-label='Filter link sets']")!;
  field.value = "ledger";
  field.dispatchEvent(new Event("input"));
  flushSync();
  const primary = dialog().querySelector<HTMLButtonElement>("footer .tf-primary")!;
  expect([squash(primary), primary.disabled]).toEqual(["Apply", true]);
  field.form!.requestSubmit();
  await settle();
  expect(saved).toEqual([]);
});

test("the drawer offers Apply a link set… and Save as a link set… beside Link to…", async () => {
  open();
  await settle();
  target.querySelector<HTMLButtonElement>(".tf-conn")!.click();
  flushSync();
  const acts = document.querySelector(".tf-dacts")!;
  expect([...acts.querySelectorAll("button")].map(squash)).toEqual(["Link to…", "Apply a link set…", "Save as a link set…"]);
});

test("Save as a link set… ticks the ticket's own items, offers the epic first with counts, and saves the set", async () => {
  open();
  await settle();
  target.querySelector<HTMLButtonElement>(".tf-conn")!.click();
  flushSync();
  button(document.querySelector(".tf-dacts")!, "Save as a link set…").click();
  await settle();
  expect(squash(dialog().querySelector("h2"))).toBe("Save PAY-231’s connections as a link set");
  expect(dialog().querySelector<HTMLInputElement>("input[aria-label='Name']")!.value).toBe("SEPA payouts kit");
  const items = [...dialog().querySelectorAll<HTMLLabelElement>("[aria-label='What goes in'] label")];
  expect(items.map(squash)).toEqual([
    "tidewater/payout-serviceRepositories",
    "Payout_BuildPipelines",
    "SEPA payout retry designPages",
  ]);
  expect(items.every((label) => label.querySelector("input")!.checked)).toBe(true);
  expect(squash(dialog())).toContain("PRs, branches and commits belong to this one ticket, so they are not offered.");
  expect(squash(dialog())).toContain("Link sets stay in knobas and are never written to a source.");
  expect([...dialog().querySelectorAll(".tf-radio label")].map(squash)).toEqual([
    "Only PAY-231",
    "Every ticket under epic PAY-200 Payout reliability6 tickets today, and new ones as they are created",
    "Every sub-task of PAY-2312 today",
    "Every ticket in project PAY",
  ]);
  expect(dialog().querySelector<HTMLInputElement>("input[value='epic']")!.checked).toBe(true);

  items[1]!.querySelector("input")!.click();
  flushSync();
  button(dialog(), "Save link set").click();
  await settle();
  expect(saved.at(-1)).toEqual({
    id: null,
    name: "SEPA payouts kit",
    description: "Saved from PAY-231.",
    members: [
      { kind: "repo", entity_id: "gitea:tidewater/payout-service" },
      { kind: "page", entity_id: "confluence:ENG-SEPA" },
    ],
    patterns: [],
    rules: [{ kind: "epic", value: "jira:PAY-200" }],
  });
  expect(document.querySelector(".tf-dialog")).toBeNull();
  expect(document.querySelector("[aria-label='Connections']")).toBeNull();
  const toast = toasts.items.find((t) => t.text === "Saved SEPA payouts kit. Every ticket under epic PAY-200 now shows it.")!;
  expect(toast.action?.label).toBe("Open it");
});

test("Save as a link set… on a sub-task names the epic above its story, and saves the epic rule (#1064)", async () => {
  open([], "PAY-232");
  await settle();
  target.querySelector<HTMLButtonElement>(".tf-conn")!.click();
  flushSync();
  button(document.querySelector(".tf-dacts")!, "Save as a link set…").click();
  await settle();
  expect(squash(dialog().querySelector("input[value='epic']")!.closest("label"))).toBe(
    "Every ticket under epic PAY-200 Payout reliability6 tickets today, and new ones as they are created",
  );
  expect(dialog().querySelector<HTMLInputElement>("input[value='epic']")!.checked).toBe(true);
  button(dialog(), "Save link set").click();
  await settle();
  expect(saved.at(-1)?.rules).toEqual([{ kind: "epic", value: "jira:PAY-200" }]);
});

test("Save link set pressed before the walk up ends still saves the epic, not the story (#1064)", async () => {
  open([], "PAY-232");
  await settle();
  target.querySelector<HTMLButtonElement>(".tf-conn")!.click();
  flushSync();
  button(document.querySelector(".tf-dacts")!, "Save as a link set…").click();
  flushSync();
  button(dialog(), "Save link set").click();
  await settle();
  expect(saved.at(-1)?.rules).toEqual([{ kind: "epic", value: "jira:PAY-200" }]);
});

test("Save as a link set… can apply it to the project", async () => {
  open();
  await settle();
  target.querySelector<HTMLButtonElement>(".tf-conn")!.click();
  flushSync();
  button(document.querySelector(".tf-dacts")!, "Save as a link set…").click();
  await settle();
  dialog().querySelector<HTMLInputElement>("input[value='project']")!.click();
  flushSync();
  button(dialog(), "Save link set").click();
  await settle();
  expect(saved.at(-1)?.rules).toEqual([{ kind: "project", value: "jira:PAY" }]);
});

test("a note and a route in the drawer open their own addresses, not Tickets (#1101)", async () => {
  extraLinks = [link("note:0f2c1a", "note", "Call the bank"), link("route:9a1b", "route", "POST /payouts")];
  open();
  await settle();
  target.querySelector<HTMLButtonElement>(".tf-conn")!.click();
  flushSync();
  const hrefs = (title: string) =>
    [...document.querySelectorAll<HTMLAnchorElement>("[aria-label='Connections'] a")].find((a) => squash(a) === title)?.getAttribute("href");
  expect(hrefs("Call the bank")).toBe("#/note/note%3A0f2c1a");
  expect(hrefs("POST /payouts")).toBe("#/route/route%3A9a1b");
});
