/**
 * *Apply a link set…* and *Save as a link set…* on the job (#869, R-7.8,
 * R-7.9), mounted with stand-ins for every read and write. What the backend
 * does with a saved set is `crates/knobas-app/tests/it/link_sets_ipc.rs`'s.
 */
import { pageContext } from "../kept-pages.svelte";
import { takeTold } from "../opener";
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
    description: null,
    markup: "text",
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
    submitWrite: reject,
    visitEntity: () => Promise.resolve(null),
    entityThread: () => Promise.resolve({ comments: [], me: [] }),
    createLink: reject,
    unlink: reject,
    search: reject,
    launcherHome: reject,
    jobReach: () => Promise.resolve([]),
    jobLinkSets: () => {
      setReads += 1;
      return Promise.resolve({ sets: applying, parent: null });
    },
    hideLinkSetMember: (_t, _m, hidden) => Promise.resolve(hidden),
    getAsset: reject,
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

/** The Connections drawer, opened with `L`, as the page opens it (#2399; its row on the card comes with #2401). */
function drawerOpen(): HTMLElement {
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "l", bubbles: true, cancelable: true }));
  flushSync();
  return target.querySelector<HTMLElement>("[aria-label='Connections']")!;
}

test("Apply a link set… lists the sets with what they hold, marks the one that already applies, and filters", async () => {
  open(LEDGER_APPLIES);
  await settle();
  button(drawerOpen(), "Apply a link set…").click();
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
  button(drawerOpen(), "Apply a link set…").click();
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

/** Nothing is picked on arrival (F-514, the enabled rule, which narrows #2289): the primary waits, and says so. */
test("Apply a link set… picks nothing on arrival, and its primary waits with a line saying why", async () => {
  open(LEDGER_APPLIES);
  await settle();
  button(drawerOpen(), "Apply a link set…").click();
  await settle();
  const radios = [...dialog().querySelectorAll<HTMLInputElement>("[role=radiogroup][aria-label='Link sets'] input[type=radio]")];
  expect(radios.map((r) => [r.checked, r.disabled])).toEqual([
    [false, true],
    [false, false],
  ]);
  expect(footer()).toEqual([
    ["Manage link sets", "tf-btn tf-first"],
    ["Cancel", "tf-btn tf-ghost"],
    ["Apply", "tf-btn tf-primary"],
  ]);
  expect(dialog().querySelector<HTMLButtonElement>("footer .tf-primary")!.disabled).toBe(true);
  expect(squash(dialog().querySelector("footer .tf-dwhy"))).toBe("Pick a link set.");
  pick("Payout stack");
  expect(footer().at(-1)?.[0]).toBe("Apply Payout stack");
  expect(dialog().querySelector("footer .tf-dwhy")).toBeNull();
});

test("back from Manage link sets, the page opens the drawer again, once, with focus in its filter", async () => {
  const shown = $state({ now: true });
  job = mount(TfJob, {
    target,
    context: pageContext(() => shown.now),
    props: {
      segment: "PAY-231",
      timer: { current: null, elapsed: null },
      layers: createLayers(),
      ontimer: () => {},
      onstarttimer: () => {},
      ports: ports([]),
      sets,
      today: () => new Date(2026, 9, 15, 14, 0),
    },
  });
  await settle();
  button(drawerOpen(), "Apply a link set…").click();
  await settle();
  dialog().querySelector<HTMLAnchorElement>("footer a")!.click();
  shown.now = false;
  await settle();
  const drawer = () => document.querySelector<HTMLElement>("[aria-label='Connections']");
  expect(drawer()).toBeNull();
  shown.now = true;
  await settle();
  expect(drawer()).not.toBeNull();
  expect(document.activeElement).toBe(drawer()!.querySelector("[data-filter]"));
  drawer()!.querySelector<HTMLButtonElement>(".tf-dclose")!.click();
  await settle();
  shown.now = false;
  await settle();
  shown.now = true;
  await settle();
  expect(drawer(), "only the trip that left from it").toBeNull();
  location.hash = "";
  await settle();
});

test("Manage link sets opens the sets' page to come back from, and the dialog and the drawer go with the move (F-520)", async () => {
  open();
  await settle();
  button(drawerOpen(), "Apply a link set…").click();
  await settle();
  dialog().querySelector<HTMLAnchorElement>("footer a")!.click();
  expect(takeTold(location.hash), "the shell stamps it as a visit").toBe("visit");
  expect(location.hash).toBe("#/sets");
  await settle();
  expect(document.querySelector("[role=dialog]")).toBeNull();
  expect(document.querySelector("[aria-label='Connections']"), "the drawer floats outside the page, so its page hiding would leave it").toBeNull();
});

/** #2289: typing is how the top match gets picked, so `Enter` in the filter still applies it. */
test("Enter in the filter applies the first set the filter leaves", async () => {
  open();
  await settle();
  button(drawerOpen(), "Apply a link set…").click();
  await settle();
  expect(footer().at(-1)?.[0]).toBe("Apply");
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
  button(drawerOpen(), "Apply a link set…").click();
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
  drawerOpen();
  flushSync();
  const acts = document.querySelector(".tf-dacts")!;
  expect([...acts.querySelectorAll("button")].map(squash)).toEqual(["Link to…", "Apply a link set…", "Save as a link set…"]);
});

test("Save as a link set… ticks the ticket's own items, offers the epic first with counts, and saves the set", async () => {
  open();
  await settle();
  drawerOpen();
  flushSync();
  button(document.querySelector(".tf-dacts")!, "Save as a link set…").click();
  await settle();
  expect(squash(dialog().querySelector("h2"))).toBe("Save PAY-231’s connections as a link set");
  expect(dialog().querySelector<HTMLInputElement>("input[aria-label='Name']")!.value).toBe("SEPA payouts kit");
  const items = [...dialog().querySelectorAll<HTMLLabelElement>("[aria-label='What goes in'] label")];
  expect(items.map(squash)).toEqual([
    "tidewater/payout-serviceRepository",
    "Payout_BuildPipeline",
    "SEPA payout retry designPage",
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

test("Save link set waits for a name and a tick, and a line says which is missing", async () => {
  open();
  await settle();
  drawerOpen();
  flushSync();
  button(document.querySelector(".tf-dacts")!, "Save as a link set…").click();
  await settle();
  const primary = () => button(dialog(), "Save link set");
  const line = () => squash(dialog().querySelector("footer .tf-dwhy"));
  expect(primary().disabled).toBe(false);
  const name = dialog().querySelector<HTMLInputElement>("input[aria-label='Name']")!;
  name.value = "   ";
  name.dispatchEvent(new Event("input"));
  flushSync();
  expect([primary().disabled, line()]).toEqual([true, "Name it first."]);
  for (const box of dialog().querySelectorAll<HTMLInputElement>("[aria-label='What goes in'] input")) box.click();
  flushSync();
  expect([primary().disabled, line()]).toEqual([true, "Name it, and tick what goes in."]);
  name.value = "Kit";
  name.dispatchEvent(new Event("input"));
  flushSync();
  expect([primary().disabled, line()]).toEqual([true, "Tick what goes in."]);
  primary().click();
  await settle();
  expect(saved).toEqual([]);
});

test("Save as a link set… on a sub-task names the epic above its story, and saves the epic rule (#1064)", async () => {
  open([], "PAY-232");
  await settle();
  drawerOpen();
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
  drawerOpen();
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
  drawerOpen();
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
  drawerOpen();
  flushSync();
  const hrefs = (title: string) =>
    [...document.querySelectorAll<HTMLAnchorElement>("[aria-label='Connections'] a")].find((a) => squash(a.querySelector(".tf-lk-name")) === title)?.getAttribute("href");
  expect(hrefs("Call the bank")).toBe("#/note/note%3A0f2c1a");
  expect(hrefs("POST /payouts")).toBe("#/route/route%3A9a1b");
});
