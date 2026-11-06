/**
 * The Inbox's pure half (#834, R-12.1 to R-12.3): what a row says, where it
 * opens, which job it belongs to, and which rows the filter keeps.
 */
import { describe, expect, test, vi } from "vitest";

import { kindRegistry } from "../../shell/kind-registry.svelte";

import type { InboxEntry, InboxItem, Reached } from "../../ipc/entity";
import { page as codeRowPage } from "../code-row/page";
import { jobIndex } from "../job/open-jobs";
import { pageFor } from "../pages";
import { parseTfHash } from "../route";
import {
  kindOf,
  opener,
  rowActions,
  named,
  sentence,
  backLabel,
  untilLabel,
  visible,
  fixOf,
  shortDay,
} from "./inbox";

function item(over: Partial<InboxItem>): InboxItem {
  return {
    key: "review_request:gitea:payouts#142",
    category: "review_request",
    source_id: "gitea",
    entity_id: "gitea:payouts#142",
    kind: "pr",
    title: "Idempotent retry window",
    reason: "jonas asked for your review",
    occurred_at: "2026-08-22T11:50:00Z",
    web_url: null,
    snoozed_until: null,
    ...over,
  };
}

function entry(over: Partial<InboxItem>, actions: string[] = []): InboxEntry {
  return { item: item(over), actions };
}

describe("a row's sentence", () => {
  test("leads with its subject, then why it is here (F-115)", () => {
    expect(sentence(item({ category: "new_assignment", reason: "assigned to you", title: "Payout dashboard latency" }))).toBe(
      "Payout dashboard latency: assigned to you",
    );
    expect(sentence(item({}))).toBe("Idempotent retry window: jonas asked for your review");
    expect(sentence(item({ reason: "somebody asked for your review" }))).toBe("Idempotent retry window: somebody asked for your review");
  });

  test("keeps a source's word as the source spells it", () => {
    expect(sentence(item({ category: "alert", reason: "knobas-jira is down", title: "jira" }))).toBe("knobas-jira is down");
    expect(sentence(item({ category: "failed_build", reason: "x #1 failed", title: "x #1" }))).toBe("x #1 failed");
  });

  test("a monitor named apart from its asset leads, and the asset says the trouble", () => {
    expect(sentence(item({ category: "alert", reason: "knobas-jira is down", title: "HTTP health" }))).toBe("HTTP health: knobas-jira is down");
  });

  test("a toast names the subject alone, or the sentence where the reason holds it", () => {
    expect(named(item({ category: "new_assignment", reason: "assigned to you", title: "Payout dashboard latency" }))).toBe("Payout dashboard latency");
    expect(named(item({ category: "alert", reason: "knobas-jira is down", title: "jira" }))).toBe("knobas-jira is down");
  });

  test("is the reason alone when the reason already names the item", () => {
    const credential = item({
      category: "credential_expiry",
      entity_id: null,
      kind: null,
      title: "Jira",
      reason: "the Jira credential expires on Monday 7 Sep 2026",
    });
    expect(sentence(credential)).toBe("The Jira credential expires on Monday 7 Sep 2026");
  });

  test("a failed sign-in reads as the backend's reason, capitalised, with its source named once", () => {
    const signIn = item({
      category: "failed_sign_in",
      entity_id: null,
      kind: null,
      title: "Tidewater TeamCity",
      reason: "the Tidewater TeamCity sign-in was rejected",
    });
    expect(sentence(signIn)).toBe("The Tidewater TeamCity sign-in was rejected");
  });
});

test("the kind word is the chip's word for each category", () => {
  expect(kindOf("review_request")).toBe("Review");
  expect(kindOf("mention")).toBe("Mention");
  expect(kindOf("failed_build")).toBe("Build");
  expect(kindOf("new_assignment")).toBe("Assignment");
  expect(kindOf("alert")).toBe("Monitor");
  expect(kindOf("credential_expiry")).toBe("Credential");
  expect(kindOf("failed_sign_in")).toBe("Sign-in");
});

