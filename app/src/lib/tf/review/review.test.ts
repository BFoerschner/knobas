/**
 * The Proposed links view's rules (#835, R-6.3, R-6.4): what a proposal is
 * called, how strong it reads, which ones the queue keeps and which one opens
 * after a decision.
 */
import { expect, test } from "vitest";

import type { LinkEnd, SuggestionEntry } from "../../ipc/entity";
import {
  claim,
  currentOf,
  endName,
  linkedToast,
  nextAfter,
  queue,
  relationOptions,
  rowNote,
  strengthOf,
  targetOf,
  type Decided,
  type QueueFilter,
} from "./review";

function end(entity_id: string, kind: string, title: string): LinkEnd {
  return { entity_id, kind, title, deleted_at: null };
}

const TICKET = end("jira:PAY-231", "ticket", "Retry failed SEPA payouts");
const OTHER_TICKET = end("jira:PAY-240", "ticket", "Payout dashboard latency");
const COMMIT = end("gitea:9f2c1ab", "commit", "payout: key the retry on the mandate id");
const PAGE = end("confluence:ENG-SEPA", "page", "SEPA retry design");
const REPO = end("gitea:tidewater/ledger", "repo", "tidewater/ledger");

function proposal(
  id: string,
  from: LinkEnd,
  to: LinkEnd,
  rule_class: "exact_key" | "similarity" | "source_relation",
  reason: string,
  created_at = "2026-08-22T10:00:00Z",
  relation = "related",
): SuggestionEntry {
  return {
    link: {
      id,
      from_id: from.entity_id,
      to_id: to.entity_id,
      relation,
      origin: rule_class === "source_relation" ? "source" : "suggested",
      note: null,
      created_by: "knobas",
      created_at,
      confirmed_at: null,
      rule: "rule",
      rule_class,
      reason,
    },
    from,
    to,
  };
}

const COMMIT_KEY = proposal("l1", COMMIT, TICKET, "exact_key", "the commit message mentions PAY-231", "2026-08-22T12:00:00Z");
const SIMILAR = proposal("l2", PAGE, TICKET, "similarity", "both mention 409, fail, idempotent, payout", "2026-08-22T11:00:00Z");
const ELSEWHERE = proposal("l3", PAGE, REPO, "exact_key", "this page mentions tidewater/ledger", "2026-08-22T10:00:00Z");
const RECORDED = proposal("l4", OTHER_TICKET, TICKET, "source_relation", "jira already records this link (blocks)", "2026-08-22T09:00:00Z", "blocks");
const ALL = [COMMIT_KEY, SIMILAR, ELSEWHERE, RECORDED];

const OPEN: QueueFilter = { scope: "all", job: null, strengths: [], decided: false, text: "" };

test("an exact key and a relation the source records read strong; shared wording reads weak", () => {
  expect(strengthOf(COMMIT_KEY.link)).toBe("strong");
  expect(strengthOf(RECORDED.link)).toBe("strong");
  expect(strengthOf(SIMILAR.link)).toBe("weak");
});

test("a ticket is named by its key and title, a commit by its sha and quoted message, anything else by its title", () => {
  expect(endName(TICKET)).toBe("PAY-231 Retry failed SEPA payouts");
  expect(endName(COMMIT)).toBe("9f2c1ab “payout: key the retry on the mandate id”");
  expect(endName(PAGE)).toBe("SEPA retry design");
  expect(endName(REPO)).toBe("tidewater/ledger");
});

test("the queue row names the item and points at the ticket, whichever end the ticket is", () => {
  expect(targetOf(COMMIT_KEY)).toEqual({ item: COMMIT, target: TICKET });
  const reversed = proposal("r", TICKET, COMMIT, "exact_key", "x");
  expect(targetOf(reversed)).toEqual({ item: COMMIT, target: TICKET });
  expect(targetOf(ELSEWHERE)).toEqual({ item: PAGE, target: REPO });
});

