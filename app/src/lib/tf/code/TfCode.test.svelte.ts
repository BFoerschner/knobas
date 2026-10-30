/**
 * Code (#837, R-8.1, R-8.2), mounted over stand-in reads: the two tabs and
 * their counts, the repository rows, the pull request groups from the
 * attention read (#712), the job column and chip, the filters, and where
 * *New pull request* goes. What `list_entities` groups is the backend's
 * (`knobas-app`'s attention tests); here the reads are stand-ins.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { CheckoutView, CodeFacts, EntityFilter, EntityPage, EntityRow, GroupTally, Reached } from "../../ipc/entity";
import type { CodePorts } from "./TfCode.svelte";
import TfCode from "./TfCode.svelte";
import { sourceFacts } from "../job/source-facts.fixture";

const NOW = new Date(2026, 7, 22, 14, 32);

function row(entity_id: string, kind: string, title: string, extra: Partial<EntityRow> = {}): EntityRow {
  return {
    entity_id,
    kind,
    source_id: "gitea",
    title,
    updated_at: "2026-08-22T08:20:00Z",
    synced_at: "2026-08-22T14:30:00Z",
    path: null,
    state: null,
    ...extra,
  };
}

const PAYOUT = "tidewater/payout-service";
const repo = (name: string, updated: string) => row(`gitea:tidewater/${name}`, "repo", `tidewater/${name}`, { updated_at: updated });
const pr = (n: number, title: string, state: "open" | "merged" | "closed", path = PAYOUT, updated = "2026-08-22T08:20:00Z") =>
  row(`gitea:${path}#${n}`, "pr", title, {
    path,
    updated_at: updated,
    state: { text: state, tone: state === "open" ? "info" : state === "merged" ? "ok" : "muted" },
  });

const REPOS = [
  repo("ledger-api", "2026-08-21T10:00:00Z"),
  repo("payout-service", "2026-08-20T10:00:00Z"),
  repo("fx-rates", "2026-08-22T09:00:00Z"),
];
const OPEN = [
  pr(144, "Add payout CSV export", "open"),
  pr(142, "SEPA retry with exponential backoff", "open"),
  pr(77, "Bump dependencies", "open", "tidewater/ledger-api", "2026-08-10T10:00:00Z"),
];
const GROUPS: GroupTally[] = [
  { group: "review_requested", count: 1 },
  { group: "my_pull_requests", count: 1 },
  { group: "open_pull_requests", count: 1 },
];
const SETTLED = [pr(139, "Fix ledger drift on partial refunds", "merged", "tidewater/ledger-api")];
const BRANCH = row(`gitea:${PAYOUT}@refs/heads/feature/PAY-231-sepa-retry`, "branch", "feature/PAY-231-sepa-retry", { path: PAYOUT });

/** What `code_facts` answers (#966): teams and languages, authors, and mara's two pull requests. */
const fact = (entity_id: string, over: Partial<CodeFacts>): CodeFacts => ({
  entity_id,
  author: null,
  mine: false,
  language: null,
  teams: [],
  ...over,
});
const FACTS: CodeFacts[] = [
  fact("gitea:tidewater/payout-service", { teams: ["Payments"], language: "Rust" }),
  fact("gitea:tidewater/ledger-api", { teams: ["Payments"], language: "Kotlin" }),
  fact("gitea:tidewater/fx-rates", { teams: ["Platform"], language: "Go" }),
  fact(`gitea:${PAYOUT}#144`, { author: "tom" }),
  fact(`gitea:${PAYOUT}#142`, { author: "mara", mine: true }),
  fact("gitea:tidewater/ledger-api#77", { author: "jonas" }),
  fact("gitea:tidewater/ledger-api#139", { author: "mara", mine: true }),
];

function page(rows: EntityRow[], groups: GroupTally[] | null = null): EntityPage {
  return { rows, total: rows.length, tally: null, groups };
}

function checkout(path: string | null): CheckoutView {
  return {
    repo_entity_id: null,
    repo_url: null,
    path,
    found_by: path === null ? "nothing" : "scan",
    clones_root: "/Users/mara/src",
    clone_command: null,
    tree:
      path === null
        ? null
        : { status: "read", branch: "feature/PAY-231-sepa-retry", head: null, upstream: null, ahead: 3, behind: 0, changed: 1, untracked: 0, conflicted: 0 },
  };
}

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;
let reads: EntityFilter[];

