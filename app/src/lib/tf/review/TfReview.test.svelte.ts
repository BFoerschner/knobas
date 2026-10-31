/**
 * Proposed links (#835, R-6.3, R-6.4), mounted over a stand-in tray: the
 * queue, the open proposal, the keys, and *Undo*. What the backend does with
 * `accept_suggestion` and `dismiss_suggestion` is
 * `crates/knobas-app/tests/it/`'s; here the writes are counted.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { LinkEnd, LinkRow, ProposalEvidence, SuggestionEntry } from "../../ipc/entity";
import { toasts } from "../../shell/toasts.svelte";
import { createLayers, type Layers } from "../layers.svelte";
import { createDecisions, type Decisions } from "./decisions.svelte";
import TfReview from "./TfReview.svelte";

function end(entity_id: string, kind: string, title: string): LinkEnd {
  return { entity_id, kind, title, deleted_at: null };
}

const TICKET = end("jira:PAY-231", "ticket", "Retry failed SEPA payouts");

function proposal(
  id: string,
  from: LinkEnd,
  to: LinkEnd,
  rule_class: "exact_key" | "similarity",
  reason: string,
  created_at: string,
): SuggestionEntry {
  return {
    link: {
      id,
      from_id: from.entity_id,
      to_id: to.entity_id,
      relation: "related",
      origin: "suggested",
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

const ROWS = [
  proposal("l1", end("gitea:9f2c1ab", "commit", "key the retry on the mandate id"), TICKET, "exact_key",
    "the commit message mentions PAY-231", "2026-08-22T12:00:00Z"),
  proposal("l2", end("confluence:ENG-SEPA", "page", "SEPA retry design"), TICKET, "similarity",
    "both mention 409, fail, idempotent, payout", "2026-08-22T11:00:00Z"),
  proposal("l3", end("confluence:ARCH", "page", "Payments architecture overview"), end("gitea:ledger", "repo", "tidewater/ledger"),
    "exact_key", "this page mentions tidewater/ledger", "2026-08-22T10:00:00Z"),
];

/** What `proposal_evidence` answers for l1 and l2; l3 has none, so its read fails. */
const EVIDENCE: Record<string, ProposalEvidence> = {
  l1: {
    link_id: "l1",
    reason: "the commit message mentions PAY-231",
    excerpts: [
      {
        entity_id: "gitea:9f2c1ab",
        parts: [
          { text: "…is now a no-op. Refs ", marked: false },
          { text: "PAY-231", marked: true },
          { text: ".", marked: false },
        ],
      },
    ],
    support: ["Found the same way and already linked to PAY-231: c90d11 “PAY-231 retry once”."],
    against: ["Nothing found against it."],
  },
  l2: {
    link_id: "l2",
    reason: "both mention 409, fail, idempotent, payout",
    excerpts: [
      { entity_id: "confluence:ENG-SEPA", parts: [{ text: "the ", marked: false }, { text: "payout", marked: true }] },
      { entity_id: "jira:PAY-231", parts: [{ text: "Retry failed SEPA ", marked: false }, { text: "payouts", marked: true }] },
    ],
    support: [],
    against: ["Wording alone often matches unrelated items."],
  },
};

let evidenceReads: string[];
let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;
let props: { ready: boolean; job: string | null; sid: string | null; layers: Layers; decisions: Decisions; ports: { roomSuggestions: () => Promise<{ rows: SuggestionEntry[]; total: number; more: [] }>; proposalEvidence: (id: string) => Promise<ProposalEvidence> }; ongo: (hash: string) => void };
let writes: string[];

beforeEach(() => {
  toasts.items.length = 0;
  writes = [];
  evidenceReads = [];
  target = document.createElement("div");
  document.body.appendChild(target);
});

afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
});

async function settle() {
  for (let turn = 0; turn < 8; turn += 1) await new Promise((resolve) => setTimeout(resolve, 0));
  flushSync();
}

/** A decisions store whose held writes run when the test says so. */
function heldDecisions(): Decisions {
  const ports = {
    acceptSuggestion: async (id: string) => void writes.push(`accept ${id}`),
    dismissSuggestion: async (id: string) => void writes.push(`dismiss ${id}`),
    createLink: async (from: string, to: string, relation?: string) => {
      writes.push(`create ${from} ${to} ${relation}`);
      return ROWS[0]!.link as LinkRow;
    },
  };
  return createDecisions(ports, 60_000);
}

