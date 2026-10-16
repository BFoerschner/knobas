/**
 * The Inbox's pure half (#834, R-12.1 to R-12.3): what a row says, where it
 * opens, which job it belongs to, and which rows the filter keeps.
 */
import { describe, expect, test } from "vitest";

import type { InboxEntry, InboxItem, Reached } from "../../ipc/entity";
import {
  jobIndex,
  kindOf,
  opener,
  rowActions,
  sentence,
  snoozedLine,
  untilLabel,
  visible,
  when,
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
  test("is the backend's reason, capitalised, then the item", () => {
    expect(sentence(item({ category: "new_assignment", reason: "assigned to you", title: "Payout dashboard latency" }))).toBe(
      "Assigned to you: Payout dashboard latency",
    );
  });

  test("keeps a source's word as the source spells it", () => {
    expect(sentence(item({}))).toBe("jonas asked for your review: Idempotent retry window");
    expect(sentence(item({ category: "alert", reason: "knobas test estate / knobas-jira is down", title: "jira" }))).toBe(
      "knobas test estate / knobas-jira is down",
    );
  });

  test("capitalises the backend's stand-in for a review with no author", () => {
    expect(sentence(item({ reason: "somebody asked for your review" }))).toBe(
      "Somebody asked for your review: Idempotent retry window",
    );
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
});

test("the kind word is the chip's word for each category", () => {
  expect(kindOf("review_request")).toBe("Review");
  expect(kindOf("mention")).toBe("Mention");
  expect(kindOf("failed_build")).toBe("Build");
  expect(kindOf("new_assignment")).toBe("Assignment");
  expect(kindOf("alert")).toBe("Monitor");
  expect(kindOf("credential_expiry")).toBe("Credential");
});

describe("the time column", () => {
  const now = new Date(2026, 7, 22, 15, 0);
  test("today is the clock", () => {
    expect(when(new Date(2026, 7, 22, 13, 50).toISOString(), now)).toBe("13:50");
  });
  test("yesterday is shortened as prototype-6 does", () => {
    expect(when(new Date(2026, 7, 21, 16, 30).toISOString(), now)).toBe("yest. 16:30");
  });
  test("older is the date", () => {
    expect(when(new Date(2026, 7, 18, 9, 5).toISOString(), now)).toBe("2026-08-18");
  });
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
  test("an alert opens Monitors", () => {
    expect(opener(item({ category: "alert", entity_id: "asset:gitea", kind: "asset" }))).toEqual({
      label: "Open monitor",
      href: "#/monitors",
    });
  });
  test("an assignment opens the job", () => {
    expect(opener(item({ category: "new_assignment", entity_id: "jira:PAY-240", kind: "ticket" }))).toEqual({
      label: "Open job",
      href: "#/job/PAY-240",
    });
  });
  test("a mention opens where it was said", () => {
    expect(opener(item({ category: "mention", entity_id: "jira:PAY-231", kind: "ticket" }))).toEqual({
      label: "Open conversation",
      href: "#/job/PAY-231/conversation",
    });
    expect(opener(item({ category: "mention", entity_id: "conf:SEPA", kind: "page" }))).toEqual({
      label: "Open page",
      href: "#/page/conf%3ASEPA",
    });
    expect(opener(item({ category: "mention", kind: "pr" }))?.label).toBe("Open PR");
  });
  test("a credential expiry opens its source's settings", () => {
    expect(opener(item({ category: "credential_expiry", source_id: "jira", entity_id: null, kind: null }))).toEqual({
      label: "Open source",
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
    expect(keys(visible(rows, { scope: "all", job: null, kinds: [], text: "" }, index))).toEqual(["a", "b", "c"]);
  });
  test("This job keeps what the job holds", () => {
    expect(keys(visible(rows, { scope: "job", job: "PAY-231", kinds: [], text: "" }, index))).toEqual(["a"]);
  });
  test("kind chips mean any of them", () => {
    expect(keys(visible(rows, { scope: "all", job: null, kinds: ["Build", "Mention"], text: "" }, index))).toEqual(["b", "c"]);
  });
  test("text matches the sentence and the job key", () => {
    expect(keys(visible(rows, { scope: "all", job: null, kinds: [], text: "ledger" }, index))).toEqual(["c"]);
    expect(keys(visible(rows, { scope: "all", job: null, kinds: [], text: "pay-231" }, index))).toEqual(["a"]);
  });
});

test("the snoozed line names each item and its return date", () => {
  const shelf = [
    entry({ category: "credential_expiry", entity_id: null, kind: null, title: "Jira", reason: "the Jira credential expires on Monday 7 Sep 2026", snoozed_until: new Date(2026, 7, 29, 9).toISOString() }),
    entry({ title: "Idempotent retry window", snoozed_until: new Date(2026, 8, 1, 9).toISOString() }),
  ];
  expect(snoozedLine(shelf)).toBe(
    "Snoozed: the Jira credential expires on Monday 7 Sep 2026 (back 2026-08-29), jonas asked for your review: Idempotent retry window (back 2026-09-01).",
  );
  expect(snoozedLine([])).toBeNull();
});

test("a snooze's toast names the day within a week, the date beyond", () => {
  const now = new Date(2026, 7, 21, 15, 0); // a Friday
  expect(untilLabel(new Date(2026, 7, 24, 9, 0), now)).toBe("Monday 09:00");
  expect(untilLabel(new Date(2026, 8, 4, 9, 0), now)).toBe("2026-09-04 09:00");
});