beforeEach(() => {
  reads = [];
  target = document.createElement("div");
  document.body.appendChild(target);
});

afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
});

function ports(over: Partial<CodePorts> = {}): CodePorts {
  return {
    listEntities: async (filter) => {
      reads.push(filter);
      if (filter.kinds.includes("repo")) return page(REPOS);
      if (filter.kinds.includes("branch")) return page(filter.reach === "jira:PAY-231" ? [BRANCH] : []);
      if (filter.order === "attention") return page(OPEN, GROUPS);
      return page([...OPEN, ...SETTLED]);
    },
    entityCheckout: async (id) => checkout(id.endsWith("payout-service") ? "/Users/mara/src/payout-service" : null),
    codeFacts: async (ids) => FACTS.filter((facts) => ids.includes(facts.entity_id)),
    listSources: async () => [{ ...sourceFacts("gitea"), base_url: "https://git.example.com" }, { ...sourceFacts("jira"), base_url: "" }],
    listContexts: async () => [],
    getEntity: async (id) => {
      if (id !== "jira:PAY-231") throw { code: "not_found" };
      return { row: { entity_id: id, kind: "ticket" } } as never;
    },
    jobReach: async (id): Promise<Reached[]> =>
      id === "jira:PAY-231"
        ? [
            { entity_id: `gitea:${PAYOUT}`, kind: "repo", step: `gitea:${PAYOUT}`, origin: { kind: "own" } as never },
            { entity_id: `gitea:${PAYOUT}#142`, kind: "pr", step: `gitea:${PAYOUT}#142`, origin: { kind: "own" } as never },
            { entity_id: BRANCH.entity_id, kind: "branch", step: BRANCH.entity_id, origin: { kind: "own" } as never },
          ]
        : [],
    ...over,
  };
}

async function draw(rest: string[], job: string | null = "PAY-231", stand: CodePorts = ports()) {
  view = mount(TfCode, { target, props: { ready: true, rest, job, ports: stand, now: NOW } });
  for (let i = 0; i < 10; i += 1) await Promise.resolve();
  await new Promise((resolve) => setTimeout(resolve, 0));
  flushSync();
}

const text = (el: Element | null) => el?.textContent?.replace(/\s+/g, " ").trim() ?? "";
const cells = (tr: Element) => [...tr.querySelectorAll("td")].map((td) => text(td));

test("#/code opens on Repositories: yours first, then by last push, with open PRs, checkout and job chip", async () => {
  await draw([]);

  expect(text(target.querySelector("h1"))).toBe("Code");
  expect(text(target.querySelector(".tf-sub"))).toBe("3 repositories on git.example.com, what asks for you first.");
  const tabs = [...target.querySelectorAll(".tf-tabs a")];
  expect(tabs.map(text)).toEqual(["Repositories 3", "Pull requests 4"]);
  expect(tabs[0]!.getAttribute("aria-current")).toBe("page");

  const rows = [...target.querySelectorAll("tbody tr")];
  expect(rows.map(cells)).toEqual([
    // A repository holding a merged pull request of yours is yours too (#966).
    ["tidewater/ledger-api", "Payments", "Kotlin", "1", "not on this machine", "yesterday 12:00", "Clone"],
    ["tidewater/payout-service PAY-231", "Payments", "Rust", "2", "feature/PAY-231-sepa-retry, 3 ahead, 1 changed", "2026-08-20", "Open in… ▾"],
    // Not on this machine: *Clone* in its place, never a disabled button (R-8.22).
    ["tidewater/fx-rates", "Platform", "Go", "0", "not on this machine", "today 11:00", "Clone"],
  ]);
  expect(rows[1]!.querySelector("a")!.getAttribute("href")).toBe("#/repo/gitea%3Atidewater%2Fpayout-service");
  expect([...target.querySelectorAll("thead th")].map(text).slice(0, 3)).toEqual(["Repository", "Team", "Language"]);
  expect(target.querySelector("input")?.getAttribute("placeholder")).toBe(
    "Filter 3 repositories by name, team or language",
  );
});

