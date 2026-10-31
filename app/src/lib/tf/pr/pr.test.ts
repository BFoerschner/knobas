/**
 * The pull request page's pure half (#841): what an address names, and what
 * the source's view of a pull request (`knobas-source-gitea`'s `view/pr.rs`)
 * says the header, *Ready to merge?*, the checks and the conversation draw.
 */
import { expect, test } from "vitest";

import type { SourceView, Tone, ViewAction, ViewRow } from "../../ipc/entity";
import {
  checksLine,
  conversationOf,
  headerOf,
  conflictsOf,
  mergeReasons,
  prHash,
  railChecks,
  readiness,
  readPrRoute,
  readyLines,
  reviewChoices,
  reviewersOf,
  tabCounts,
} from "./pr";

const PR = "gitea:tidewater/payout-service#142";

test("an address names the pull request and its tab, Overview by default", () => {
  expect(readPrRoute([PR])).toEqual({ prId: PR, tab: "overview" });
  expect(readPrRoute([PR, "conversation"])).toEqual({ prId: PR, tab: "conversation" });
  expect(readPrRoute([PR, "commits"])).toEqual({ prId: PR, tab: "commits" });
  expect(readPrRoute([PR, "diff"])).toEqual({ prId: PR, tab: "diff" });
  expect(readPrRoute([PR, "nonsense"])).toEqual({ prId: PR, tab: "overview" });
  expect(readPrRoute([])).toEqual({ prId: null, tab: "overview" });
});

test("a tab's address carries the entity id encoded whole; Overview is the bare address", () => {
  expect(prHash(PR)).toBe("#/pr/gitea%3Atidewater%2Fpayout-service%23142");
  expect(prHash(PR, "overview")).toBe("#/pr/gitea%3Atidewater%2Fpayout-service%23142");
  expect(prHash(PR, "commits")).toBe("#/pr/gitea%3Atidewater%2Fpayout-service%23142/commits");
});

const fact = (label: string, value: string, tone: Tone | null = null) => ({
  label,
  value,
  tone,
});
const vrow = (title: string, chip: string | null, tone: ViewRow["tone"], meta: string | null = null, url: string | null = null): ViewRow => ({
  entity_id: null,
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

const VIEW: SourceView = {
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
            fact("Mergeable", "no: 1 of 2 required approvals; required check Payout_IntegrationTests", "warn"),
            fact("Approvals", "1 of 2", "warn"),
            fact("Conflicts", "none with main", "ok"),
            fact("Created", "2026-08-21 15:30"),
            fact("Review", "approved by priya", "ok"),
            fact("Review", "changes requested by jonas", "warn"),
          ],
        },
        {
          type: "entities",
          title: "Checks",
          rows: [
            vrow("Payout_IntegrationTests #1187", "failure", "err", "41 passed, 1 failed"),
            vrow("Payout_Build #1188", "pending", "info", "step 3 of 5"),
            vrow("Payout_Lint #904", "success", "ok"),
          ],
        },
        { type: "text", text: "Retries SEPA payouts.\n\nCloses PAY-231.", markdown: true },
      ],
    },
    { id: "files", title: "Files", count: 3, deferred: true, blocks: [] },
    { id: "commits", title: "Commits", count: 3, deferred: false, blocks: [{ type: "entities", title: null, rows: [] }] },
    {
      id: "conversation",
      title: "Conversation",
      count: 2,
      deferred: false,
      blocks: [
        { type: "facts", facts: [fact("jonas", "2026-08-22 09:30"), fact("Review", "changes requested", "warn")] },
        { type: "text", text: "Should the jitter be bounded?", markdown: true },
        { type: "facts", facts: [fact("mara", "2026-08-22 10:20")] },
        { type: "text", text: "Bounded to ±10 % in c90d11.", markdown: true },
        { type: "facts", facts: [fact("priya", "2026-08-22 11:00"), fact("Review", "approved", "ok")] },
      ],
    },
  ],
  actions: [action("comment", "Comment…"), action("edit_pull_request", "Edit…")],
};

test("the header reads the state, head into base and author from the overview's facts", () => {
  expect(headerOf(VIEW)).toEqual({
    state: { text: "open", tone: "info" },
    author: "mara",
    head: "feature/PAY-231-sepa-retry",
    base: "main",
    description: "Retries SEPA payouts.\n\nCloses PAY-231.",
    checks: VIEW.sections[0]!.blocks[1]!.type === "entities" ? VIEW.sections[0]!.blocks[1]!.rows : [],
  });
});