async function open(over: { job?: string | null; sid?: string | null } = {}) {
  const fresh = $state({
    ready: true,
    job: over.job === undefined ? null : over.job,
    sid: over.sid ?? null,
    layers: createLayers(),
    decisions: heldDecisions(),
    ports: {
      roomSuggestions: async () => ({ rows: ROWS, total: ROWS.length, more: [] as [] }),
      proposalEvidence: async (id: string) => {
        evidenceReads.push(id);
        const found = EVIDENCE[id];
        if (!found) throw { code: "not_found", message: `${id} is not a link`, source_id: null };
        return found;
      },
    },
    ongo: (hash: string) => {
      const id = hash.replace(/^#\/review\/?/, "");
      props.sid = id === "" ? null : decodeURIComponent(id);
    },
  });
  props = fresh;
  view = mount(TfReview, { target, props });
  await settle();
}

const rows = () => [...target.querySelectorAll(".tf-ql a")].map((row) => row.textContent?.replace(/\s+/g, " ").trim());
const heading = () => target.querySelector(".tf-review h1")?.textContent;
const names = () => [...target.querySelectorAll(".tf-pend .tf-name")].map((name) => name.textContent);
const key = (k: string) => window.dispatchEvent(new KeyboardEvent("keydown", { key: k, bubbles: true }));
function button(text: string): HTMLButtonElement {
  const found = [...target.querySelectorAll("button")].find((candidate) => candidate.textContent?.includes(text));
  if (!found) throw new Error(`no button ${text}`);
  return found as HTMLButtonElement;
}

test("the queue lists each proposal by its item, strength and target, newest first, and opens the first", async () => {
  await open();
  expect(rows()).toEqual([
    "9f2c1ab “key the retry on the mandate id” strong, to PAY-231",
    "SEPA retry design weak, to PAY-231",
    "Payments architecture overview strong, to tidewater/ledger",
  ]);
  expect(heading()).toBe("Is this a real connection?");
  expect(names()).toEqual(["9f2c1ab “key the retry on the mandate id”", "PAY-231 Retry failed SEPA payouts"]);
  expect(target.querySelector(".tf-claim")?.textContent).toBe("The commit message mentions PAY-231.");
  expect(target.querySelector(".tf-review-top .tf-dim")?.textContent).toBe("1 of 3, about PAY-231");
  expect(target.querySelector(".tf-review-top .tf-dim a")?.getAttribute("href")).toBe("#/job/jira%3APAY-231");
  expect(target.querySelector(".tf-ql a[aria-current='true']")?.getAttribute("href")).toBe("#/review/l1");
});

test("the three decisions are the screen's one large decision (design system, Controls; #2280)", async () => {
  await open();
  for (const text of ["Link them", "Not related", "Decide later"]) {
    expect(button(text).classList.contains("tf-lg"), text).toBe(true);
  }
});

test("the address opens the proposal it names", async () => {
  await open({ sid: "l2" });
  expect(names()).toEqual(["SEPA retry design", "PAY-231 Retry failed SEPA payouts"]);
  expect(target.querySelector(".tf-strength")?.textContent).toBe("Weak");
});

test("Y links it after the undo window, opens the next, and the toast says what was linked", async () => {
  await open();
  key("y");
  flushSync();
  expect(props.sid).toBe("l2");
  expect(names()[0]).toBe("SEPA retry design");
  expect(toasts.items.at(-1)?.text).toBe("Linked: 9f2c1ab related PAY-231");
  expect(toasts.items.at(-1)?.action?.label).toBe("Undo");
  expect(rows()).toHaveLength(2);
  expect(writes).toEqual([]);
});

test("N dismisses, and the toast says it is remembered", async () => {
  await open();
  key("n");
  flushSync();
  expect(toasts.items.at(-1)?.text).toBe("Dismissed. It will not be proposed again.");
  expect(props.sid).toBe("l2");
});

test("S decides later: no toast, the next opens", async () => {
  await open();
  key("s");
  flushSync();
  expect(toasts.items).toHaveLength(0);
  expect(props.sid).toBe("l2");
});

test("Undo takes the decision back and opens that proposal again", async () => {
  await open();
  key("y");
  flushSync();
  toasts.items.at(-1)!.action!.run();
  flushSync();
  expect(props.sid).toBe("l1");
  expect(rows()).toHaveLength(3);
  expect(target.querySelector(".tf-ql a.tf-decided")).toBeNull();
});

test("Decided brings decided proposals back into the queue, marked", async () => {
  await open();
  key("n");
  flushSync();
  button("Decided").click();
  flushSync();
  expect(rows()[0]).toBe("9f2c1ab “key the retry on the mandate id” not related, strong, to PAY-231");
  expect(target.querySelector(".tf-ql a.tf-decided")?.getAttribute("href")).toBe("#/review/l1");
});

test("the keys do nothing while a text field has focus or a dialog is open", async () => {
  await open();
  const field = target.querySelector<HTMLInputElement>("input[data-filter]")!;
  field.focus();
  key("y");
  field.blur();
  props.layers.add({ kind: "dialog", close: () => {} });
  key("y");
  flushSync();
  expect(props.sid).toBeNull();
  expect(toasts.items).toHaveLength(0);
});

test("a changed relation is the one linked", async () => {
  await open();
  const select = target.querySelector<HTMLSelectElement>(".tf-bridge select")!;
  select.value = "implements";
  select.dispatchEvent(new Event("change", { bubbles: true }));
  flushSync();
  button("Link them").click();
  flushSync();
  expect(toasts.items.at(-1)?.text).toBe("Linked: 9f2c1ab implements PAY-231");
});

test("This job keeps the job's proposals, and when they are done it offers all", async () => {
  await open({ job: "PAY-231" });
  expect(target.querySelector(".tf-seg button[aria-pressed='true']")?.textContent).toBe("This job PAY-231");
  expect(rows()).toHaveLength(2);
  key("n");
  flushSync();
  key("n");
  flushSync();
  expect(heading()).toBe("Nothing left to judge for PAY-231");
  button("Show all proposals").click();
  flushSync();
  expect(names()).toEqual(["Payments architecture overview", "tidewater/ledger"]);
});

test("with nothing proposed, the view says what would fill it", async () => {
  await open();
  key("n");
  flushSync();
  key("n");
  flushSync();
  key("n");
  flushSync();
  expect(props.sid).toBeNull();
  expect(heading()).toBe("Nothing left to judge");
  expect(target.textContent).toContain("Dismissed ones stay dismissed.");
});

const quotes = () => [...target.querySelectorAll(".tf-evidence blockquote")];
const said = (heading: string) =>
  [...target.querySelectorAll(".tf-against")]
    .find((block) => block.querySelector("strong")?.textContent === heading)
    ?.querySelectorAll("span");

test("the evidence quotes the item with the key marked, then what supports it and what speaks against it", async () => {
  await open();
  expect(quotes().map((quote) => quote.textContent?.trim())).toEqual(["…is now a no-op. Refs PAY-231."]);
  expect([...quotes()[0]!.querySelectorAll("mark")].map((mark) => mark.textContent)).toEqual(["PAY-231"]);
  expect(quotes()[0]!.classList.contains("tf-mono"), "a commit message is prose").toBe(false);
  expect(target.querySelector(".tf-quotes")?.classList.contains("tf-sbs")).toBe(false);
  expect([...said("Also found")!].map((line) => line.textContent)).toEqual([
    "Found the same way and already linked to PAY-231: c90d11 “PAY-231 retry once”.",
  ]);
  expect([...said("Speaks against it")!].map((line) => line.textContent)).toEqual(["Nothing found against it."]);
});

test("shared wording is quoted from both ends side by side, and with no support there is no Also found", async () => {
  await open({ sid: "l2" });
  expect(target.querySelector(".tf-quotes")?.classList.contains("tf-sbs")).toBe(true);
  expect(quotes().flatMap((quote) => [...quote.querySelectorAll("mark")].map((mark) => mark.textContent))).toEqual([
    "payout",
    "payouts",
  ]);
  expect(quotes()[0]!.classList.contains("tf-mono"), "a page is prose").toBe(false);
  expect(said("Also found")).toBeUndefined();
  expect([...said("Speaks against it")!].map((line) => line.textContent)).toEqual([
    "Wording alone often matches unrelated items.",
  ]);
});

test("each proposal's evidence is read once, when it opens", async () => {
  await open();
  expect(evidenceReads).toEqual(["l1"]);
  props.sid = "l2";
  await settle();
  props.sid = "l1";
  await settle();
  expect(evidenceReads).toEqual(["l1", "l2"]);
});

test("evidence that cannot be read says why, and the reason still stands", async () => {
  await open({ sid: "l3" });
  expect(target.querySelector(".tf-claim")?.textContent).toBe("This page mentions tidewater/ledger.");
  expect(target.querySelector(".tf-evidence .tf-dim")?.textContent).toBe("l3 is not a link");
  expect(said("Speaks against it")).toBeUndefined();
});
