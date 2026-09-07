/**
 * The worklog draft: what it offers, what it sends, and the two things it must
 * never do (#280).
 *
 * The load-bearing claims are about **the comment**, because that is the only
 * value on this surface the reader composes rather than reads: unticking a box
 * must take its line out, and typing must take the comment over for good. A
 * checkbox that silently rewrote a sentence somebody wrote is the failure this
 * component is shaped around.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { Candidate, Draft, LoggedWork, ReaderDay, Worklog } from "../ipc/time";
import { paste } from "../shell/test-paste";
import { offsetMinutes } from "./draft";
import WorklogDraft from "./WorklogDraft.svelte";

const TICKET = "jira:PAY-231";

function candidate(id: string, bullet: string): Candidate {
  return {
    id,
    source: "mirror",
    at: "2026-09-03T09:30:00Z",
    entity_id: TICKET,
    bullet,
  };
}

/** 09:00–14:00 with a lunch in it: two and a half hours to log. */
const DRAFT: Draft = {
  entity_id: TICKET,
  day: "2026-09-03",
  started_at: "2026-09-03T09:00:00Z",
  ended_at: "2026-09-03T14:00:00Z",
  seconds: 150 * 60,
  block_ids: [7, 8],
  candidates: [
    candidate("item:jira:c1", "- Retry SEPA payouts"),
    candidate("activity:41", "- linked PAY-231"),
  ],
  comment: "- Retry SEPA payouts\n- linked PAY-231",
};

/** What `log_work` was called with, in order. */
type Sent = {
  entityId: string;
  day: string;
  offsetMinutes: number;
  startedAt: string;
  seconds: number;
  comment: string;
};

let target: HTMLDivElement;
let app: Record<string, unknown> | undefined;

function logged(id = 3): Worklog {
  return {
    id,
    entity_id: TICKET,
    started_at: DRAFT.started_at,
    seconds: DRAFT.seconds,
    comment: DRAFT.comment,
    block_ids: DRAFT.block_ids,
    write_queue_id: 11,
    remote_id: "30007",
    created_at: "2026-09-03T14:01:00Z",
  };
}

function render(
  draft: Draft = DRAFT,
  logWork: () => Promise<Worklog> = () => Promise.resolve(logged()),
) {
  const sent: Sent[] = [];
  const onclose = vi.fn();
  const onlogged = vi.fn();
  app = mount(WorklogDraft, {
    target,
    props: {
      draft,
      onclose,
      onlogged,
      logWork: ((entityId: string, when: ReaderDay, work: LoggedWork) => {
        sent.push({ entityId, ...when, ...work });
        return logWork();
      }) as never,
    },
  });
  flushSync();
  return { sent, onclose, onlogged };
}

function boxes(): HTMLInputElement[] {
  return [...target.querySelectorAll<HTMLInputElement>(".cands input[type=checkbox]")];
}

function commentBox(): HTMLTextAreaElement {
  const box = target.querySelector<HTMLTextAreaElement>("textarea");
  expect(box, "the draft has no comment field").not.toBeNull();
  return box!;
}

function type(text: string) {
  const box = commentBox();
  box.value = text;
  box.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
}

/** The *Log …* button. */
function logButton(): HTMLButtonElement {
  const button = [...target.querySelectorAll<HTMLButtonElement>("button")].find((b) =>
    (b.textContent ?? "").trim().startsWith("Log "),
  );
  expect(button, "the draft has no log button").toBeDefined();
  return button!;
}

beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
});

afterEach(() => {
  if (app) unmount(app);
  app = undefined;
  target.remove();
});

test("it opens on the interval, every candidate ticked, and the generated comment", () => {
  render();

  expect(boxes()).toHaveLength(2);
  expect(boxes().every((box) => box.checked)).toBe(true);
  expect(commentBox().value).toBe("- Retry SEPA payouts\n- linked PAY-231");
  // The time that will be logged, not the five-hour window it sat in.
  expect(logButton().textContent).toContain("2h 30m");
  expect(logButton().textContent).toContain("PAY-231");
});