test("a fork's head keeps its owner, and a Branches fact without an arrow is the head alone", () => {
  const with_ = (value: string): SourceView => ({
    sections: [{ id: "overview", title: "Overview", count: null, deferred: false, blocks: [{ type: "facts", facts: [fact("Branches", value)] }] }],
    actions: [],
  });
  expect(headerOf(with_("mara/payout-service:fix → main"))).toMatchObject({ head: "mara/payout-service:fix", base: "main" });
  expect(headerOf(with_("odd"))).toMatchObject({ head: "odd", base: null });
  expect(headerOf(null)).toMatchObject({ state: null, author: null, head: null, base: null, description: null, checks: [] });
});

test("the tab counts are the sections' own: conversation, files changed, commits", () => {
  expect(tabCounts(VIEW)).toEqual({ overview: null, conversation: 2, diff: 3, commits: 3 });
  expect(tabCounts(null)).toEqual({ overview: null, conversation: null, diff: null, commits: null });
});

test("Ready to merge? reads prototype-6's three lines from the declared Approvals, the checks and Conflicts (#994)", () => {
  expect(readyLines(VIEW)).toEqual([
    { text: "Approvals: 1 of 2", tone: "warn" },
    { text: "Checks: Payout_IntegrationTests #1187 failure, Payout_Build #1188 pending", tone: "err" },
    { text: "No conflicts with main", tone: "ok" },
  ]);
});

const overviewOf = (...facts: ReturnType<typeof fact>[]): SourceView => ({
  sections: [{ id: "overview", title: "Overview", count: null, deferred: false, blocks: [{ type: "facts", facts }] }],
  actions: [],
});

test("a conflict reads warn; no approval required says so; no checks draws no checks line", () => {
  expect(readyLines(overviewOf(fact("Approvals", "none required", "ok"), fact("Conflicts", "conflicts with main", "warn")))).toEqual([
    { text: "Approvals: none required", tone: "ok" },
    { text: "Conflicts with main", tone: "warn" },
  ]);
});

test("a source that declares neither falls back to its Mergeable sentence, drawn as written", () => {
  expect(readyLines(overviewOf(fact("State", "open", "info"), fact("Mergeable", "yes")))).toEqual([
    { text: "Mergeable: yes", tone: "ok" },
  ]);
  // A merged pull request declares nothing to merge on.
  expect(readyLines(overviewOf(fact("State", "merged", "ok")))).toEqual([]);
});

test("Merge…'s reasons are its source's sentence, one line per reason", () => {
  const merge = (disabled: string | null) => action("merge_pull_request", "Merge…", { disabled });
  expect(mergeReasons(merge(null))).toEqual([]);
  expect(mergeReasons(merge("1 of 2 required approvals; required check Payout_IntegrationTests #1187 failed"))).toEqual([
    "1 of 2 required approvals",
    "required check Payout_IntegrationTests #1187 failed",
  ]);
  expect(mergeReasons(null)).toEqual([]);
});

test("the review's choices are the verdicts the source offers; Comment only with line comments waiting", () => {
  const submit = action("submit_review", "Submit review…", { values: { verdict: "comment" }, line_comments: "requires" });
  const approve = action("submit_review", "Approve…", { values: { verdict: "approve" }, line_comments: "sends" });
  const changes = action("submit_review", "Request changes…", { values: { verdict: "request_changes" }, line_comments: "sends" });
  const all = [action("comment", "Comment…"), submit, approve, changes];
  expect(reviewChoices(all, 0).map((c) => [c.verdict, c.label, c.action === null])).toEqual([
    ["comment", "Comment", true],
    ["approve", "Approve", false],
    ["request_changes", "Request changes", false],
  ]);
  expect(reviewChoices(all, 2).map((c) => [c.verdict, c.action?.label ?? null])).toEqual([
    ["comment", "Submit review…"],
    ["approve", "Approve…"],
    ["request_changes", "Request changes…"],
  ]);
  // One's own pull request: only the comment, and only with comments waiting.
  expect(reviewChoices([submit], 1).map((c) => c.verdict)).toEqual(["comment"]);
  expect(reviewChoices([submit], 0).map((c) => c.action)).toEqual([null]);
  expect(reviewChoices([], 3)).toEqual([]);
  // A verdict the source draws disabled (#2030) cannot be sent, and says why.
  const refused = reviewChoices([{ ...approve, disabled: "This token cannot write" }], 2);
  expect(refused.map((c) => [c.action, c.why])).toEqual([[null, "This token cannot write"]]);
  expect(reviewChoices([submit], 0).map((c) => c.why)).toEqual(["Add a line comment first"]);
  expect(reviewChoices(all, 2).map((c) => c.why)).toEqual([null, null, null]);
  // The approval's meaning names the base its protection is on.
  expect(reviewChoices(all, 0, "main").find((c) => c.verdict === "approve")?.meaning).toBe("Counts toward the approvals main needs");
});

