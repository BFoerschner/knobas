/**
 * The write queue panel and the state behind it (issue #42).
 *
 * What is asserted is what a reader can see and do: a write that needs a
 * decision is somewhere different from one that needs patience, a held write
 * shows both versions, the three choices reach the three commands — and,
 * above all, **nothing moves a held write on its own**. The engine has that
 * pinned in Rust twice over; this is the half that stops a convenience
 * affordance putting it back.
 */
import { flushSync, mount, unmount } from "svelte";
import { beforeEach, expect, test, vi } from "vitest";

import type { QueueCounts, QueuedWrite } from "../ipc/sources";

const calls: { command: string; args: unknown }[] = [];
let rows: QueuedWrite[] = [];
let counts: QueueCounts = { pending: 0, held: 0, refused: 0 };
/** Set to make the next command reject, so the error path is reachable. */
let reject: unknown = null;

function record<T>(command: string, args: unknown, value: T): Promise<T> {
  calls.push({ command, args });
  if (reject !== null) {
    const error = reject;
    reject = null;
    return Promise.reject(error);
  }
  return Promise.resolve(value);
}

vi.mock("../ipc/sources", () => ({
  pendingWrites: () => record("pending_writes", null, rows),
  writeQueueCounts: () => record("write_queue_counts", null, counts),
  flushWrites: (sourceId: string | null) => record("flush_writes", { sourceId }, undefined),
  applyHeldWrite: (id: number) => record("apply_held_write", { id }, undefined),
  discardWrite: (id: number) => record("discard_write", { id }, undefined),
  amendWrite: (id: number, payload: unknown) => record("amend_write", { id, payload }, undefined),
}));

const { default: WriteQueuePanel } = await import("./WriteQueue.svelte");
const {
  createWriteQueue,
  decisionsIn,
  demandOf,
  editableBody,
  readSnapshot,
  withBody,
} = await import("./write-queue.svelte");

const NOW = new Date("2026-08-29T12:00:00Z");

function write(over: Partial<QueuedWrite> = {}): QueuedWrite {
  return {
    id: 1,
    source_id: "jira",
    entity_id: "jira:PAY-231",
    op: "comment",
    payload: { Comment: { entity: "jira:PAY-231", body: "on it" } },
    target_snapshot: { op: "comment", live: true, text: "a payout fails" },
    state: "pending",
    wait_reason: "unreachable",
    detail: "connection timed out",
    queued_at: "2026-08-29T11:57:00Z",
    attempted_at: "2026-08-29T11:58:00Z",
    attempts: 1,
    held_snapshot: null,
    settled_at: null,
    ...over,
  };
}

function held(over: Partial<QueuedWrite> = {}): QueuedWrite {
  return write({
    id: 2,
    state: "held",
    wait_reason: null,
    detail: null,
    held_snapshot: {
      op: "comment",
      live: true,
      text: "a payout fails\n\njonas: already on it",
    },
    ...over,
  });
}

async function render() {
  const queue = createWriteQueue();
  await queue.refresh();
  const target = document.createElement("div");
  document.body.append(target);
  const app = mount(WriteQueuePanel, {
    target,
    props: { queue, now: NOW, onclose: () => {} },
  });
  flushSync();
  return {
    queue,
    text: () => (target.textContent ?? "").replace(/\s+/g, " "),
    button: (label: string) =>
      [...target.querySelectorAll("button")].find((b) =>
        (b.textContent ?? "").trim().startsWith(label),
      ),
    section: (heading: string) => {
      const h = [...target.querySelectorAll("h3")].find((node) =>
        (node.textContent ?? "").includes(heading),
      );
      return h?.closest("section") ?? null;
    },
    done: () => {
      unmount(app);
      target.remove();
    },
  };
}

beforeEach(() => {
  calls.length = 0;
  rows = [];
  counts = { pending: 0, held: 0, refused: 0 };
  reject = null;
});

// -- the vocabulary -----------------------------------------------------------

/**
 * The distinction the whole surface turns on: patience versus a decision. A
 * total switch, so a state added to the wire is a type error here rather than
 * a row that silently renders as patience.
 */
test("held and refused need a decision; pending needs patience", () => {
  expect(demandOf("pending")).toBe("waiting");
  expect(demandOf("held")).toBe("decide");
  expect(demandOf("refused")).toBe("decide");
  expect(demandOf("sent")).toBe("settled");
  expect(demandOf("discarded")).toBe("settled");
  expect(decisionsIn({ pending: 4, held: 1, refused: 2 })).toBe(3);
  expect(decisionsIn({ pending: 4, held: 0, refused: 0 })).toBe(0);
});