test("Repositories: the Team and Language dropdowns keep what the repositories declare (#966)", async () => {
  await draw([]);
  const select = (label: string) =>
    [...target.querySelectorAll("select")].find((el) => el.getAttribute("aria-label") === label) as HTMLSelectElement;
  const pick = (label: string, value: string) => {
    const el = select(label);
    el.value = value;
    el.dispatchEvent(new Event("change", { bubbles: true }));
    flushSync();
  };
  expect([...select("Team").options].map((option) => option.value)).toEqual(["", "Payments", "Platform"]);
  pick("Team", "Platform");
  expect([...target.querySelectorAll("tbody tr")].map((tr) => cells(tr)[0])).toEqual(["tidewater/fx-rates"]);
  pick("Team", "");
  pick("Language", "Kotlin");
  expect([...target.querySelectorAll("tbody tr")].map((tr) => cells(tr)[0])).toEqual(["tidewater/ledger-api"]);
});

test("Pull requests are grouped in the attention read's order, then merged or closed, each with its job", async () => {
  await draw(["prs"]);

  // Your merged pull request is under *Yours* (#966), so nothing is left
  // under *Merged or closed*.
  expect([...target.querySelectorAll(".tf-cgroup-h")].map(text)).toEqual([
    "Asking for your review 1",
    "Yours 2",
    "Open, others 1",
  ]);
  const rows = [...target.querySelectorAll("tbody tr")].map(cells);
  expect(rows).toEqual([
    ["#144", `Add payout CSV export${PAYOUT}, tom`, "open", "", "today 10:20"],
    ["#142", `SEPA retry with exponential backoff${PAYOUT}, mara`, "open", "PAY-231", "today 10:20"],
    ["#139", "Fix ledger drift on partial refundstidewater/ledger-api, mara", "merged", "", "today 10:20"],
    ["#77", "Bump dependenciestidewater/ledger-api, jonas, idle for more than 5 days", "open", "", "2026-08-10"],
  ]);
  expect(target.querySelector("input")?.getAttribute("placeholder")).toBe(
    "Filter pull requests by title, repo, author or number",
  );
  const attention = reads.find((filter) => filter.order === "attention");
  expect(attention?.kinds).toEqual(["pr"]);
});

test("Waiting on me keeps the review asked of you; This job with nothing held offers Search all of Code", async () => {
  await draw(["prs"]);
  const chip = [...target.querySelectorAll("button")].find((button) => text(button) === "Waiting on me")!;
  chip.click();
  flushSync();
  expect([...target.querySelectorAll("tbody tr")].map((tr) => cells(tr)[0])).toEqual(["#144"]);
  chip.click();

  const merged = [...target.querySelectorAll("button")].find((button) => text(button) === "Merged")!;
  merged.click();
  const scope = [...target.querySelectorAll("button")].find((button) => text(button).startsWith("This job"))!;
  scope.click();
  flushSync();
  expect(target.querySelectorAll("tbody tr")).toHaveLength(0);
  const all = [...target.querySelectorAll("button")].find((button) => text(button) === "Search all of Code")!;
  all.click();
  flushSync();
  expect([...target.querySelectorAll("tbody tr")].map((tr) => cells(tr)[0])).toEqual(["#139"]);
});

test("New pull request opens the compare page for the job's branch; with no job it cannot", async () => {
  await draw([]);
  const link = [...target.querySelectorAll("a")].find((a) => text(a) === "New pull request");
  expect(link?.getAttribute("href")).toBe(
    "#/repo/gitea%3Atidewater%2Fpayout-service/compare/feature%2FPAY-231-sepa-retry",
  );
  unmount(view!);
  view = null;

  await draw([], null);
  const button = [...target.querySelectorAll("button")].find((b) => text(b) === "New pull request");
  expect(button?.getAttribute("aria-disabled")).toBe("true");
  expect(target.querySelector(".tf-chip-job")).toBeNull();
});