test("a queue row says the decision, the strength and the target", () => {
  expect(rowNote(COMMIT_KEY, undefined)).toBe("strong, to PAY-231");
  expect(rowNote(COMMIT_KEY, "yes")).toBe("linked, strong, to PAY-231");
  expect(rowNote(SIMILAR, "no")).toBe("not related, weak, to PAY-231");
  expect(rowNote(ELSEWHERE, "skip")).toBe("later, strong, to tidewater/ledger");
});

test("the queue keeps every undecided proposal, newest first", () => {
  const shuffled = [RECORDED, ELSEWHERE, COMMIT_KEY, SIMILAR];
  expect(queue(shuffled, {}, OPEN, null).map((row) => row.link.id)).toEqual(["l1", "l2", "l3", "l4"]);
});

test("This job keeps the proposals with the job's ticket at one end", () => {
  const ids = queue(ALL, {}, { ...OPEN, scope: "job", job: "PAY-231" }, null).map((row) => row.link.id);
  expect(ids).toEqual(["l1", "l2", "l4"]);
});

test("strength chips keep any of the strengths switched on", () => {
  expect(queue(ALL, {}, { ...OPEN, strengths: ["weak"] }, null).map((row) => row.link.id)).toEqual(["l2"]);
  expect(queue(ALL, {}, { ...OPEN, strengths: ["indirect"] }, null)).toEqual([]);
});

test("the text filter reads both ends' names and the reason", () => {
  expect(queue(ALL, {}, { ...OPEN, text: "idempotent" }, null).map((row) => row.link.id)).toEqual(["l2"]);
  expect(queue(ALL, {}, { ...OPEN, text: "ledger 409" }, null).map((row) => row.link.id)).toEqual([]);
  expect(queue(ALL, {}, { ...OPEN, text: "ledger mentions" }, null).map((row) => row.link.id)).toEqual(["l3"]);
});

test("decided proposals leave the queue, except the one open, and Decided brings them all back", () => {
  const decided: Decided = { l1: "yes", l2: "skip" };
  expect(queue(ALL, decided, OPEN, null).map((row) => row.link.id)).toEqual(["l3", "l4"]);
  expect(queue(ALL, decided, OPEN, "l2").map((row) => row.link.id)).toEqual(["l2", "l3", "l4"]);
  expect(queue(ALL, decided, { ...OPEN, decided: true }, null).map((row) => row.link.id)).toEqual(["l1", "l2", "l3", "l4"]);
});

test("the open proposal is the addressed one, else the first undecided", () => {
  expect(currentOf(ALL, {}, "l3")?.link.id).toBe("l3");
  expect(currentOf(ALL, { l1: "no" }, null)?.link.id).toBe("l2");
  expect(currentOf(ALL, { l1: "no" }, "gone")?.link.id).toBe("l2");
  expect(currentOf(ALL, { l1: "no", l2: "no", l3: "no", l4: "yes" }, null)).toBeNull();
});

test("after a decision the next undecided proposal opens, wrapping to the top", () => {
  expect(nextAfter(ALL, { l1: "yes" }, "l1")?.link.id).toBe("l2");
  expect(nextAfter(ALL, { l3: "no" }, "l3")?.link.id).toBe("l4");
  expect(nextAfter(ALL, { l4: "no" }, "l4")?.link.id).toBe("l1");
  expect(nextAfter(ALL, { l1: "no", l2: "skip", l3: "no", l4: "yes" }, "l4")).toBeNull();
});

test("the reason reads as one sentence", () => {
  expect(claim("the commit message mentions PAY-231")).toBe("The commit message mentions PAY-231.");
  expect(claim("Already a sentence.")).toBe("Already a sentence.");
});

test("Link them says what was linked, by the first word of each end", () => {
  expect(linkedToast(COMMIT_KEY, "implements")).toBe("Linked: 9f2c1ab implements PAY-231");
});

test("the relation offers prototype-6's list, and keeps a relation the proposal carries that the list lacks", () => {
  expect(relationOptions("related").map((option) => option.label)).toEqual([
    "implements",
    "deploys",
    "documents",
    "mentions",
    "related",
    "blocks",
    "depends on",
  ]);
  expect(relationOptions("affects")[0]).toEqual({ id: "affects", label: "affects" });
});