/**
 * Unticking takes the line out — **and the line is the backend's own**.
 *
 * The assertion is on the remaining bullet's exact text, which is what makes
 * this a test of the join rather than of a format: a component that composed
 * its own line would pass an "it got shorter" assertion and send Jira a
 * sentence spelled differently from the one the draft opened with.
 */
test("unticking a candidate takes its bullet out of the comment", () => {
  render();

  boxes()[1]!.click();
  flushSync();

  expect(commentBox().value).toBe("- Retry SEPA payouts");

  boxes()[0]!.click();
  flushSync();
  expect(commentBox().value, "a worklog with no words is a worklog").toBe("");
});

/**
 * Once the reader has typed, the boxes stop rewriting the comment.
 *
 * The failure this exists for is silent: tick a box after writing a sentence
 * and the sentence is gone, with no undo and nothing on screen to say what
 * happened.
 */
test("a comment the reader has typed is never rewritten by a checkbox", () => {
  render();

  type("Pairing on the SEPA retry with Jonas.");
  boxes()[0]!.click();
  flushSync();

  expect(commentBox().value).toBe("Pairing on the SEPA retry with Jonas.");
  expect(target.textContent).toContain("The comment is yours now");
});

test("it sends the interval, the edited comment and the reader's own day", async () => {
  const { sent, onlogged, onclose } = render();

  boxes()[1]!.click();
  flushSync();
  logButton().click();
  await vi.waitFor(() => expect(sent).toHaveLength(1));

  expect(sent[0]).toMatchObject({
    entityId: TICKET,
    day: "2026-09-03",
    startedAt: "2026-09-03T09:00:00Z",
    seconds: 150 * 60,
    comment: "- Retry SEPA payouts",
  });
  // East of UTC is positive, which is the opposite of `getTimezoneOffset`.
  // Negating the platform's answer inline is what `offsetMinutes` exists to
  // stop: in a zero-offset zone it builds `-0`, and `toBe` is `Object.is`, so
  // the expectation would miss the `+0` the component correctly sends (#406).
  // The sign convention itself is pinned in `draft.test.ts` against fake
  // offsets, so reading it from the helper here costs no coverage.
  expect(sent[0]!.offsetMinutes).toBe(offsetMinutes());

  await vi.waitFor(() => expect(onlogged).toHaveBeenCalledTimes(1));
  expect(onclose).toHaveBeenCalledTimes(1);
});

/**
 * The interval is editable **as a whole** (#272, story 31): the start as well
 * as the length, because a timer started ten minutes after the work did is the
 * ordinary case.
 */
test("a corrected start is what is logged, on the same day", async () => {
  const { sent } = render();
  const began = target.querySelector<HTMLInputElement>("input[type=time]");
  expect(began, "the draft has no start field").not.toBeNull();

  began!.value = "08:30";
  began!.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();

  logButton().click();
  await vi.waitFor(() => expect(sent).toHaveLength(1));

  const at = new Date(sent[0]!.startedAt);
  expect(at.getHours()).toBe(8);
  expect(at.getMinutes()).toBe(30);
  expect(sent[0]!.day, "correcting a start must not move the day").toBe("2026-09-03");
});

/**
 * **An untouched start is sent verbatim, seconds and all.**
 *
 * The fixture above starts on a whole minute, so it cannot tell the two
 * readings apart; this one starts at 09:00:37. A time field has no seconds, so
 * a component that put every start through `atClock` would send 09:00:00 — and
 * a draft nobody edited would have quietly moved a fact knobas measured.
 */
test("a start nobody touched is sent exactly as the block recorded it", async () => {
  const measured = "2026-09-03T09:00:37.482Z";
  const { sent } = render({ ...DRAFT, started_at: measured });

  logButton().click();
  await vi.waitFor(() => expect(sent).toHaveLength(1));

  expect(sent[0]!.startedAt).toBe(measured);
});

/** An edited length is what is logged — the minutes field, not the blocks. */
test("the length can be corrected, and the button says what will be logged", async () => {
  const { sent } = render();
  const minutes = target.querySelector<HTMLInputElement>("input[type=number]")!;

  minutes.value = "60";
  minutes.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  expect(logButton().textContent).toContain("1h");

  logButton().click();
  await vi.waitFor(() => expect(sent).toHaveLength(1));
  expect(sent[0]!.seconds).toBe(3_600);
});