test("a snapshot is read per op, and says so when it cannot be", () => {
  expect(readSnapshot({ op: "comment", live: true, text: "hello" })).toEqual({
    text: "hello",
    live: true,
    raw: null,
  });
  // A target that is gone is not an unreadable snapshot -- it is a fact.
  expect(readSnapshot({ op: "comment", live: false, text: null })).toEqual({
    text: null,
    live: false,
    raw: null,
  });
  // A projection this build has no reader for is shown verbatim rather than
  // hidden: the reader is being asked to decide.
  const unknown = readSnapshot({ op: "transition", live: true, payload: { status: "done" } });
  expect(unknown.text).toBeNull();
  expect(unknown.raw).toContain("transition");
});

/**
 * `WriteOp` grows per milestone, so a build will meet a payload it cannot
 * edit. That must cost the *edit box* and nothing else.
 */
test("only a payload this build understands is editable", () => {
  expect(editableBody({ Comment: { entity: "jira:PAY-1", body: "words" } })).toBe("words");
  expect(editableBody({ Transition: { entity: "jira:PAY-1", to: "Done" } })).toBeNull();
  expect(editableBody(null)).toBeNull();

  expect(withBody({ Comment: { entity: "jira:PAY-1", body: "old" } }, "new")).toEqual({
    Comment: { entity: "jira:PAY-1", body: "new" },
  });
  expect(withBody({ Transition: { entity: "jira:PAY-1" } }, "new")).toBeNull();
});

// -- the panel ----------------------------------------------------------------

test("nothing owed says so, rather than showing an empty list", async () => {
  const screen = await render();
  expect(screen.text()).toContain("Nothing is queued");
  screen.done();
});

/**
 * Story 18, as something a reader can act on: a write needing a decision is in
 * a different section with a different heading, not a differently-shaded row
 * in one list.
 */
test("a write needing a decision is separated from one merely waiting", async () => {
  rows = [held(), write()];
  counts = { pending: 1, held: 1, refused: 0 };
  const screen = await render();

  const decisions = screen.section("Needs your decision");
  const patience = screen.section("Waiting");
  expect(decisions).not.toBeNull();
  expect(patience).not.toBeNull();
  expect(decisions!.textContent).toContain("The target changed");
  expect(patience!.textContent).toContain("the server did not answer");
  // And neither section contains the other's row.
  expect(decisions!.textContent).not.toContain("the server did not answer");
  screen.done();
});

/** Story 12: both versions, side by side, from the two persisted snapshots. */
test("a held write shows what it was and what it is", async () => {
  rows = [held()];
  counts = { pending: 0, held: 1, refused: 0 };
  const screen = await render();

  const text = screen.text();
  expect(text).toContain("When you queued it");
  expect(text).toContain("As it stands now");
  expect(text).toContain("a payout fails");
  expect(text).toContain("jonas: already on it");
  screen.done();
});

/** A target the source withdrew is a change like any other, and named as one. */
test("a target that vanished is shown as gone rather than as blank", async () => {
  rows = [held({ held_snapshot: { op: "comment", live: false, text: null } })];
  counts = { pending: 0, held: 1, refused: 0 };
  const screen = await render();
  expect(screen.text()).toContain("the source withdrew it");
  screen.done();
});

/** Story 13. */
test("send mine anyway releases exactly that write", async () => {
  rows = [held()];
  counts = { pending: 0, held: 1, refused: 0 };
  const screen = await render();
  calls.length = 0;

  screen.button("Send mine anyway")!.click();
  await vi.waitFor(() =>
    expect(calls.some((c) => c.command === "apply_held_write")).toBe(true),
  );
  expect(calls.find((c) => c.command === "apply_held_write")!.args).toEqual({ id: 2 });
  screen.done();
});

/** Stories 7 and 14 — the same act on the same row, from either section. */
test("discard and cancel both withdraw the write", async () => {
  rows = [held(), write()];
  counts = { pending: 1, held: 1, refused: 0 };
  const screen = await render();
  calls.length = 0;

  screen.button("Discard")!.click();
  await vi.waitFor(() => expect(calls.some((c) => c.command === "discard_write")).toBe(true));
  expect(calls.find((c) => c.command === "discard_write")!.args).toEqual({ id: 2 });

  // The panel refuses a second action while the first is in flight, so the
  // buttons are disabled until it settles. Waiting for that is not test
  // ceremony -- it *is* the guard against a double-fired discard.
  await vi.waitFor(() => expect(screen.queue.busy).toBe(false));
  flushSync();

  calls.length = 0;
  screen.button("Cancel")!.click();
  await vi.waitFor(() => expect(calls.some((c) => c.command === "discard_write")).toBe(true));
  expect(calls.find((c) => c.command === "discard_write")!.args).toEqual({ id: 1 });
  screen.done();
});

