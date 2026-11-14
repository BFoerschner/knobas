/**
 * A repository's Builds tab (the repo-builds pass, `mockups/builds/repo-builds.md`): the branches as a rail, and the
 * branch shown against the default branch, failures only it has first.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { RepoPipelineRow, RunRow } from "../../ipc/entity";
import TfRepoBuilds from "./TfRepoBuilds.svelte";
import type { BranchOption } from "./repo";

let target: HTMLElement;
let page: ReturnType<typeof mount> | null = null;

beforeEach(() => {
  target = document.createElement("div");
  document.body.appendChild(target);
});

afterEach(() => {
  if (page) unmount(page);
  page = null;
  target.remove();
});

async function settle() {
  for (let turn = 0; turn < 6; turn += 1) await new Promise((resolve) => setTimeout(resolve, 0));
  flushSync();
}

function run(id: string, branch: string, tone: "ok" | "err" | "run", at: string): RunRow {
  return {
    entity_id: `tc:build:${id}`,
    number: id,
    state: tone === "run" ? { text: "running", tone: "info" } : { text: tone === "ok" ? "SUCCESS" : "FAILURE", tone },
    branch,
    started: at,
    finished: tone === "run" ? null : at,
    duration_seconds: null,
    updated_at: at,
    web_url: null,
    trigger: null,
    started_by: null,
    started_by_me: false,
    revision: null,
    commit_id: null,
    agent: null,
    current_step: null,
    tests: tone === "err" ? { failed: 3, passed: 41, ignored: 0 } : null,
  };
}

const pipeline = (name: string, latest: RunRow[]): RepoPipelineRow => ({
  entity_id: `tc:buildType:${name}`,
  source_id: "tc",
  source_name: "TeamCity",
  name,
  group: "Payouts",
  path: ["Payments", "Payouts"],
  web_url: null,
  latest,
});

const ROWS = [
  pipeline("Payout_Build", [run("1188", "feat/PAY-231-retry", "run", "2026-08-22T11:45:00Z"), run("1176", "main", "ok", "2026-08-22T09:12:00Z")]),
  pipeline("Payout_IntegrationTests", [run("1187", "feat/PAY-231-retry", "err", "2026-08-22T10:09:00Z"), run("1171", "main", "ok", "2026-08-22T09:20:00Z")]),
  pipeline("Payout_Deploy_Prod", [run("90", "main", "ok", "2026-08-22T09:40:00Z")]),
  pipeline("Payout_Lint", [run("904", "spike/x", "err", "2026-08-21T10:00:00Z"), run("903", "main", "ok", "2026-08-22T09:00:00Z")]),
];

const option = (name: string, isDefault = false): BranchOption => ({ name, entityId: `git:${name}`, isDefault });

function draw(
  props: {
    branch?: string | null;
    rows?: RepoPipelineRow[];
    pipeline?: string | null;
    pulls?: { branch: string; entity_id: string; cell: string | null }[];
    branches?: BranchOption[];
    runs?: () => Promise<RunRow[]>;
  } = {},
) {
  page = mount(TfRepoBuilds, {
    target,
    props: {
      repoId: "git:repo:tidewater/payout-service",
      branches: props.branches ?? [option("main", true), option("feat/PAY-231-retry"), option("spike/x"), option("chore/quiet")],
      defaultBranch: "main",
      mine: ["feat/PAY-231-retry"],
      job: "PAY-231",
      branch: props.branch ?? null,
      pipeline: props.pipeline ?? null,
      builds: { pipelines: props.rows ?? ROWS, pull_requests: props.pulls ?? [] },
      runs:
        props.runs ??
        (() =>
          Promise.resolve([
          run("1187", "feat/PAY-231-retry", "err", "2026-08-22T10:09:00Z"),
          run("1171", "main", "ok", "2026-08-22T09:20:00Z"),
            run("77", "elsewhere/x", "ok", "2026-08-22T08:00:00Z"),
          ])),
    },
  });
}

const rail = () =>
  [...target.querySelectorAll<HTMLElement>(".tf-rbrail h3, .tf-rbrail .tf-rbr")].map((el) =>
    el.tagName === "H3" ? `# ${el.textContent}` : `${el.querySelector(".tf-pi-nm")?.textContent}${el.getAttribute("aria-current") === "true" ? " *" : ""}`,
  );

test("the rail: the default branch, yours, then the rest by newest run, and it opens on yours", async () => {
  draw();
  await settle();
  expect(rail()).toEqual(["# Default branch", "main", "# Yours", "feat/PAY-231-retry *", "# Other branches", "spike/x", "chore/quiet"]);
  const mine = target.querySelector<HTMLAnchorElement>('.tf-rbr[aria-current="true"]')!;
  expect(mine.querySelector(".tf-rg")?.getAttribute("aria-label")).toBe("Failed");
  expect(mine.querySelector(".tf-pi-when")?.textContent).toBe("1 failing");
  expect(mine.getAttribute("href")).toBe("#/repo/git%3Arepo%3Atidewater%2Fpayout-service/builds/feat%2FPAY-231-retry");
});

test("a branch's page leads with what fails only there, as a problem with its log, then the rest beside main", async () => {
  draw();
  await settle();
  const problems = [...target.querySelectorAll(".tf-rbp .tf-problem h3")].map((h) => h.textContent);
  expect(problems).toEqual(["Payout_IntegrationTests fails here and passes on main"]);
  const log = target.querySelector<HTMLAnchorElement>(".tf-rbp .tf-problem a")!;
  expect(log.textContent).toBe("Log");
  expect(log.getAttribute("href")).toBe("#/build/tc%3AbuildType%3APayout_IntegrationTests/tc%3Abuild%3A1187/log");
  const table = [...target.querySelectorAll(".tf-rbt tbody tr")].map((tr) =>
    [tr.querySelector(".tf-pi-nm")?.textContent, tr.querySelector(".tf-rbt-v")?.textContent?.trim()].join(" | "),
  );
  expect(table).toEqual(["Payout_IntegrationTests | Only on this branch", "Payout_Build | Running"]);
  expect(target.querySelector(".tf-rb-notrun")?.textContent).toContain("Not run on this branch: Payout_Deploy_Prod, Payout_Lint.");
});

test("on the default branch the page has no comparison, and says when nothing fails", async () => {
  draw({ branch: "main" });
  await settle();
  expect([...target.querySelectorAll(".tf-rbt thead th")].map((th) => th.textContent?.trim())).toEqual(["Pipeline", "main", "Verdict"]);
  expect(target.querySelector(".tf-rbp .tf-problem")).toBeNull();
  expect(target.querySelector(".tf-rbp-ok")?.textContent).toContain("Every pipeline that ran on main passed");
});

test("Failing keeps the branches with a failure, and the filter narrows by name", async () => {
  draw();
  await settle();
  target.querySelector<HTMLButtonElement>('.tf-rbrail button[aria-pressed]')!.click();
  flushSync();
  expect(rail()).toEqual(["# Yours", "feat/PAY-231-retry *", "# Other branches", "spike/x"]);
  const field = target.querySelector<HTMLInputElement>('.tf-rbrail input')!;
  field.value = "spike";
  field.dispatchEvent(new Event("input"));
  flushSync();
  expect(rail()).toEqual(["# Other branches", "spike/x"]);
});

test("a repository nothing has built says what would fill the tab", async () => {
  draw({ rows: [] });
  await settle();
  expect(target.querySelector(".tf-rbv")).toBeNull();
  expect(target.textContent).toContain("No pipeline has built a commit of this repository yet");
});

test("Mine keeps your branches, and the default branch is not one", async () => {
  draw();
  await settle();
  const mine = [...target.querySelectorAll<HTMLButtonElement>(".tf-rbrail .tf-fchips button")].find((b) => b.textContent === "Mine")!;
  mine.click();
  flushSync();
  expect(rail()).toEqual(["# Yours", "feat/PAY-231-retry *"]);
  expect(target.querySelector(".tf-rb-count")?.textContent).toBe("1 of 4");
});

test("a branch an open pull request is opened from is in Pull requests, its key on the row", async () => {
  draw({ pulls: [{ branch: "spike/x", entity_id: "git:pr:142", cell: "#142" }] });
  await settle();
  expect(rail()).toEqual(["# Default branch", "main", "# Yours", "feat/PAY-231-retry *", "# Pull requests", "spike/x", "# Other branches", "chore/quiet"]);
});

test("a pipeline opens inside the tab: its runs on every branch of the repository, or the default branch's", async () => {
  draw({ pipeline: "tc:buildType:Payout_IntegrationTests" });
  await settle();
  expect(target.querySelector(".tf-rbp h2")?.textContent).toBe("Payout_IntegrationTests");
  const runs = () => [...target.querySelectorAll(".tf-rbruns tbody tr")].map((tr) => tr.querySelector(".tf-rbt-br")?.textContent?.trim());
  // A run on a branch this repository does not have is another repository's.
  expect(runs()).toEqual(["feat/PAY-231-retry PAY-231", "main"]);
  [...target.querySelectorAll<HTMLButtonElement>('.tf-seg[aria-label="Runs on"] button')].find((b) => b.textContent === "main")!.click();
  flushSync();
  expect(runs()).toEqual(["main"]);
  expect(target.querySelector<HTMLAnchorElement>(".tf-rb-back a")?.getAttribute("href")).toBe(
    "#/repo/git%3Arepo%3Atidewater%2Fpayout-service/builds/feat%2FPAY-231-retry",
  );
});

/** A repository of thousands of branches (the extremes pass, 2026-11-13): the rail is a windowed list. */
const many = (n: number) => [option("main", true), option("feat/PAY-231-retry"), ...Array.from({ length: n }, (_, k) => option(`many/b-${String(k).padStart(4, "0")}`))];