test("the checks line: every check passing, else each one that is not, the loudest tone", () => {
  expect(checksLine([vrow("A", "success", "ok"), vrow("B", "success", "ok")])).toEqual({ text: "Checks: all checks passing", tone: "ok" });
  expect(checksLine([vrow("A", "success", "ok"), vrow("B", "pending", "info")])).toEqual({ text: "Checks: B pending", tone: "info" });
  expect(checksLine([vrow("A", "warning", "warn"), vrow("B", "error", "err")])).toEqual({ text: "Checks: A warning, B error", tone: "err" });
  expect(checksLine([])).toBeNull();
  // The muted line a failed status read leaves is drawn as the source wrote it.
  expect(checksLine([vrow("Checks could not be read: 500", null, "muted")])).toEqual({ text: "Checks could not be read: 500", tone: "muted" });
});

test("the conversation is one message per facts block: who, when, the review's verdict, then the words", () => {
  expect(conversationOf(VIEW)).toEqual([
    { who: "jonas", at: "2026-08-22 09:30", verdict: { text: "changes requested", tone: "warn" }, body: "Should the jitter be bounded?", markdown: true },
    { who: "mara", at: "2026-08-22 10:20", verdict: null, body: "Bounded to ±10 % in c90d11.", markdown: true },
    { who: "priya", at: "2026-08-22 11:00", verdict: { text: "approved", tone: "ok" }, body: "", markdown: false },
  ]);
  expect(conversationOf(null)).toEqual([]);
});

/** The side rail (#2278): an overview whose base requires ci/test and ci/build, ci/test failing. */
const RAIL = (merge: ViewAction | null, ...extra: ReturnType<typeof fact>[]): SourceView => ({
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
            fact("Approvals", "2 of 2", "ok"),
            fact("Conflicts", "none with main", "ok"),
            fact("Required checks", "1 of 3 pass", "err"),
            fact("Required check", "ci/build", "ok"),
            fact("Required check", "ci/test", "err"),
            fact("Required check", "deploy", "info"),
            fact("Review", "approved by sven", "ok"),
            fact("Review", "changes requested by jonas", "warn"),
            fact("Review", "requested from priya", "muted"),
            ...extra,
          ],
        },
        {
          type: "entities",
          title: "Checks",
          rows: [
            vrow("coverage", "failure", "err"),
            vrow("ci/test", "failure", "err", "6:12"),
            vrow("ci/lint", "pending", "info"),
            vrow("ci/build", "success", "ok"),
          ],
        },
      ],
    },
  ],
  actions: merge === null ? [] : [merge],
});

test("the rail's checks: a failing required check first, each marked required, a required one that never reported last", () => {
  expect(railChecks(RAIL(null)).map((check) => [check.row.title, check.row.tone, check.required])).toEqual([
    ["ci/test", "err", true],
    ["coverage", "err", false],
    ["ci/lint", "info", false],
    ["ci/build", "ok", true],
    ["deploy", "info", true],
  ]);
  expect(railChecks(RAIL(null)).at(-1)!.row.chip).toBe("not reported");
  // A source that names no required check: the checks as it sorted them, none marked.
  expect(railChecks(VIEW).map((check) => check.required)).toEqual([false, false, false]);
});