/** Story 15: the reader merges the two intentions themselves. */
test("editing a held write sends the words the reader typed", async () => {
  rows = [held()];
  counts = { pending: 0, held: 1, refused: 0 };
  const screen = await render();

  screen.button("Edit")!.click();
  flushSync();
  const box = document.querySelector("textarea")!;
  expect(box.value).toBe("on it");
  box.value = "thanks jonas — adding the trace";
  box.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();

  calls.length = 0;
  screen.button("Send this instead")!.click();
  await vi.waitFor(() => expect(calls.some((c) => c.command === "amend_write")).toBe(true));
  expect(calls.find((c) => c.command === "amend_write")!.args).toEqual({
    id: 2,
    payload: { Comment: { entity: "jira:PAY-231", body: "thanks jonas — adding the trace" } },
  });
  screen.done();
});

/**
 * An op this build cannot edit still gets its other two choices. The reader is
 * not stuck with a held write just because a later milestone added the op.
 */
test("an op this build cannot edit is still applicable and discardable", async () => {
  rows = [held({ payload: { Transition: { entity: "jira:PAY-231", to: "Done" } } as never })];
  counts = { pending: 0, held: 1, refused: 0 };
  const screen = await render();

  expect(screen.button("Edit")).toBeUndefined();
  expect(screen.button("Send mine anyway")).toBeDefined();
  expect(screen.button("Discard")).toBeDefined();
  screen.done();
});

/** Story 19, on screen: what the source said, not merely that it said no. */
test("a refused write reports what the source said and offers no retry", async () => {
  rows = [
    held({
      id: 3,
      state: "refused",
      held_snapshot: null,
      detail: "this issue type does not accept comments",
    }),
  ];
  counts = { pending: 0, held: 0, refused: 1 };
  const screen = await render();

  const text = screen.text();
  expect(text).toContain("will not be retried");
  expect(text).toContain("this issue type does not accept comments");
  expect(screen.button("Send mine anyway")).toBeUndefined();
  screen.done();
});

// -- the rule with no exceptions ---------------------------------------------

/**
 * **Story 16.** A held write is terminal until the reader acts, and this is
 * the half of that rule the UI could break: a timer, a poll that "helpfully"
 * retries, a flush that sweeps.
 *
 * The clock is run a day forward over an open panel holding a held write, and
 * the panel must have asked for nothing.
 */
test("no amount of time moves a held write", async () => {
  vi.useFakeTimers();
  try {
    rows = [held()];
    counts = { pending: 0, held: 1, refused: 0 };
    const screen = await render();
    calls.length = 0;

    vi.advanceTimersByTime(24 * 60 * 60 * 1000);
    flushSync();

    expect(calls).toEqual([]);
    expect(screen.text()).toContain("Needs your decision");
    screen.done();
  } finally {
    vi.useRealTimers();
  }
});

/**
 * And the other way the rule could be broken: a bulk action. *Flush now* is
 * impatience, and it must reach the flush command and nothing else — the
 * backend will not offer a held write to its flush loop, and the panel must
 * not compensate for that on the reader's behalf.
 */
test("flush now retries the waiting writes and releases nothing", async () => {
  rows = [held(), write()];
  counts = { pending: 1, held: 1, refused: 0 };
  const screen = await render();
  calls.length = 0;

  screen.button("Flush now")!.click();
  await vi.waitFor(() => expect(calls.some((c) => c.command === "flush_writes")).toBe(true));

  expect(calls.filter((c) => c.command === "apply_held_write")).toEqual([]);
  expect(calls.find((c) => c.command === "flush_writes")!.args).toEqual({ sourceId: null });
  // The footer says which rows it moves, so pressing it is never mistaken for
  // answering the conflict above it.
  expect(screen.text()).toContain("Held writes are untouched");
  screen.done();
});

/**
 * A failed read leaves the counts where they were.
 *
 * A badge that blinked to zero during a hiccup is a held write the reader
 * stops looking for, which is the failure this whole surface exists to
 * prevent.
 */
test("a failed refresh reports the failure without claiming nothing is owed", async () => {
  rows = [held()];
  counts = { pending: 0, held: 1, refused: 0 };
  const queue = createWriteQueue();
  await queue.refresh();
  expect(queue.counts.held).toBe(1);

  reject = { code: "internal", message: "the database went away", source_id: null };
  await queue.refresh();

  expect(queue.error).toBe("the database went away");
  expect(queue.counts.held).toBe(1);
  expect(queue.rows).toHaveLength(1);
});