describe("where a row opens", () => {
  test("a review request opens the pull request as Review", () => {
    expect(opener(item({}))).toEqual({ label: "Review", href: "#/pr/gitea%3Apayouts%23142" });
  });
  test("a failed build opens its log", () => {
    expect(opener(item({ category: "failed_build", entity_id: "tc:Payout#1187", kind: "build" }))).toEqual({
      label: "Open log",
      href: "#/build/tc%3APayout%231187",
    });
  });
  test("an alert opens Monitors at its monitor's row, named by its key", () => {
    expect(opener(item({ category: "alert", key: "alert:kuma:3", entity_id: "asset:gitea", kind: "asset" }))?.href).toBe("#/monitors/at/kuma%3A3");
  });
  test("an alert whose key names no monitor opens Monitors", () => {
    expect(opener(item({ category: "alert", entity_id: "asset:gitea", kind: "asset" }))).toEqual({
      label: "Open monitor",
      href: "#/monitors",
    });
  });
  test("an assignment opens the ticket page, to read before taking it on (spec #2227)", () => {
    expect(opener(item({ category: "new_assignment", entity_id: "jira:PAY-240", kind: "ticket" }))).toEqual({
      label: "Open ticket",
      href: "#/ticket/jira%3APAY-240",
    });
  });
  test("a mention opens where it was said, at the mention on a ticket", () => {
    expect(opener(item({ category: "mention", entity_id: "jira:PAY-231", kind: "ticket" }))).toEqual({
      label: "Open conversation",
      href: "#/ticket/jira%3APAY-231/mention",
    });
    expect(opener(item({ category: "mention", entity_id: "conf:SEPA", kind: "page" }))).toEqual({
      label: "Open page",
      href: "#/page/conf%3ASEPA",
    });
    expect(opener(item({ category: "mention", kind: "pr" }))?.label).toBe("Open PR");
  });
  test("a branch or commit opens its own address, which the repository page's redirect draws (#1440)", () => {
    const branch = opener(item({ category: "mention", entity_id: "gitea:x/pay@refs/heads/main", kind: "branch" }))!;
    const commit = opener(item({ category: "mention", entity_id: "gitea:x/pay@9f2c1ab", kind: "commit" }))!;
    expect(branch).toEqual({ label: "Open", href: "#/branch/gitea%3Ax%2Fpay%40refs%2Fheads%2Fmain" });
    expect(commit).toEqual({ label: "Open", href: "#/commit/gitea%3Ax%2Fpay%409f2c1ab" });
    for (const { href } of [branch, commit]) expect(pageFor(parseTfHash(href)), href).toBe(codeRowPage);
  });
  test("a failed sign-in opens its source's settings, where Re-enter is", () => {
    expect(opener(item({ category: "failed_sign_in", source_id: "teamcity", entity_id: null, kind: null }))).toEqual({
      label: "Source settings",
      href: "#/settings/source/teamcity",
    });
  });
  test("a credential expiry opens its source's settings", () => {
    expect(opener(item({ category: "credential_expiry", source_id: "jira", entity_id: null, kind: null }))).toEqual({
      label: "Source settings",
      href: "#/settings/source/jira",
    });
  });
});

describe("a row's actions", () => {
  test("re-run is drawn from the source's ops; approve and comment are not", () => {
    expect(rowActions(entry({ category: "failed_build" }, ["rerun_build"]))).toEqual(["rerun_build", "snooze", "done"]);
    expect(rowActions(entry({}, ["approve", "comment"]))).toEqual(["snooze", "done"]);
  });
  test("an alert offers Ack", () => {
    expect(rowActions(entry({ category: "alert" }))).toEqual(["ack", "snooze", "done"]);
  });
  test("an op the source does not declare is not drawn", () => {
    expect(rowActions(entry({ category: "failed_build" }, []))).toEqual(["snooze", "done"]);
  });
});