test("Code asks every read for the code host role with its own kinds, and draws the rows as read (#1298)", async () => {
  const rocket = row("forge:acme/rocket", "repo", "acme/rocket", { source_id: "forge", updated_at: "2026-08-22T11:00:00Z" });
  const launch = row("forge:acme/rocket#1", "pr", "Launch window", {
    source_id: "forge",
    path: "acme/rocket",
    state: { text: "open", tone: "info" },
  });
  const asked: EntityFilter[] = [];
  await draw(
    [],
    "PAY-231",
    ports({
      listEntities: async (filter) => {
        asked.push(filter);
        if (filter.kinds.includes("repo")) return page([...REPOS, rocket]);
        if (filter.kinds.includes("branch")) return page([]);
        return page([...OPEN, launch]);
      },
    }),
  );

  const kinds = asked.map((filter) => `${filter.role}:${filter.kinds.join(",")}:${filter.order}`);
  expect(kinds.sort()).toEqual(["code-host:branch:updated_desc", "code-host:pr:attention", "code-host:pr:updated_desc", "code-host:repo:updated_desc"]);
  expect(target.querySelectorAll("tbody tr")).toHaveLength(REPOS.length + 1);
  expect([...target.querySelectorAll(".tf-tabs a")].map(text)[0]).toBe(`Repositories ${REPOS.length + 1}`);
});

test("a mirror of more than 1000 repositories is read on to its total, not cut at 1000 (#1825)", async () => {
  const many = Array.from({ length: 1200 }, (_, n) => row(`gitea:o/r${n}`, "repo", `o/r${n}`));
  const offsets: number[] = [];
  await draw(
    ["repos"],
    "PAY-231",
    ports({
      listEntities: async (filter, limit, offset) => {
        if (!filter.kinds.includes("repo")) return page([]);
        offsets.push(offset);
        return { rows: many.slice(offset, offset + limit), total: many.length, tally: null, groups: null };
      },
    }),
  );
  expect(offsets).toEqual([0, 1000]);
  expect(text(target.querySelector(".tf-tabs a"))).toBe("Repositories 1200");
});

const drawn = (table: Element) => table.querySelectorAll("tbody tr:not(.tf-wgap)").length;

test("a long Repositories tab draws the repositories near the viewport, spacer rows standing for the rest (#2210)", async () => {
  const many = Array.from({ length: 2000 }, (_, n) => repo(`bulk-${n}`, "2026-08-01T10:00:00Z"));
  await draw(["repos"], null, ports({ listEntities: async (filter) => page(filter.kinds.includes("repo") ? many : []) }));
  const table = target.querySelector(".tf-tbl")!;
  expect(drawn(table)).toBeGreaterThan(5);
  expect(drawn(table)).toBeLessThan(100);
  expect(table.querySelectorAll("tbody tr.tf-wgap")).toHaveLength(1);

  const field = target.querySelector<HTMLInputElement>("input[data-filter]")!;
  field.value = "bulk-1999";
  field.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  expect([...target.querySelectorAll(".tf-tbl tbody tr")].map((tr) => cells(tr)[0])).toEqual(["tidewater/bulk-1999"]);
});

test("a long Pull requests tab draws each group's pull requests near the viewport (#2210)", async () => {
  const many = Array.from({ length: 2000 }, (_, n) => pr(5000 + n, `Bulk change ${n}`, "open"));
  await draw(
    ["prs"],
    null,
    ports({
      listEntities: async (filter) => {
        if (filter.kinds.includes("repo")) return page(REPOS);
        if (filter.kinds.includes("branch")) return page([]);
        if (filter.order === "attention") return page(many, [{ group: "open_pull_requests", count: many.length }]);
        return page([...many, ...SETTLED]);
      },
    }),
  );
  const tables = [...target.querySelectorAll(".tf-cprs")];
  const rows = tables.reduce((sum, table) => sum + drawn(table), 0);
  expect(rows).toBeGreaterThan(5);
  expect(rows).toBeLessThan(100);
  expect(text(target.querySelector(".tf-tabs a:last-child"))).toBe("Pull requests 2001");

  const field = target.querySelector<HTMLInputElement>("input[data-filter]")!;
  field.value = "Bulk change 1999";
  field.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  expect([...target.querySelectorAll(".tf-cprs tbody tr")].map((tr) => cells(tr)[0])).toEqual(["#6999"]);
});