/**
 * A refusal is said, and the dialog **stays open**.
 *
 * A draft that closed on failure would leave the reader believing their day is
 * on the ticket when the blocks are still sitting there unlogged.
 */
/**
 * **A length nobody touched is sent as the blocks measured it.**
 *
 * The fixture above sums to a whole number of minutes, so it cannot tell the
 * two readings apart; this one sums to 2h 30m and 37s. The minutes field
 * cannot hold that second, and a component that read every length back out of
 * the field would log 9000s while marking blocks worth 9037s spent — the same
 * failure as a rounded start, in the number the reader is billed on.
 */
test("a length nobody touched is sent as the blocks measured it", async () => {
  const measured = 150 * 60 + 37;
  const { sent } = render({ ...DRAFT, seconds: measured });

  logButton().click();
  await vi.waitFor(() => expect(sent).toHaveLength(1));

  expect(sent[0]!.seconds).toBe(measured);
});

/**
 * **A worklog with no words is a worklog**, all the way to the call: unticking
 * everything empties the comment, and the empty string is what goes.
 *
 * The sibling test above stops at the textarea's value; this one is the wire.
 * A component that fell back to the generated comment, or refused to send an
 * empty one, would pass that assertion and put words nobody chose on a ticket.
 */
test("a comment emptied down to nothing is sent empty", async () => {
  const { sent } = render();

  for (const box of boxes()) {
    box.click();
    flushSync();
  }
  expect(commentBox().value).toBe("");

  logButton().click();
  await vi.waitFor(() => expect(sent).toHaveLength(1));
  expect(sent[0]!.comment).toBe("");
});

test("a refused log is reported and leaves the draft standing", async () => {
  const { onclose, onlogged } = render(DRAFT, () =>
    Promise.reject(new Error("there is no unlogged time on jira:PAY-231 for 2026-09-03")),
  );

  logButton().click();
  await vi.waitFor(() => expect(target.textContent).toContain("no unlogged time"));

  expect(onclose, "the draft closed over a failure").not.toHaveBeenCalled();
  expect(onlogged).not.toHaveBeenCalled();
});

/**
 * ADR-0012's sentence, on the surface where a worklog is sent.
 *
 * A duplicated worklog is hours somebody bills twice, which is why the
 * guarantee is stated here and not only in the write-queue panel. The Rust
 * side (`commands/time.rs`) is what checks the wording against the ADR itself;
 * this checks it is rendered at all, whatever the draft holds.
 */
test("the draft states that a write may arrive twice", () => {
  render({ ...DRAFT, candidates: [], comment: "" });
  expect(target.textContent).toContain(
    "A write knobas was sending when it stopped may arrive twice.",
  );
});

/** With nothing seen, the comment is the reader's to write from empty. */
test("a draft with no candidates says so rather than showing an empty list", () => {
  render({ ...DRAFT, candidates: [], comment: "" });

  expect(boxes()).toHaveLength(0);
  expect(target.textContent).toContain("knobas saw nothing else in that time");
  expect(commentBox().value).toBe("");
});

/**
 * Spec #491 story 13. The comment goes to Jira as a worklog comment, so a URL
 * pasted into it has to arrive there as the link that was pasted. Only the
 * note body substitutes a `[[ref]]` for one (story 12,
 * `notes/NoteView.svelte`); a worklog comment that did would send Jira a
 * spelling only knobas can read.
 */
test("a URL pasted into the comment is left as text and is logged as typed", () => {
  const link = "https://jira.example/browse/PAY-231";
  const { sent } = render();

  expect(paste(commentBox(), link), "something took the paste over").toBe(false);

  type(`- Retry SEPA payouts\n- see ${link}`);
  logButton().click();
  flushSync();

  expect(sent).toHaveLength(1);
  expect(sent[0]!.comment).toBe(`- Retry SEPA payouts\n- see ${link}`);
});