test("the rail's checks stand where the source's required check says: a required warning fails, a skipped one passes", () => {
  const view: SourceView = {
    actions: [],
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
              fact("Approvals", "none required", "ok"),
              fact("Required checks", "1 of 2 pass", "err"),
              fact("Required check", "ci/lint", "err"),
              fact("Required check", "ci/docs", "ok"),
            ],
          },
          {
            type: "entities",
            title: "Checks",
            rows: [vrow("coverage", "failure", "err"), vrow("ci/lint", "warning", "warn"), vrow("ci/docs", "skipped", null)],
          },
        ],
      },
    ],
  };
  expect(railChecks(view).map((check) => [check.row.title, check.standing])).toEqual([
    ["ci/lint", "err"],
    ["coverage", "err"],
    ["ci/docs", "ok"],
  ]);
  expect(readyLines(view)[1]).toEqual({ text: "Required checks: 1 of 2 pass", tone: "err", why: "ci/lint warning" });
});

test("Ready to merge? reads the required checks from the source, saying which ones stand in the way", () => {
  expect(readyLines(RAIL(null))).toEqual([
    { text: "Approvals: 2 of 2", tone: "ok" },
    { text: "Required checks: 1 of 3 pass", tone: "err", why: "ci/test failure, deploy not reported" },
    { text: "No conflicts with main", tone: "ok" },
  ]);
});

test("readiness: not ready while a line is unmet or Merge is refused, ready only when both are clear", () => {
  const blocked = action("merge_pull_request", "Merge…", { disabled: "required check ci/test failed" });
  expect(readiness(RAIL(blocked)).verdict).toEqual({ text: "Not yet: 2 of 3 met", tone: "err", why: "required check ci/test failed" });
  const passing = RAIL(blocked).sections[0]!;
  const allMet: SourceView = {
    actions: [action("merge_pull_request", "Merge…")],
    sections: [
      {
        ...passing,
        blocks: [{ type: "facts", facts: [fact("Approvals", "2 of 2", "ok"), fact("Conflicts", "none with main", "ok")] }],
      },
    ],
  };
  expect(readiness(allMet).verdict).toEqual({ text: "Ready: 2 of 2 met", tone: "ok", why: null });
  // A line unmet while Merge is offered (the source has not caught up with ci/test): not ready, no reason to quote.
  expect(readiness(RAIL(action("merge_pull_request", "Merge…"))).verdict).toEqual({ text: "Not yet: 2 of 3 met", tone: "err", why: null });
  // Every required check passed: ready with the required checks among the lines.
  const passed: SourceView = {
    actions: [action("merge_pull_request", "Merge…")],
    sections: [
      {
        ...passing,
        blocks: [
          {
            type: "facts",
            facts: [
              fact("Approvals", "2 of 2", "ok"),
              fact("Conflicts", "none with main", "ok"),
              fact("Required checks", "1 of 1 pass", "ok"),
              fact("Required check", "ci/build", "ok"),
            ],
          },
          { type: "entities", title: "Checks", rows: [vrow("ci/build", "success", "ok")] },
        ],
      },
    ],
  };
  expect(readiness(passed).verdict).toEqual({ text: "Ready: 3 of 3 met", tone: "ok", why: null });
  // Every line met, Merge still refused (a head behind a protected base): not ready, saying why.
  const behind: SourceView = { ...allMet, actions: [action("merge_pull_request", "Merge…", { disabled: "the head is behind main" })] };
  expect(readiness(behind).verdict).toEqual({ text: "Not yet: 2 of 2 met", tone: "warn", why: "the head is behind main" });
  // No Merge offered (merged, closed): no verdict, the lines alone.
  expect(readiness(RAIL(null)).verdict).toBeNull();
  expect(readiness(RAIL(null)).lines).toHaveLength(3);
});

test("the reviewers and their state, from the source's standing verdicts", () => {
  expect(reviewersOf(RAIL(null))).toEqual([
    { who: "sven", state: "Approved", tone: "ok" },
    { who: "jonas", state: "Changes requested", tone: "warn" },
    { who: "priya", state: "Review requested", tone: "muted" },
  ]);
  expect(reviewersOf(null)).toEqual([]);
});

test("conflicts: none, or the base it conflicts with; nothing while the source has not said", () => {
  expect(conflictsOf(RAIL(null))).toEqual({ conflicts: false, text: "No conflicts with main" });
  expect(conflictsOf(overviewOf(fact("Conflicts", "conflicts with main", "warn")))).toEqual({ conflicts: true, text: "Conflicts with main" });
  expect(conflictsOf(overviewOf(fact("State", "open", "info")))).toBeNull();
});