describe("the job a row belongs to", () => {
  const reach = (ids: string[]): Reached[] => ids.map((entity_id) => ({ entity_id, kind: "pr", step: entity_id, origin: { kind: "own" } }));
  const index = jobIndex([
    { id: "jira:PAY-231", reached: reach(["gitea:payouts#142", "tc:Payout#1187"]) },
    { id: "jira:PAY-236", reached: reach(["gitea:payouts#142", "gitea:payouts#150"]) },
  ]);

  test("is the first job whose reach holds the item", () => {
    expect(index.jobOf(item({}))).toBe("PAY-231");
    expect(index.jobOf(item({ entity_id: "gitea:payouts#150" }))).toBe("PAY-236");
  });
  test("jobIdOf is the owning job's entity id, the ticket's own for a ticket", () => {
    expect(index.jobIdOf(item({}))).toBe("jira:PAY-231");
    expect(index.jobIdOf(item({ entity_id: "gitea:payouts#150" }))).toBe("jira:PAY-236");
    expect(index.jobIdOf(item({ entity_id: "gitea:PAY-198", kind: "ticket" }))).toBe("gitea:PAY-198");
    expect(index.jobIdOf(item({ entity_id: "gitea:other#1" }))).toBeNull();
  });
  test("is the ticket itself for an item about a ticket", () => {
    expect(index.jobOf(item({ entity_id: "jira:PAY-198", kind: "ticket" }))).toBe("PAY-198");
  });
  test("is none for what no job reaches", () => {
    expect(index.jobOf(item({ entity_id: "gitea:other#1" }))).toBeNull();
    expect(index.jobOf(item({ entity_id: null, kind: null }))).toBeNull();
  });
  test("a job holds its own ticket and its reach", () => {
    expect(index.holds("PAY-231", item({ entity_id: "jira:PAY-231", kind: "ticket" }))).toBe(true);
    expect(index.holds("PAY-231", item({ entity_id: "gitea:payouts#150" }))).toBe(false);
    expect(index.holds("PAY-236", item({ entity_id: "gitea:payouts#150" }))).toBe(true);
    expect(index.holds("PAY-999", item({}))).toBe(false);
  });
});

describe("the filter", () => {
  const rows = [
    entry({ key: "a", entity_id: "gitea:payouts#142" }),
    entry({ key: "b", category: "failed_build", entity_id: "tc:x#1", kind: "build", title: "x #1", reason: "this build failed" }),
    entry({ key: "c", category: "mention", entity_id: "jira:PAY-198", kind: "ticket", title: "Ledger drift", reason: "you are mentioned here" }),
  ];
  const index = jobIndex([{ id: "jira:PAY-231", reached: [{ entity_id: "gitea:payouts#142" }] }]);
  const keys = (list: InboxEntry[]) => list.map((row) => row.item.key);

  test("All with nothing set keeps every row", () => {
    expect(keys(visible(rows, { scope: "all", job: null, kinds: [] }, index))).toEqual(["a", "b", "c"]);
  });
  test("This job keeps what the job holds", () => {
    expect(keys(visible(rows, { scope: "job", job: "PAY-231", kinds: [] }, index))).toEqual(["a"]);
  });
  test("kind chips mean any of them", () => {
    expect(keys(visible(rows, { scope: "all", job: null, kinds: ["Build", "Mention"] }, index))).toEqual(["b", "c"]);
  });
});

test("a snoozed row says when it comes back in the words a credential's sentence dates itself in", () => {
  expect(backLabel({ snoozed_until: new Date(2026, 7, 29, 9).toISOString() })).toBe("Saturday 29 Aug 2026");
  expect(backLabel({ snoozed_until: null })).toBeNull();
});

test("a credential row says its fix once the sources are read, and opens the prompt", () => {
  const row = item({ category: "failed_sign_in", source_id: "kuma", entity_id: null, kind: null });
  expect(fixOf(row, () => "Enter token")).toEqual({ label: "Enter token…", href: "#/settings/source/kuma/reenter" });
  expect(fixOf(row, () => null)).toBeNull();
  // The sentence is the link, and it opens the source's page whatever the fix is called.
  expect(opener(row)).toEqual({ label: "Source settings", href: "#/settings/source/kuma" });
  // Only a credential row has a fix.
  expect(fixOf(item({ category: "failed_build" }), () => "Enter token")).toBeNull();
});

test("a day in the time column's room has its weekday, and its year only when it is not this year's", () => {
  const now = new Date(2026, 10, 6, 9);
  expect(shortDay(new Date(2026, 10, 4, 10, 56), now)).toBe("Wed 4 Nov");
  expect(shortDay(new Date(2025, 11, 31), now)).toBe("Wed 31 Dec 2025");
});

test("a snooze's toast names the day within a week, the date beyond", () => {
  const now = new Date(2026, 7, 21, 15, 0); // a Friday
  expect(untilLabel(new Date(2026, 7, 24, 9, 0), now)).toBe("Monday 09:00");
  expect(untilLabel(new Date(2026, 8, 4, 9, 0), now)).toBe("2026-09-04 09:00");
});

test("a run of a kind the CI adapters claim opens Builds' run page (#1278)", () => {
  const spy = vi.spyOn(kindRegistry, "frameKinds").mockReturnValue(["build", "workflow_run"]);
  try {
    expect(opener(item({ category: "mention", entity_id: "forge:run#77", kind: "workflow_run" }))?.href).toBe("#/build/forge%3Arun%2377");
  } finally {
    spy.mockRestore();
  }
});
