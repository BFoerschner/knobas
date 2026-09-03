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

import type { Candidate, Draft, Worklog } from "../ipc/time";
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
  logWork: (...args: unknown[]) => Promise<Worklog> = () => Promise.resolve(logged()),
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
      logWork: ((
        entityId: string,
        day: string,
        offsetMinutes: number,
        startedAt: string,
        seconds: number,
        comment: string,
      ) => {
        sent.push({ entityId, day, offsetMinutes, startedAt, seconds, comment });
        return logWork(entityId, day, offsetMinutes, startedAt, seconds, comment);
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
  expect(sent[0]!.offsetMinutes).toBe(-new Date().getTimezoneOffset());

  await vi.waitFor(() => expect(onlogged).toHaveBeenCalledTimes(1));
  expect(onclose).toHaveBeenCalledTimes(1);
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
