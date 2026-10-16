/**
 * The pull request page's pure half (#841): what an address names, and what
 * the source's view of a pull request (`knobas-source-gitea`'s `view/pr.rs`)
 * says the header, *Ready to merge?*, the checks and the conversation draw.
 */
import { expect, test } from "vitest";

import type { SourceView, Tone, ViewAction, ViewRow } from "../../ipc/entity";
import { checksLine, conversationOf, headerOf, prHash, readPrRoute, readyLines, tabCounts } from "./pr";

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

test("Ready to merge? draws the source's Mergeable, each standing review and the checks, each with its tone", () => {
  expect(readyLines(VIEW)).toEqual([
    { text: "Mergeable: no: 1 of 2 required approvals; required check Payout_IntegrationTests", tone: "warn" },
    { text: "Approved by priya", tone: "ok" },
    { text: "Changes requested by jonas", tone: "warn" },
    { text: "Checks: Payout_IntegrationTests #1187 failure, Payout_Build #1188 pending", tone: "err" },
  ]);
});

test("a mergeable pull request reads ok; no reviews yet says so; no checks draws no checks line", () => {
  const view: SourceView = {
    sections: [
      {
        id: "overview",
        title: "Overview",
        count: null,
        deferred: false,
        blocks: [{ type: "facts", facts: [fact("State", "open", "info"), fact("Mergeable", "yes")] }],
      },
    ],
    actions: [],
  };
  expect(readyLines(view)).toEqual([
    { text: "Mergeable: yes", tone: "ok" },
    { text: "No reviews yet", tone: "muted" },
  ]);
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
    { who: "jonas", at: "2026-08-22 09:30", verdict: { text: "changes requested", tone: "warn" }, body: "Should the jitter be bounded?" },
    { who: "mara", at: "2026-08-22 10:20", verdict: null, body: "Bounded to ±10 % in c90d11." },
    { who: "priya", at: "2026-08-22 11:00", verdict: { text: "approved", tone: "ok" }, body: "" },
  ]);
  expect(conversationOf(null)).toEqual([]);
});
