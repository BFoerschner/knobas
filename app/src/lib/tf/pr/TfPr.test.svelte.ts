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
import { createPendingComments, type PendingCommentStore } from "../../detail/view/pending-comments.svelte";
import { health } from "../../shell/health.svelte";
import { toasts } from "../../shell/toasts.svelte";
import type { Hear, WriteFollower } from "../../shell/follow-write";
import { forgetQueued } from "../queued.svelte";
import TfPr, { type PrPorts } from "./TfPr.svelte";

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

function prView(actions: ViewAction[] = [COMMENT, EDIT, CLOSE, UPDATE, MERGE], state = "open"): SourceView {
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
              fact("State", state, state === "open" ? "info" : "muted"),
              fact("Author", "mara"),
              fact("Branches", "feature/PAY-231-sepa-retry → main"),
              fact("Mergeable", "no: 1 of 2 required approvals", "warn"),
              fact("Approvals", "1 of 2", "warn"),
              fact("Conflicts", "none with main", "ok"),
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
    links: [],
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
let pending: PendingCommentStore;

beforeEach(() => {
  writes = [];
  pending = createPendingComments(() => null);
  target = document.createElement("div");
  document.body.appendChild(target);
});

/** What a page queued is held for the session (#1424), and each test is one. */
afterEach(() => {
  forgetQueued();
  health.replace([]);
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
    prViewedFiles: async () => [],
    setPrFileViewed: async () => [],
    ...over,
  };
}

async function draw(rest: string[], with_: PrPorts = ports(), follower?: WriteFollower) {
  view = mount(TfPr, { target, props: { ready: true, rest, job: "PAY-231", ports: with_, pending, ...(follower ? { follower } : {}) } });
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
  // Blocked by the source, Merge… says so (#1441) and its tooltip says why; it still opens its dialog with the reasons (#843, below).
  const merge = button("Merge…")!;
  expect(merge.getAttribute("aria-disabled")).toBe("true");
  expect(merge.title).toBe("1 of 2 required approvals");
  expect(merge.classList.contains("tf-primary")).toBe(true);
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

test("Ready to merge? reads prototype-6's three lines, approvals, checks and conflicts, each with its dot; the clone beside it", async () => {
  await draw([PR]);
  const lines = [...target.querySelectorAll(".tf-prready > span")];
  expect(lines.map((line) => line.textContent?.trim())).toEqual([
    "Approvals: 1 of 2",
    "Checks: Payout_IntegrationTests #1187 failure",
    "No conflicts with main",
  ]);
  expect(lines.map((line) => line.querySelector(".tf-dot")!.className)).toEqual(["tf-dot tf-warn", "tf-dot tf-err", "tf-dot tf-ok"]);
  expect(text(".tf-prlocal")).toBe("/Users/mara/code/payout-service On this branch, 1 changed file Open in… ▾ Local changes");
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

test("a source action draws the dialog its source declares and queues SourceAction without the shell knowing the op (#1194)", async () => {
  // An action no part of this shell has heard of: its id, its fields and its
  // words are the source's, and the write it queues carries them as given.
  const stamp = action("source_action", "Stamp release…", {
    fields: [
      { name: "channel", label: "Channel", kind: { type: "choice", options: [{ value: "beta", label: "Beta" }, { value: "stable", label: "Stable" }] }, value: null, required: true, placeholder: null },
      { name: "note", label: "Note", kind: { type: "text", suggestions: [] }, value: null, required: false, placeholder: null },
    ],
    confirm: "Stamp this pull request for a release?",
    values: { id: "stamp_release" },
  });
  await draw([PR], ports({ pr: prView([EDIT, stamp, MERGE]) }));
  expect(buttons()).toEqual(["Edit", "Stamp release", "Merge…"]);
  button("Stamp release")!.click();
  flushSync();
  const dialog = document.querySelector(".tf-dialog")!;
  expect(dialog.textContent).toContain("Stamp this pull request for a release?");
  document.querySelector<HTMLInputElement>(".tf-pra input[value=stable]")!.click();
  const note = document.querySelector<HTMLInputElement>(".tf-pra input[type=text]")!;
  note.value = "for 2.0";
  note.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  [...document.querySelectorAll<HTMLButtonElement>(".tf-dialog footer button")].find((b) => b.textContent?.trim() === "Stamp release")!.click();
  await settle();
  expect(writes).toEqual([{ SourceAction: { entity: PR, id: "stamp_release", fields: { channel: "stable", note: "for 2.0" } } }]);
  expect(document.querySelector(".tf-dialog")).toBeNull();
  expect(text(".tf-fhead .tf-sub")).toContain("stamp release queued");
});

test("a source action the source draws disabled says why and opens nothing", async () => {
  const none = action("source_action", "Add label…", { values: { id: "add_label" }, disabled: "This repository has no labels." });
  await draw([PR], ports({ pr: prView([none]) }));
  const add = button("Add label")!;
  expect(add.getAttribute("aria-disabled")).toBe("true");
  expect(add.title).toBe("This repository has no labels.");
  add.click();
  flushSync();
  expect(document.querySelector(".tf-dialog")).toBeNull();
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
  expect(toasts.items.at(-1)!.text).toBe("Queued: comment on PR #142.");
  expect(toasts.items.at(-1)!.action?.label).toBe("Show queue");
});

/** The view with one section's blocks replaced, as a source that wrote something else there would send it. */
function withBlocks(id: string, blocks: SourceView["sections"][number]["blocks"], from: SourceView = prView()): SourceView {
  return { ...from, sections: from.sections.map((section) => (section.id === id ? { ...section, blocks } : section)) };
}

/** A muted line, as Gitea's `view::muted_line` writes one: an untitled list of one muted row with no entity (#2065). */
const mutedLine = (title: string) => ({ type: "entities" as const, title: null, rows: [vrow(null, title, null, "muted")] });

const UNREAD = "This token cannot read this pull request's comments: it lacks the read:issue scope";

test("Conversation opens on the source's line where it cannot read the comments, the reviews under it (#2065)", async () => {
  const pr = withBlocks("conversation", [
    mutedLine(UNREAD),
    { type: "facts", facts: [fact("priya.nair", "2026-08-22 09:30"), fact("Review", "approved", "ok")] },
  ]);
  await draw([PR, "conversation"], ports({ pr }));
  const convo = [...target.querySelectorAll(".tf-convo > *")].map((node) => node.textContent?.replace(/\s+/g, " ").trim());
  expect(convo[0]).toBe(UNREAD);
  expect(convo[1]).toBe("PN priya.nair2026-08-22 09:30 approved");
  expect(target.querySelector(".tf-convo .tf-prline")!.classList.contains("tf-dim")).toBe(true);
});

test("with no reviews either, Conversation is the source's line and never says No comments yet. (#2065)", async () => {
  await draw([PR, "conversation"], ports({ pr: withBlocks("conversation", [mutedLine(UNREAD)]) }));
  expect(text(".tf-convo .tf-prline")).toBe(UNREAD);
  expect(target.querySelector(".tf-convo .tf-msg")).toBeNull();
  expect(target.querySelector(".tf-convo .tf-empty")).toBeNull();
});

test("a muted line in Overview is drawn as a line where the source put it, never as a check (#2065)", async () => {
  const [facts, checks, description] = prView().sections[0]!.blocks;
  const unread = "Checks could not be read: gitea: 403 Forbidden";
  await draw([PR], ports({ pr: withBlocks("overview", [facts!, mutedLine(unread), description!]) }));
  expect(target.querySelector(".tf-prchecks")).toBeNull();
  const body = [...target.querySelectorAll(".tf-prline, .tf-cgroup-h, .tf-prdesc")].map((node) => node.textContent?.trim());
  expect(body).toEqual([unread, "Description", "Retries SEPA payouts."]);
  // Where the checks could not be read, *Ready to merge?* says so in their place.
  expect([...target.querySelectorAll(".tf-prready > span")].map((line) => line.textContent?.trim())).toEqual([
    "Approvals: 1 of 2",
    unread,
    "No conflicts with main",
  ]);
  if (view) unmount(view);
  view = null;
  // A line after the checks stays a line: the checks are only the checks.
  await draw([PR], ports({ pr: withBlocks("overview", [facts!, checks!, mutedLine("Two checks are on Gitea only."), description!]) }));
  expect([...target.querySelectorAll(".tf-prchecks tr")].length).toBe(2);
  expect(text(".tf-prchecks + .tf-prline")).toBe("Two checks are on Gitea only.");
  expect([...target.querySelectorAll(".tf-prready > span")][1]!.textContent?.trim()).toBe("Checks: Payout_IntegrationTests #1187 failure");
});

test("a muted line in Commits is drawn as a line, never as a commit; the list after it is the commits (#2065)", async () => {
  const [commits] = prView().sections[2]!.blocks;
  await draw([PR, "commits"], ports({ pr: withBlocks("commits", [mutedLine("Commits signed by an unknown key are left out."), commits!]) }));
  expect(text(".tf-prline")).toBe("Commits signed by an unknown key are left out.");
  expect([...target.querySelectorAll(".tf-rcommits tr")].map(cells)).toEqual([
    "c90d11e4a7 | PAY-231: jitter in backoff | mara · 2h",
    "a41f2c08d3 | PAY-231 backoff jitter proposed link, judge it | mara · 6h",
  ]);
  if (view) unmount(view);
  view = null;
  // A tab holding only a line says it, not *No commits.*
  await draw([PR, "commits"], ports({ pr: withBlocks("commits", [mutedLine("Commits could not be read.")]) }));
  expect(text(".tf-prline")).toBe("Commits could not be read.");
  expect(target.querySelector(".tf-rcommits")).toBeNull();
  expect(target.querySelector(".tf-empty")).toBeNull();
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


test("a pull request the mirror does not hold says why", async () => {
  await draw([PR], ports({ getEntity: async () => Promise.reject({ code: "not_found", message: "no such entity" }) }));
  expect(text(".tf-cfail")).toContain("no such entity");
  vi.restoreAllMocks();
});

// -- Submit review (#842, R-8.15, R-8.18) ------------------------------------

const SUBMIT = action("submit_review", "Submit review…", { primary: true, values: { verdict: "comment" }, line_comments: "requires" });
const APPROVE_SENDS = action("submit_review", "Approve…", {
  values: { verdict: "approve" },
  line_comments: "sends",
  fields: [{ name: "body", label: "Comment", kind: { type: "long_text" }, value: null, required: false, placeholder: null }],
});
const CHANGES = action("submit_review", "Request changes…", {
  values: { verdict: "request_changes" },
  line_comments: "sends",
  fields: [{ name: "body", label: "Comment", kind: { type: "long_text" }, value: null, required: true, placeholder: null }],
});

const dialogButton = (name: string) =>
  [...document.querySelectorAll<HTMLButtonElement>(".tf-dialog footer button")].find((b) => b.textContent?.trim() === name)!;
const type = (field: HTMLInputElement | HTMLTextAreaElement, value: string) => {
  field.value = value;
  field.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
};

test("line comments waiting turn Review… into Submit review (N pending), which sends them with the verdict", async () => {
  pending.add(PR, { path: "src/sepa/retry.rs", side: "new", line: 14, body: "Bound it." });
  pending.add(PR, { path: "src/sepa/retry.rs", side: "old", line: 9, body: "Why was this 60 s?" });
  await draw([PR], ports({ author: "jonas", pr: prView([SUBMIT, COMMENT, APPROVE_SENDS, CHANGES, EDIT, MERGE]) }));
  expect(buttons()[0]).toBe("Submit review (2 pending)");
  expect(button("Submit review (2 pending)")!.classList.contains("tf-warnbtn")).toBe(true);
  button("Submit review (2 pending)")!.click();
  flushSync();
  const dialog = document.querySelector(".tf-dialog")!;
  expect(dialog.querySelector("h2")!.textContent).toBe("Submit your review of PR #142");
  const radios = [...dialog.querySelectorAll<HTMLLabelElement>(".tf-radio label")].map((l) => l.textContent?.replace(/\s+/g, " ").trim());
  expect(radios).toEqual([
    // The meaning is a `small` on its own line under the verdict.
    "CommentFeedback without a verdict",
    "ApproveCounts toward the approvals main needs",
    "Request changesBlocks the merge until you approve",
  ]);
  expect(dialog.querySelector(".tf-pane-h")!.textContent).toBe("2 line comments go with it");
  // Comment is picked first; Approve with a summary sends both comments.
  dialog.querySelector<HTMLInputElement>("input[value=approve]")!.click();
  flushSync();
  type(dialog.querySelector("textarea")!, "Looks right.");
  dialogButton("Submit review").click();
  await settle();
  expect(writes).toEqual([
    {
      SubmitReview: {
        entity: PR,
        verdict: "approve",
        body: "Looks right.",
        comments: [
          { path: "src/sepa/retry.rs", line: 14, side: "new", body: "Bound it." },
          { path: "src/sepa/retry.rs", line: 9, side: "old", body: "Why was this 60 s?" },
        ],
      },
    },
  ]);
  // The review took them: none waits, the toolbar is Review… again, the header says so.
  expect(pending.of(PR)).toEqual([]);
  expect(buttons()[0]).toBe("Review…");
  expect(text(".tf-fhead .tf-sub")).toContain("review queued");
});

test("Request changes waits for its words; Comment waits for a line comment", async () => {
  await draw([PR], ports({ author: "jonas", pr: prView([SUBMIT, COMMENT, APPROVE_SENDS, CHANGES, EDIT, MERGE]) }));
  button("Review…")!.click();
  flushSync();
  const dialog = document.querySelector(".tf-dialog")!;
  expect(dialog.querySelector<HTMLInputElement>("input[value=comment]")!.disabled).toBe(true);
  expect(dialog.querySelector<HTMLInputElement>("input[value=approve]")!.checked).toBe(true);
  dialog.querySelector<HTMLInputElement>("input[value=request_changes]")!.click();
  flushSync();
  expect(dialogButton("Submit review").disabled).toBe(true);
  type(dialog.querySelector("textarea")!, "Bound the jitter.");
  expect(dialogButton("Submit review").disabled).toBe(false);
  dialogButton("Submit review").click();
  await settle();
  expect(writes).toEqual([{ SubmitReview: { entity: PR, verdict: "request_changes", body: "Bound the jitter.", comments: [] } }]);
});

test("a token that may not comment or review draws Comment and each verdict disabled saying why, and queues nothing (#2030)", async () => {
  const issue = "This token cannot write: it lacks the write:issue scope";
  const repository = "This token cannot write: it lacks the write:repository scope";
  const refused = (one: ViewAction, why: string) => ({ ...one, disabled: why });
  await draw(
    [PR, "conversation"],
    ports({ author: "jonas", pr: prView([refused(SUBMIT, repository), refused(COMMENT, issue), refused(APPROVE_SENDS, repository), refused(CHANGES, repository), EDIT]) }),
  );
  type(target.querySelector<HTMLTextAreaElement>(".tf-compose textarea")!, "Thanks, looks good.");
  const send = [...target.querySelectorAll<HTMLButtonElement>(".tf-compose button")].find((b) => b.textContent?.trim() === "Comment")!;
  expect(send.getAttribute("aria-disabled")).toBe("true");
  expect(send.title).toBe(issue);
  send.click();
  await settle();
  expect(target.querySelector<HTMLTextAreaElement>(".tf-compose textarea")!.value).toBe("Thanks, looks good.");

  button("Review…")!.click();
  flushSync();
  const dialog = document.querySelector(".tf-dialog")!;
  for (const verdict of ["comment", "approve", "request_changes"]) {
    const radio = dialog.querySelector<HTMLInputElement>(`input[value=${verdict}]`)!;
    expect(radio.disabled).toBe(true);
    expect(radio.closest("label")!.title).toBe(repository);
  }
  expect(dialogButton("Submit review").disabled).toBe(true);
  expect(writes).toEqual([]);
});

test("one's own pull request: no Review…, but Submit review once a line comment waits, Comment its only verdict", async () => {
  await draw([PR], ports({ pr: prView([SUBMIT, COMMENT, EDIT, MERGE]) }));
  expect(buttons()).toEqual(["Edit", "Merge…"]);
  if (view) unmount(view);
  pending.add(PR, { path: "docs/backoff.md", side: "new", line: 3, body: "Say ±10 %." });
  await draw([PR], ports({ pr: prView([SUBMIT, COMMENT, EDIT, MERGE]) }));
  button("Submit review (1 pending)")!.click();
  flushSync();
  const verdicts = [...document.querySelectorAll<HTMLInputElement>(".tf-dialog .tf-radio input")].map((r) => r.value);
  expect(verdicts).toEqual(["comment"]);
  dialogButton("Submit review").click();
  await settle();
  expect(writes).toEqual([
    { SubmitReview: { entity: PR, verdict: "comment", body: "", comments: [{ path: "docs/backoff.md", line: 3, side: "new", body: "Say ±10 %." }] } },
  ]);
});

// -- Merge… (#843, R-8.19) --------------------------------------------------

const mergeAction = (disabled: string | null) =>
  action("merge_pull_request", "Merge…", {
    primary: true,
    disabled,
    confirm: "Merge feature/PAY-231-sepa-retry into main?",
    fields: [
      {
        name: "style",
        label: "Style",
        kind: {
          type: "choice",
          options: [
            { value: "merge", label: "Merge commit" },
            { value: "rebase", label: "Rebase" },
            { value: "squash", label: "Squash" },
          ],
        },
        value: null,
        required: true,
        placeholder: null,
      },
      { name: "delete_branch", label: "Delete branch after merge", kind: { type: "flag" }, value: "true", required: false, placeholder: null },
      {
        name: "message",
        label: "Squash commit message",
        kind: { type: "text", suggestions: [] },
        value: "SEPA retry with exponential backoff (#142)",
        required: false,
        placeholder: null,
      },
    ],
  });

/** The pull request, linked by knobas to PAY-231. */
function linked(): EntityDetail {
  return {
    ...detail("mara"),
    source: { id: "gitea", display_name: "Gitea", adapter_kind: "gitea", enabled: true },
    links: [{ link: { id: "link:1" }, other: { entity_id: "jira:PAY-231", kind: "ticket", title: "Retry", deleted_at: null } }],
  } as unknown as EntityDetail;
}

test("Merge… blocked: a warn box names each reason, Merge is disabled with the sentence as its tooltip", async () => {
  const blocked = mergeAction("1 of 2 required approvals; required check Payout_IntegrationTests #1187 failed");
  await draw([PR], ports({ getEntity: async () => linked(), pr: prView([COMMENT, EDIT, blocked]) }));
  button("Merge…")!.click();
  flushSync();
  const dialog = document.querySelector(".tf-dialog")!;
  expect(dialog.querySelector("h2")!.textContent).toBe("Merge PR #142 into main");
  const box = dialog.querySelector(".tf-prm-blocked")!;
  expect(box.firstChild!.textContent!.trim()).toBe("Gitea blocks this merge:");
  expect([...box.querySelectorAll("span")].map((s) => s.textContent)).toEqual([
    "1 of 2 required approvals",
    "required check Payout_IntegrationTests #1187 failed",
  ]);
  const merge = dialogButton("Merge");
  expect(merge.getAttribute("aria-disabled")).toBe("true");
  expect(merge.title).toBe("1 of 2 required approvals; required check Payout_IntegrationTests #1187 failed");
  merge.click();
  await settle();
  expect(writes).toEqual([]);
});

test("Merge… open: the three lines, the styles, a squash's message, the delete flag and the In Review line", async () => {
  await draw([PR], ports({ getEntity: async () => linked(), pr: prView([COMMENT, EDIT, mergeAction(null)]) }));
  button("Merge…")!.click();
  flushSync();
  const dialog = document.querySelector(".tf-dialog")!;
  expect([...dialog.querySelectorAll(".tf-prm-ready > span")].map((s) => s.textContent?.trim())).toEqual([
    "Approvals: 1 of 2",
    "Checks: Payout_IntegrationTests #1187 failure",
    "No conflicts with main",
  ]);
  expect([...dialog.querySelectorAll(".tf-radio label")].map((l) => l.textContent?.trim())).toEqual([
    "Create a merge commit",
    "Rebase onto main",
    "Squash into one commit",
  ]);
  expect(text(".tf-dialog .tf-ck")).toBe("Delete feature/PAY-231-sepa-retry after merging");
  expect(dialog.querySelector<HTMLInputElement>(".tf-ck input")!.checked).toBe(true);
  expect(text(".tf-dialog .tf-prm-note")).toBe("After the merge, PAY-231 moves to In Review (the merge follower already does this).");
  // The message shows only under Squash, prefilled as the source writes it.
  expect(dialog.querySelector("input[aria-label='Squash commit message']")).toBeNull();
  dialog.querySelector<HTMLInputElement>("input[value=squash]")!.click();
  flushSync();
  const message = dialog.querySelector<HTMLInputElement>("input[aria-label='Squash commit message']")!;
  expect(message.value).toBe("SEPA retry with exponential backoff (#142)");
  type(message, "SEPA retry with backoff (#142)");
  dialog.querySelector<HTMLInputElement>(".tf-ck input")!.click();
  flushSync();
  dialogButton("Merge").click();
  await settle();
  expect(writes).toEqual([
    { MergePullRequest: { entity: PR, style: "squash", delete_branch: false, message: "SEPA retry with backoff (#142)" } },
  ]);
  expect(document.querySelector(".tf-dialog")).toBeNull();
  expect(text(".tf-fhead .tf-sub")).toContain("merge queued");
});

test("a merge commit sends no message, and a pull request linked to no ticket draws no In Review line", async () => {
  await draw([PR], ports({ pr: prView([COMMENT, EDIT, mergeAction(null)]) }));
  button("Merge…")!.click();
  flushSync();
  expect(document.querySelector(".tf-prm-note")).toBeNull();
  dialogButton("Merge").click();
  await settle();
  expect(writes).toEqual([{ MergePullRequest: { entity: PR, style: "merge", delete_branch: true } }]);
});

test("a merge while the pull request's source fails its sign-in says it waits there", async () => {
  health.patch({ source_id: "gitea", state: "unauthorized", checked_at: null, detail: null, secret_expires_at: null });
  await draw([PR], ports({ getEntity: async () => linked(), pr: prView([COMMENT, EDIT, mergeAction(null)]) }));
  button("Merge…")!.click();
  flushSync();
  dialogButton("Merge").click();
  await settle();
  expect(toasts.items.at(-1)!.text).toBe("Queued: merge PR #142. Gitea sign-in is failing, so it waits there.");
});

// -- A queued write holds back what it makes stale (#1441) ------------------

/** Queue a merge through Merge…'s dialog. */
async function queueMerge() {
  button("Merge…")!.click();
  flushSync();
  dialogButton("Merge").click();
  await settle();
}

const holdOf = (name: string) => {
  const b = button(name)!;
  return [b.getAttribute("aria-disabled"), b.title];
};

test("after a queued merge, Merge… is held: aria-disabled, titled merge queued, not primary, and a press opens nothing", async () => {
  await draw([PR], ports({ pr: prView([COMMENT, EDIT, CLOSE, UPDATE, mergeAction(null)]) }));
  await queueMerge();
  expect(writes).toHaveLength(1);
  expect(holdOf("Merge…")).toEqual(["true", "merge queued"]);
  expect(button("Merge…")!.classList.contains("tf-primary")).toBe(false);
  button("Merge…")!.click();
  flushSync();
  expect(document.querySelector(".tf-dialog")).toBeNull();
  expect(writes).toHaveLength(1);
});

test("after a queued merge, Update branch, Edit and Close are held with merge queued and open nothing", async () => {
  await draw([PR], ports({ pr: prView([COMMENT, EDIT, CLOSE, UPDATE, mergeAction(null)]) }));
  await queueMerge();
  for (const name of ["Update branch", "Edit", "Close"]) {
    expect(holdOf(name)).toEqual(["true", "merge queued"]);
    button(name)!.click();
    flushSync();
    expect(document.querySelector(".tf-dialog")).toBeNull();
  }
});

test("after a queued close, Merge… and Close are held with close queued; Edit and Update branch stay live", async () => {
  await draw([PR], ports({ pr: prView([COMMENT, EDIT, CLOSE, UPDATE, mergeAction(null)]) }));
  button("Close")!.click();
  flushSync();
  dialogButton("Close").click();
  await settle();
  expect(writes).toHaveLength(1);
  expect(holdOf("Merge…")).toEqual(["true", "close queued"]);
  expect(holdOf("Close")).toEqual(["true", "close queued"]);
  expect(holdOf("Edit")).toEqual([null, ""]);
  expect(holdOf("Update branch")).toEqual([null, ""]);
  button("Merge…")!.click();
  flushSync();
  expect(document.querySelector(".tf-dialog")).toBeNull();
});

test("the hold outlives leaving the page and ends with the session; a source's own action is never held", async () => {
  const stamp = action("source_action", "Stamp release", { values: { id: "stamp_release" } });
  const offered = () => ports({ pr: prView([COMMENT, EDIT, stamp, mergeAction(null)]) });
  await draw([PR], offered());
  await queueMerge();
  expect(holdOf("Stamp release")).toEqual([null, ""]);
  unmount(view!);
  await draw([PR], offered());
  expect(holdOf("Merge…")).toEqual(["true", "merge queued"]);
  expect(holdOf("Edit")).toEqual(["true", "merge queued"]);
  unmount(view!);
  forgetQueued();
  await draw([PR], offered());
  expect(holdOf("Merge…")).toEqual([null, ""]);
  expect(button("Merge…")!.classList.contains("tf-primary")).toBe(true);
  expect(holdOf("Edit")).toEqual([null, ""]);
});

// -- A queued mark goes once the source's read shows it (#1542) -------------

const REOPEN = action("edit_pull_request", "Reopen", { values: { state: "open" } });

/** Queue a close through Close's dialog. */
async function queueClose() {
  button("Close")!.click();
  flushSync();
  dialogButton("Close").click();
  await settle();
}

const chips = () => [...target.querySelectorAll(".tf-fhead .tf-sub .tf-chip")].map((c) => c.textContent?.trim());

test("a landed close drops its chip and the holds it drove; the source reopening it later does not bring the chip back", async () => {
  await draw([PR], ports({ pr: prView([COMMENT, EDIT, CLOSE, mergeAction(null)]) }));
  await queueClose();
  expect(chips()).toEqual(["open", "close queued"]);
  unmount(view!);
  await draw([PR], ports({ pr: prView([COMMENT, EDIT, REOPEN], "closed") }));
  expect(chips()).toEqual(["closed"]);
  expect(holdOf("Reopen")).toEqual([null, ""]);
  unmount(view!);
  await draw([PR], ports({ pr: prView([COMMENT, EDIT, CLOSE, mergeAction(null)]) }));
  expect(chips()).toEqual(["open"]);
  expect(holdOf("Merge…")).toEqual([null, ""]);
});

test("a close the read still shows open keeps its chip and its holds", async () => {
  await draw([PR], ports({ pr: prView([COMMENT, EDIT, CLOSE, mergeAction(null)]) }));
  await queueClose();
  unmount(view!);
  await draw([PR], ports({ pr: prView([COMMENT, EDIT, CLOSE, mergeAction(null)]) }));
  expect(chips()).toEqual(["open", "close queued"]);
  expect(holdOf("Merge…")).toEqual(["true", "close queued"]);
});

test("a landed merge drops its chip; an edit, which no read shows, keeps its chip for the session", async () => {
  await draw([PR], ports({ pr: prView([COMMENT, EDIT, mergeAction(null)]) }));
  button("Edit")!.click();
  flushSync();
  const title = document.querySelector<HTMLInputElement>('.tf-dialog input[aria-label="Title"]')!;
  title.value = "SEPA retry with backoff";
  title.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  dialogButton("Edit").click();
  await settle();
  await queueMerge();
  expect(chips()).toEqual(["open", "edit queued", "merge queued"]);
  unmount(view!);
  await draw([PR], ports({ pr: prView([COMMENT], "merged") }));
  expect(chips()).toEqual(["merged", "edit queued"]);
});

test("a comment's mark goes once the conversation holds its words", async () => {
  await draw([PR, "conversation"]);
  const box = target.querySelector<HTMLTextAreaElement>(".tf-compose textarea")!;
  box.value = "Bounded to ±10 %.";
  box.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  button("Comment")!.click();
  await settle();
  expect(target.querySelectorAll(".tf-convo .tf-msg")).toHaveLength(3);
  unmount(view!);
  await draw([PR, "conversation"]);
  expect(target.querySelectorAll(".tf-convo .tf-msg")).toHaveLength(2);
});

test("a close the queue refuses drops its chip and its holds, and says why", async () => {
  let hear: Hear = () => {};
  const follower: WriteFollower = {
    follow: (_id, h) => {
      hear = h;
      return () => {};
    },
  };
  const numbered = ports({
    pr: prView([COMMENT, EDIT, CLOSE, mergeAction(null)]),
    submitWrite: async (payload) => {
      writes.push(payload);
      return { id: 31 } as never;
    },
  });
  await draw([PR], numbered, follower);
  await queueClose();
  expect(holdOf("Merge…")).toEqual(["true", "close queued"]);
  hear({ id: 31, state: "refused", detail: "403 Forbidden" } as never);
  flushSync();
  expect(chips()).toEqual(["open"]);
  expect(holdOf("Merge…")).toEqual([null, ""]);
  expect(toasts.items.at(-1)!.text).toBe("Close was refused: 403 Forbidden");
});

test("the repository is found in a source of more than 1000 repositories (#1825)", async () => {
  const many = Array.from({ length: 1200 }, (_, n) => row(`gitea:tidewater/r${n}`, "repo", `tidewater/r${n}`));
  many[1100] = row(REPO, "repo", "tidewater/payout-service");
  const offsets: number[] = [];
  await draw(
    [PR],
    ports({
      listEntities: async (_filter, limit, offset) => {
        offsets.push(offset);
        return { rows: many.slice(offset, offset + limit), total: many.length } as never;
      },
    }),
  );
  expect(target.querySelector(".tf-crumbs a[href='#/repo/" + encodeURIComponent(REPO) + "']")?.textContent).toBe("tidewater/payout-service");
  expect(offsets).toEqual([0, 1000]);
  expect(text(".tf-prlocal")).toContain("/Users/mara/code/payout-service");
});
