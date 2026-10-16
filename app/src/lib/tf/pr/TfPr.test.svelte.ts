/**
 * The pull request page (#841, R-8.15, R-8.16, R-8.20), mounted over stand-in
 * reads: the header and its toolbar, the tabs and their counts, Overview with
 * its checks and *Ready to merge?*, Conversation and its comment box, and
 * Commits with a proposed link. What the source's view holds is
 * `knobas-source-gitea`'s (`view/pr.rs`, its tests and `gitea-live`); here
 * it is a stand-in shaped like it.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type {
  CheckoutView,
  EntityDetail,
  EntityRow,
  Fact,
  SourceView,
  SuggestionPage,
  ViewAction,
  ViewRow,
} from "../../ipc/entity";
import type { WriteOpPayload } from "../../ipc/sources";
import TfPr, { forgetPrQueued, type PrPorts } from "./TfPr.svelte";

const REPO = "gitea:tidewater/payout-service";
const PR = `${REPO}#142`;
const C1 = `${REPO}@c90d11e4a7b2`;
const C2 = `${REPO}@a41f2c08d3e9`;
const RUN = "teamcity:Payout_IntegrationTests#1187";
const RUN_URL = "https://ci.example.test/build/1187";

function row(entity_id: string, kind: string, title: string, extra: Partial<EntityRow> = {}): EntityRow {
  return {
    entity_id,
    kind,
    source_id: "gitea",
    title,
    updated_at: "2026-08-22T10:30:00Z",
    synced_at: "2026-08-22T14:30:00Z",
    path: null,
    state: null,
    ...extra,
  };
}

const fact = (label: string, value: string, tone: Fact["tone"] = null): Fact => ({ label, value, tone });
const vrow = (entity_id: string | null, title: string, chip: string | null, tone: ViewRow["tone"] = null, meta: string | null = null, url: string | null = null): ViewRow => ({
  entity_id,
  title,
  meta,
  tone,
  chip,
  url,
});

function action(write_op: string, label: string, extra: Partial<ViewAction> = {}): ViewAction {
  return {
    write_op,
    label,
    fields: [],
    confirm: null,
    primary: false,
    danger: false,
    disabled: null,
    with_settings: false,
    values: {},
    entity: null,
    line_comments: "none",
    ...extra,
  } as ViewAction;
}

const EDIT = action("edit_pull_request", "Edit…", {
  fields: [
    { name: "title", label: "Title", kind: { type: "text", suggestions: [] }, value: "SEPA retry", required: true, placeholder: null },
    { name: "body", label: "Description", kind: { type: "long_text" }, value: "Closes PAY-231.", required: false, placeholder: null },
  ],
});
const CLOSE = action("edit_pull_request", "Close", {
  values: { state: "closed" },
  danger: true,
  confirm: "Close feature/PAY-231-sepa-retry → main without merging it?",
});
const UPDATE = action("update_pull_request_branch", "Update branch…", {
  confirm: "Bring feature/PAY-231-sepa-retry up to date with main?",
  fields: [
    {
      name: "style",
      label: "How",
      kind: { type: "choice", options: [{ value: "merge", label: "Merge main in" }, { value: "rebase", label: "Rebase onto main" }] },
      value: null,
      required: true,
      placeholder: null,
    },
  ],
});
const MERGE = action("merge_pull_request", "Merge…", { primary: true, disabled: "1 of 2 required approvals" });
const COMMENT = action("comment", "Comment…");
const APPROVE = action("submit_review", "Approve…", { values: { verdict: "approve" } });

function prView(actions: ViewAction[] = [COMMENT, EDIT, CLOSE, UPDATE, MERGE]): SourceView {
  return {
    sections: [
      {
        id: "overview",
        title: "Overview",
        count: null,
        deferred: false,
        blocks: [
          {
            type: "facts",
            facts: [
              fact("State", "open", "info"),
              fact("Author", "mara"),
              fact("Branches", "feature/PAY-231-sepa-retry → main"),
              fact("Mergeable", "no: 1 of 2 required approvals", "warn"),
              fact("Review", "approved by priya", "ok"),
            ],
          },
          {
            type: "entities",
            title: "Checks",
            rows: [
              vrow(null, "Payout_IntegrationTests #1187", "failure", "err", "41 passed, 1 failed", RUN_URL),
              vrow(null, "Payout_Lint #904", "success", "ok", null, "https://ci.example.test/build/904"),
            ],
          },
          { type: "text", text: "Retries SEPA payouts.", markdown: true },
        ],
      },
      { id: "files", title: "Files", count: 3, deferred: true, blocks: [] },
      {
        id: "commits",
        title: "Commits",
        count: 2,
        deferred: false,
        blocks: [
          {
            type: "entities",
            title: null,
            rows: [vrow(C1, "PAY-231: jitter in backoff", "c90d11e4a7", null, "mara · 2h"), vrow(C2, "PAY-231 backoff jitter", "a41f2c08d3", null, "mara · 6h")],
          },
        ],
      },
      {
        id: "conversation",
        title: "Conversation",
        count: 2,
        deferred: false,
        blocks: [
          { type: "facts", facts: [fact("jonas", "2026-08-22 09:30"), fact("Review", "commented")] },
          { type: "text", text: "Should the jitter be bounded?", markdown: true },
          { type: "facts", facts: [fact("mara", "2026-08-22 10:20")] },
          { type: "text", text: "Bounded to ±10 %.", markdown: true },
        ],
      },
    ],
    actions,
  };
}

function detail(author: string): EntityDetail {
  return {
    row: row(PR, "pr", "SEPA retry with exponential backoff", { path: "tidewater/payout-service", state: { text: "open", tone: "info" } }),
    author,
  } as unknown as EntityDetail;
}

const CLONED: CheckoutView = {
  repo_entity_id: REPO,
  repo_url: null,
  path: "/Users/mara/code/payout-service",
  found_by: "scan",
  clones_root: "/Users/mara/code",
  clone_command: null,
  tree: { status: "read", branch: "feature/PAY-231-sepa-retry", head: null, upstream: null, ahead: 0, behind: 0, changed: 1, untracked: 0, conflicted: 0 },
};

const SUGGESTIONS: SuggestionPage = {
  rows: [
    {
      link: { id: "link:7", from_id: C2, to_id: "jira:PAY-231" } as SuggestionPage["rows"][number]["link"],
      from: { entity_id: C2, kind: "commit", title: "PAY-231 backoff jitter", deleted_at: null },
      to: { entity_id: "jira:PAY-231", kind: "ticket", title: "Retry", deleted_at: null },
    },
  ],
  total: 1,
  more: [],
};

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;
let writes: WriteOpPayload[];

beforeEach(() => {
  forgetPrQueued();
  writes = [];
  target = document.createElement("div");
  document.body.appendChild(target);
});

afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
  location.hash = "";
});

function ports(over: Partial<PrPorts> & { pr?: SourceView; author?: string; checkout?: CheckoutView } = {}): PrPorts {
  return {
    getEntity: async () => detail(over.author ?? "mara"),
    entityView: async () => over.pr ?? prView(),
    listEntities: async () => ({ rows: [row(REPO, "repo", "tidewater/payout-service"), row("gitea:tidewater/ledger", "repo", "tidewater/ledger")], total: 2 }) as never,
    entityCheckout: async () => over.checkout ?? CLONED,
    roomSuggestions: async () => SUGGESTIONS,
    resolveUrl: async (url) => (url === RUN_URL ? { entity_id: RUN, kind: "build" } : null),
    listSources: async () => [{ id: "gitea", config: { username: "mara" } }] as never,
    submitWrite: async (payload) => {
      writes.push(payload);
      return {} as never;
    },
    ...over,
  };
}

async function draw(rest: string[], with_: PrPorts = ports()) {
  view = mount(TfPr, { target, props: { ready: true, rest, job: "PAY-231", ports: with_ } });
  await settle();
}

async function settle() {
  for (let n = 0; n < 6; n += 1) {
    await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 0));
  }
  flushSync();
}

const text = (selector: string) => target.querySelector(selector)?.textContent?.replace(/\s+/g, " ").trim() ?? "";
const button = (name: string) =>
  [...target.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent?.trim() === name) ??
  [...document.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent?.trim() === name);
const cells = (tr: Element) => [...tr.querySelectorAll("td")].map((td) => td.textContent?.replace(/\s+/g, " ").trim()).join(" | ");
const buttons = () => [...target.querySelectorAll(".tf-fhead .tf-acts button")].map((b) => b.textContent?.trim());

test("the header names the pull request, its state, head into base and the reader as its author", async () => {
  await draw([PR]);
  expect(text("h1")).toBe("PR #142 SEPA retry with exponential backoff");
  expect(text(".tf-fhead .tf-sub")).toBe("open feature/PAY-231-sepa-retry into main, by you");
  const crumbs = [...target.querySelectorAll(".tf-crumbs a")].map((a) => [a.textContent, a.getAttribute("href")]);
  expect(crumbs).toEqual([
    ["PAY-231", "#/job/PAY-231"],
    ["Code", "#/code"],
    ["tidewater/payout-service", `#/repo/${encodeURIComponent(REPO)}`],
  ]);
  expect(text(".tf-crumbs strong")).toBe("PR #142");
});

test("somebody else's pull request names its author and offers Review…", async () => {
  const theirs = prView([COMMENT, APPROVE, EDIT, MERGE]);
  const facts = theirs.sections[0]!.blocks[0]!;
  if (facts.type === "facts") facts.facts[1] = fact("Author", "jonas");
  await draw([PR], ports({ author: "jonas", pr: theirs }));
  expect(text(".tf-fhead .tf-sub")).toBe("open feature/PAY-231-sepa-retry into main, by jonas");
  expect(buttons()).toEqual(["Review…", "Edit", "Merge…"]);
});

test("the toolbar draws what the source offers, in prototype-6's order: Review…, Update branch, Edit, Close, Merge…", async () => {
  await draw([PR], ports({ pr: prView([APPROVE, COMMENT, EDIT, CLOSE, UPDATE, MERGE]) }));
  expect(buttons()).toEqual(["Review…", "Update branch", "Edit", "Close", "Merge…"]);
  // Merge… is #843's dialog; until it is built it says why it waits.
  const merge = button("Merge…")!;
  expect(merge.getAttribute("aria-disabled")).toBe("true");
  expect(merge.title).toBe("1 of 2 required approvals");
});

test("a pull request whose head is not behind offers no Update branch; a merged one no Close or Merge…", async () => {
  await draw([PR], ports({ pr: prView([COMMENT, EDIT]) }));
  expect(buttons()).toEqual(["Edit"]);
});

test("the tabs carry the sections' counts and light the one the address names", async () => {
  await draw([PR, "commits"]);
  const tabs = [...target.querySelectorAll(".tf-tabs a")];
  expect(tabs.map((a) => a.textContent?.replace(/\s+/g, " ").trim())).toEqual(["Overview", "Conversation 2", "Files changed 3", "Commits 2"]);
  expect(tabs.map((a) => a.getAttribute("href"))).toEqual([
    `#/pr/${encodeURIComponent(PR)}`,
    `#/pr/${encodeURIComponent(PR)}/conversation`,
    `#/pr/${encodeURIComponent(PR)}/diff`,
    `#/pr/${encodeURIComponent(PR)}/commits`,
  ]);
  expect(tabs.map((a) => a.getAttribute("aria-current"))).toEqual([null, null, null, "page"]);
});

test("Overview: each check with its state, the one the mirror holds opening its run, then the description", async () => {
  await draw([PR]);
  const checks = [...target.querySelectorAll(".tf-prchecks tr")];
  expect(checks.map(cells)).toEqual([
    "Payout_IntegrationTests #1187 | failure, 41 passed, 1 failed",
    "Payout_Lint #904 | success",
  ]);
  expect(checks[0]!.querySelector(".tf-dot")!.classList.contains("tf-err")).toBe(true);
  expect(checks[0]!.querySelector("a")!.getAttribute("href")).toBe(`#/build/${encodeURIComponent(RUN)}`);
  // A link the mirror does not hold opens nothing.
  expect(checks[1]!.querySelector("a")).toBeNull();
  expect(text(".tf-prdesc")).toBe("Retries SEPA payouts.");
});

test("Ready to merge? lists the source's verdict, the reviews and the checks, each with its dot; the clone beside it", async () => {
  await draw([PR]);
  const lines = [...target.querySelectorAll(".tf-prready > span")];
  expect(lines.map((line) => line.textContent?.trim())).toEqual([
    "Mergeable: no: 1 of 2 required approvals",
    "Approved by priya",
    "Checks: Payout_IntegrationTests #1187 failure",
  ]);
  expect(lines.map((line) => line.querySelector(".tf-dot")!.className)).toEqual(["tf-dot tf-warn", "tf-dot tf-ok", "tf-dot tf-err"]);
  expect(text(".tf-prlocal")).toBe("/Users/mara/code/payout-service On this branch, 1 changed file Open in… Local changes");
  expect(target.querySelector(".tf-prlocal a")!.getAttribute("href")).toBe(`#/repo/${encodeURIComponent(REPO)}/local`);
});

test("a repository not on this machine draws no Local checkout", async () => {
  await draw([PR], ports({ checkout: { ...CLONED, path: null, tree: null } }));
  expect(target.querySelector(".tf-prlocal")).toBeNull();
  expect(target.querySelector(".tf-prready")).not.toBeNull();
});

test("Edit sends only what changed, and the header says an edit is queued", async () => {
  await draw([PR]);
  button("Edit")!.click();
  flushSync();
  const title = document.querySelector<HTMLInputElement>(".tf-pra input[type=text]")!;
  expect(title.value).toBe("SEPA retry");
  title.value = "SEPA retry with backoff";
  title.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  [...document.querySelectorAll<HTMLButtonElement>(".tf-dialog footer button")].find((b) => b.textContent?.trim() === "Edit")!.click();
  await settle();
  expect(writes).toEqual([{ EditPullRequest: { entity: PR, title: "SEPA retry with backoff", body: null, state: null } }]);
  expect(document.querySelector(".tf-dialog")).toBeNull();
  expect(text(".tf-fhead .tf-sub")).toContain("edit queued");
});

test("Close asks the source's sentence and queues the closed state", async () => {
  await draw([PR]);
  button("Close")!.click();
  flushSync();
  expect(document.querySelector(".tf-dialog")!.textContent).toContain("Close feature/PAY-231-sepa-retry → main without merging it?");
  [...document.querySelectorAll<HTMLButtonElement>(".tf-dialog footer button")].find((b) => b.textContent?.trim() === "Close")!.click();
  await settle();
  expect(writes).toEqual([{ EditPullRequest: { entity: PR, title: null, body: null, state: "closed" } }]);
  expect(text(".tf-fhead .tf-sub")).toContain("close queued");
});

test("Update branch queues the style picked", async () => {
  await draw([PR]);
  button("Update branch")!.click();
  flushSync();
  const rebase = document.querySelector<HTMLInputElement>(".tf-pra input[value=rebase]")!;
  rebase.click();
  flushSync();
  [...document.querySelectorAll<HTMLButtonElement>(".tf-dialog footer button")].find((b) => b.textContent?.trim() === "Update branch")!.click();
  await settle();
  expect(writes).toEqual([{ UpdatePullRequestBranch: { entity: PR, style: "rebase" } }]);
});

test("Conversation: each entry's who, when and verdict, then a comment queued and drawn at once", async () => {
  await draw([PR, "conversation"]);
  const messages = () => [...target.querySelectorAll(".tf-convo .tf-msg")].map((m) => m.textContent?.replace(/\s+/g, " ").trim());
  expect(messages()).toEqual(["JO jonas2026-08-22 09:30 commented Should the jitter be bounded?", "MA You2026-08-22 10:20 Bounded to ±10 %."]);
  const box = target.querySelector<HTMLTextAreaElement>(".tf-compose textarea")!;
  expect(box.placeholder).toBe("Comment on PR #142");
  box.value = "Thanks, looks good.";
  box.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  button("Comment")!.click();
  await settle();
  expect(writes).toEqual([{ Comment: { entity: PR, body: "Thanks, looks good." } }]);
  expect(messages()[2]).toBe("MA Youqueued Thanks, looks good.");
  expect(box.value).toBe("");
});

test("a source that offers no comment draws no comment box", async () => {
  await draw([PR, "conversation"], ports({ pr: prView([EDIT]) }));
  expect(target.querySelector(".tf-compose")).toBeNull();
});

test("Commits: each opens its diff on the repository page; one with a proposed link says judge it", async () => {
  await draw([PR, "commits"]);
  const rows = [...target.querySelectorAll(".tf-rcommits tr")];
  expect(rows.map(cells)).toEqual([
    "c90d11e4a7 | PAY-231: jitter in backoff | mara · 2h",
    "a41f2c08d3 | PAY-231 backoff jitter proposed link, judge it | mara · 6h",
  ]);
  expect(rows[0]!.querySelector("a")!.getAttribute("href")).toBe(`#/repo/${encodeURIComponent(REPO)}/commits/${encodeURIComponent(C1)}`);
  expect(rows[1]!.querySelector("a.tf-chip")!.getAttribute("href")).toBe("#/review/link%3A7");
  (rows[0]!.querySelector("td:nth-child(2)") as HTMLElement).click();
  expect(location.hash).toBe(`#/repo/${encodeURIComponent(REPO)}/commits/${encodeURIComponent(C1)}`);
});

test("Files changed is #842's: an empty page that says so, with no side column", async () => {
  await draw([PR, "diff"]);
  expect(text(".tf-cbody")).toBe("Files changed is not built in the new shell yet.");
  expect(target.querySelector(".tf-side")).toBeNull();
});

test("a pull request the mirror does not hold says why", async () => {
  await draw([PR], ports({ getEntity: async () => Promise.reject({ code: "not_found", message: "no such entity" }) }));
  expect(text(".tf-cfail")).toContain("no such entity");
  vi.restoreAllMocks();
});