test("a rail of thousands of branches draws the rows near its scroll, its headings among them, and the filter still finds any", async () => {
  draw({ branches: many(5000) });
  await settle();
  const drawn = rail();
  // The groups lead as before; the rows past the window are room, not rows.
  expect(drawn.slice(0, 5)).toEqual(["# Default branch", "main", "# Yours", "feat/PAY-231-retry *", "# Other branches"]);
  expect(drawn.length).toBeGreaterThan(20);
  expect(drawn.length).toBeLessThan(300);
  // The 5,002 listed, and the one a run names that the list does not (`spike/x`).
  expect(target.querySelector(".tf-rb-count")?.textContent).toBe("5003 branches");
  // Each branch not drawn holds a drawn branch's 26 px, as the Builds page's rail does, so the scrollbar and a scroll
  // land where every row drawn would (the WSL block, 2026-11-14: at 30 px the rail of 1,009 was 30,221 px for 26,321).
  const undrawn = 5003 + 3 - drawn.length;
  const room = parseFloat(target.querySelector<HTMLElement>(".tf-rbrail .tf-pl-rows")!.style.paddingBottom);
  expect(room / undrawn).toBe(26);
  // One far down the list is a filter away.
  const box = target.querySelector<HTMLInputElement>('input[aria-label="Filter branches"]')!;
  box.value = "b-4321";
  box.dispatchEvent(new Event("input"));
  await settle();
  expect(rail()).toEqual(["# Other branches", "many/b-4321"]);
  box.value = "no such branch";
  box.dispatchEvent(new Event("input"));
  await settle();
  expect(target.querySelector(".tf-rbrail .tf-pl-note")?.textContent).toBe("No branch matches.");
});

test("a pipeline's thousands of runs on the repository draw the rows near the view", async () => {
  const runs = Array.from({ length: 3000 }, (_, k) => run(String(5000 - k), "main", "ok", "2026-08-22T09:20:00Z"));
  draw({ pipeline: "tc:buildType:Payout_IntegrationTests", runs: () => Promise.resolve(runs) });
  await settle();
  const rows = target.querySelectorAll(".tf-rbruns tbody tr:not(.tf-wgap)");
  expect(rows.length).toBeGreaterThan(10);
  expect(rows.length).toBeLessThan(300);
  expect(rows[0]?.querySelector("a")?.textContent).toBe("#5000");
  expect(target.querySelector(".tf-rbruns tbody tr.tf-wgap")).not.toBeNull();
});