test("2,500 repositories with the later pages held back: the first 1,000 draw at once, counted at the total (#2245)", async () => {
  const many = Array.from({ length: 2500 }, (_, n) => repo(`bulk-${String(n).padStart(4, "0")}`, "2026-08-01T10:00:00Z"));
  let release!: () => void;
  const later = new Promise<void>((resolve) => (release = resolve));
  await draw(
    ["repos"],
    null,
    ports({
      listEntities: async (filter, limit, offset) => {
        if (!filter.kinds.includes("repo")) return page([]);
        if (offset > 0) await later;
        return { rows: many.slice(offset, offset + limit), total: many.length, tally: null, groups: null };
      },
    }),
  );
  const first = target.querySelector(".tf-tbl tbody tr:not(.tf-wgap)");
  expect(first).not.toBeNull();
  expect(target.querySelector(".tf-skel")).toBeNull();
  expect(cells(first!)[0]).toBe("tidewater/bulk-0000");
  // The counts are the mirror's total, not the rows in so far.
  expect(text(target.querySelector(".tf-tabs a"))).toBe("Repositories 2500");
  expect(text(target.querySelector(".tf-sub"))).toContain("2500 repositories");

  const field = target.querySelector<HTMLInputElement>("input[data-filter]")!;
  const find = (typed: string) => {
    field.value = typed;
    field.dispatchEvent(new Event("input", { bubbles: true }));
    flushSync();
    return [...target.querySelectorAll(".tf-tbl tbody tr")].map((tr) => cells(tr)[0]);
  };
  expect(find("bulk-0999")).toEqual(["tidewater/bulk-0999"]);
  expect(find("bulk-2499")).toEqual([]);
  find("");
  const before = target.querySelector(".tf-tbl tbody tr:not(.tf-wgap)");

  release();
  for (let i = 0; i < 20; i += 1) await Promise.resolve();
  await new Promise((resolve) => setTimeout(resolve, 0));
  flushSync();
  // Appended behind what was drawn: the same row node, not a list drawn anew.
  expect(target.querySelector(".tf-tbl tbody tr:not(.tf-wgap)")).toBe(before);
  expect(find("bulk-2499")).toEqual(["tidewater/bulk-2499"]);
  expect(find("bulk-0999")).toEqual(["tidewater/bulk-0999"]);
});

test("Code draws skeleton rows until its first page answers (#2246)", async () => {
  await draw(["repos"], null, ports({ listEntities: () => new Promise<EntityPage>(() => {}) }));
  expect(target.querySelectorAll(".tf-skel tbody tr").length).toBeGreaterThan(0);
  expect(target.querySelector(".tf-empty")).toBeNull();
  unmount(view!);
  view = null;
  await draw(["prs"], null, ports({ listEntities: () => new Promise<EntityPage>(() => {}) }));
  expect(target.querySelectorAll(".tf-skel tbody tr").length).toBeGreaterThan(0);
});

test("Sync now's re-read keeps the repositories drawn, dimmed, until the new list is in (#2246)", async () => {
  let hold: Promise<void> | null = null;
  const base = ports();
  const props = $state({
    ready: true,
    rest: ["repos"],
    job: null,
    now: NOW,
    syncRevision: 0,
    ports: ports({
      listEntities: async (filter, limit, offset) => {
        if (hold !== null) await hold;
        return base.listEntities(filter, limit, offset);
      },
    }),
  });
  view = mount(TfCode, { target, props });
  for (let i = 0; i < 10; i += 1) await Promise.resolve();
  flushSync();
  expect(target.querySelector(".tf-wait-dim")).toBeNull();

  let release!: () => void;
  hold = new Promise<void>((resolve) => (release = resolve));
  props.syncRevision = 1;
  flushSync();
  const dimmed = target.querySelector(".tf-wait-dim");
  expect(dimmed).not.toBeNull();
  expect(dimmed!.querySelectorAll("tbody tr").length).toBe(3);

  release();
  for (let i = 0; i < 10; i += 1) await Promise.resolve();
  await new Promise((resolve) => setTimeout(resolve, 0));
  flushSync();
  expect(target.querySelector(".tf-wait-dim")).toBeNull();
  expect(target.querySelectorAll(".tf-tbl tbody tr").length).toBe(3);
});
