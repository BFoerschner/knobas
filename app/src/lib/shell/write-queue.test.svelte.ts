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
import { readFileSync } from "node:fs";
import { join } from "node:path";

import { flushSync, mount, unmount } from "svelte";
import { beforeEach, expect, test, vi } from "vitest";

import type { QueueCounts, QueuedWrite } from "../ipc/sources";
import { onWithdrawn, type WordedRow } from "./write-queue.svelte";
import { waitFor } from "./test-wait";

const calls: { command: string; args: unknown }[] = [];
let rows: QueuedWrite[] = [];
let counts: QueueCounts = { pending: 0, held: 0, refused: 0 };
/** Set to make the next command reject, so the error path is reachable. */
let reject: unknown = null;
/** Set to make every flush a send that never answers (#775). */
let flushHold: Promise<void> | null = null;
/** Set to make every apply a call that never answers, so the store stays busy (#1671). */
let applyHold: Promise<void> | null = null;

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
  flushWrites: (sourceId: string | null) =>
    flushHold ?? record("flush_writes", { sourceId }, undefined),
  applyHeldWrite: (id: number) => applyHold ?? record("apply_held_write", { id }, undefined),
  discardWrite: (id: number) =>
    // The fake queue actually forgets the row, so "the row is untouched" is
    // something a test reads off the panel rather than infers from the call
    // log. A rejected discard leaves the rows alone, which is the truth as
    // well: nothing settled.
    record("discard_write", { id }, undefined).then(() => {
      rows = rows.filter((row) => row.id !== id);
    }),
  amendWrite: (id: number, payload: unknown) => record("amend_write", { id, payload }, undefined),
}));

const { default: WriteQueuePanel } = await import("./WriteQueue.svelte");
const {
  blockers,
  createWriteQueue,
  decisionsIn,
  demandOf,
  editableBody,
  movedFile,
  readSnapshot,
  withBody,
  withdrawnWorklog,
  writeWords,
} = await import("./write-queue.svelte");
const { createLayers } = await import("../tf/layers.svelte");

/**
 * ADR-0012's canonical sentence, **read out of the ADR at test time**.
 *
 * Not retyped here: a copy in the test is a second place for the wording to
 * drift from the decision record, and a test holding its own copy would go on
 * passing while the surface and the ADR disagreed. `commands/time.rs` does
 * the same for the worklog draft; this side can go further and check the
 * sentence in the *rendered* dialog rather than in the component's source.
 */
function adrSentence(): string {
  const adr = readFileSync(
    join(process.cwd(), "..", "docs/adr/0012-writes-are-at-least-once-no-idempotency-key.md"),
    "utf8",
  );
  const quoted = adr
    .split("\n")
    .find((line) => line.startsWith("> A write knobas was sending"));
  if (quoted === undefined) {
    throw new Error("ADR-0012 no longer states its canonical sentence as a block quote");
  }
  // Collapsed on both sides: the ADR is Prettier-formatted and the component
  // wraps its prose, so a literal comparison would fail on formatting.
  return quoted.replace(/^>\s*/, "").split(/\s+/).join(" ");
}

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
    source_enabled: true,
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

/**
 * A queued worklog — the one op whose withdrawal can bill an hour twice
 * (#331).
 *
 * Its `started` is deliberately a **different day** from `queued_at`, and its
 * `seconds` a different number from every other quantity in this file, so a
 * dialog that named the wrong instant or the wrong length would fail rather
 * than coincidentally agree.
 */
function worklog(over: Partial<QueuedWrite> = {}): QueuedWrite {
  return write({
    id: 4,
    op: "log_work",
    payload: {
      LogWork: {
        entity: "jira:PAY-231",
        started: "2026-08-27T10:00:00Z",
        seconds: 8100,
        comment: "traced the payout retry",
      },
    },
    target_snapshot: { op: "log_work", live: true, text: null },
    ...over,
  });
}

/**
 * Let anything a click started run to the end, then re-render.
 *
 * A `setTimeout(0)` rather than a microtask flush, because the queue's own
 * `act` is three awaits deep — the command, its `.then`, and the re-read that
 * follows — and a macrotask runs after all of them. Without it a *negative*
 * assertion about the panel ("the row is still listed") reads the state as it
 * was before the click's work landed, and passes whatever the button did.
 */
async function settle() {
  await new Promise((resolve) => setTimeout(resolve, 0));
  flushSync();
}

async function render(names: Record<string, string> = {}, onclose: () => void = () => {}) {
  const queue = createWriteQueue();
  await queue.refresh();
  const target = document.createElement("div");
  document.body.append(target);
  const layers = createLayers();
  const app = mount(WriteQueuePanel, {
    target,
    props: { queue, now: NOW, onclose, layers, nameOf: (id: string) => Promise.resolve(names[id] ?? null) },
  });
  flushSync();
  return {
    queue,
    layers,
    target,
    text: () => (target.textContent ?? "").replace(/\s+/g, " "),
    button: (label: string) =>
      [...target.querySelectorAll("button")].find((b) =>
        (b.textContent ?? "").trim().startsWith(label),
      ),
    /** Every open dialog, panel first — a confirmation stacks a second one. */
    dialogs: () => [...target.querySelectorAll<HTMLElement>('[role="dialog"]')],
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

// -- a write behind a stalled one (#1473) ------------------------------------

/**
 * One entity's writes go in order, and a held or refused head stops the ones
 * behind it (`knobas_core::write_queue::due`). The panel names that blocker
 * rather than promising the write goes on its own.
 */
test("a waiting write behind a held or refused write on the same item names it", () => {
  const pause = (over: Partial<QueuedWrite>) =>
    write({ entity_id: "kuma:8", op: "pause_monitor", payload: { PauseMonitor: { entity: "kuma:8" } }, ...over });
  const refused = pause({ id: 10, state: "refused" });
  const behind = pause({ id: 11 });
  const heldPage = write({ id: 20, entity_id: "jira:PAY-300", state: "held" });
  const behindHeld = write({ id: 21, entity_id: "jira:PAY-300" });
  const elsewhere = write({ id: 12, entity_id: "jira:PAY-231" });
  const ahead = pause({ id: 9 });
  const found = blockers([behindHeld, heldPage, behind, refused, elsewhere, ahead]);
  expect(found.get(11)?.id).toBe(10);
  expect(found.get(11)?.state).toBe("refused");
  expect(found.get(21)?.state).toBe("held");
  expect(found.has(12), "a write on another item").toBe(false);
  expect(found.has(9), "a write ahead of the refused one").toBe(false);
  expect(found.has(10), "the blocker itself is no waiting write").toBe(false);
});

test("of two stalled writes ahead, the oldest is the one that blocks", () => {
  const found = blockers([
    write({ id: 3 }),
    write({ id: 2, state: "held" }),
    write({ id: 1, state: "refused" }),
  ]);
  expect(found.get(3)?.id).toBe(1);
  expect(found.size).toBe(1);
});

const PROMISE = "These go on their own as soon as their source can take them.";

test("a write behind a refused one says it waits for it, and the group does not promise it goes on its own", async () => {
  const pause = { entity_id: "kuma:8", op: "pause_monitor", payload: { PauseMonitor: { entity: "kuma:8" } } };
  rows = [
    write({ id: 11, ...pause, attempted_at: null }),
    write({ id: 10, ...pause, state: "refused", detail: "monitor is in maintenance" }),
  ];
  counts = { pending: 1, held: 0, refused: 1 };
  const screen = await render({ "kuma:8": "Payments API" });
  const waiting = screen.section("Waiting")!;
  expect(waiting.querySelector(".tf-wq-why")?.textContent).toBe("Waits for the refused write above.");
  expect(waiting.textContent).not.toContain("Not tried yet");
  expect(waiting.textContent).not.toContain("go on their own");
  expect(waiting.querySelector(".tf-wq-note")?.textContent).toBe(
    "These wait for a write above that needs your decision. Each goes once that write is sent or discarded.",
  );
  screen.done();
});

test("a write behind a held one names the held write, and an unblocked neighbour keeps its own reason", async () => {
  rows = [write({ id: 12, entity_id: "jira:PAY-300" }), write({ id: 3 }), held({ id: 2 })];
  counts = { pending: 2, held: 1, refused: 0 };
  const screen = await render();
  const waiting = screen.section("Waiting")!;
  const reasons = [...waiting.querySelectorAll(".tf-wq-why")].map((node) => node.textContent);
  expect(reasons).toEqual(["The server did not answer.", "Waits for the held write above."]);
  expect(waiting.querySelector(".tf-wq-note")?.textContent).toBe(
    "Unless a row says it waits for a write above, these go on their own as soon as their source can take them.",
  );
  screen.done();
});

// #1741: a write flushed while the keychain was locked waits `unreachable`
// and climbs the ladder; the detail the queue records starts `keychain:`.
test("a write waiting on a keychain that would not open says so, not that the server did not answer", async () => {
  rows = [
    write({ id: 4, detail: "keychain: User interaction is not allowed." }),
    write({ id: 5, entity_id: "jira:PAY-300" }),
  ];
  counts = { pending: 2, held: 0, refused: 0 };
  const screen = await render();
  const waiting = screen.section("Waiting")!;
  const reasons = [...waiting.querySelectorAll(".tf-wq-why")].map((node) => node.textContent);
  expect(reasons).toEqual([
    "The keychain would not open. Unlock it and this is tried again on its own.",
    "The server did not answer.",
  ]);
  screen.done();
});

test("once the blocker is discarded the write waits on its source alone", async () => {
  rows = [write({ id: 3, attempted_at: null }), write({ id: 1, state: "refused" })];
  counts = { pending: 1, held: 0, refused: 1 };
  const screen = await render();
  expect(screen.section("Waiting")!.textContent).toContain("Waits for the refused write above.");
  screen.section("Needs your decision")!.querySelector<HTMLButtonElement>("button.tf-danger")!.click();
  await settle();
  const waiting = screen.section("Waiting")!;
  expect(waiting.querySelector(".tf-wq-why")?.textContent).toBe("Not tried yet.");
  expect(waiting.querySelector(".tf-wq-note")?.textContent).toBe(PROMISE);
  screen.done();
});

// -- a row in words (#1425) ---------------------------------------------------

/** A row's heading as the reader sees it: the parts, joined. */
function words(row: WordedRow, names: Record<string, string> = {}): string {
  return writeWords(row, (id) => names[id] ?? null)
    .map((part) => part.text)
    .join("");
}

/**
 * The `?fake-ipc` queue's rows (`dev/fake/write-queue.ts`), by op, target and
 * payload. Copied, not imported: only the dev harness may reach the fixture.
 */
const FIXTURE_QUEUE = [
  { op: "comment", entity_id: "mock:PAY-231", payload: { Comment: { entity: "mock:PAY-231", body: "Taking this." } } },
  { op: "comment", entity_id: "mock:PAY-228", payload: { Comment: { entity: "mock:PAY-228", body: "Closing." } } },
  { op: "comment", entity_id: "mock:PAY-300", payload: { Comment: { entity: "mock:PAY-300", body: "Same root cause." } } },
  {
    op: "log_work",
    entity_id: "mock:PAY-228",
    payload: { LogWork: { entity: "mock:PAY-228", started: "2026-08-27T14:00:00Z", seconds: 3600, comment: "" } },
  },
  // The refused pause and the pause waiting behind it (#1473): one heading.
  { op: "pause_monitor", entity_id: "mock:payouts-stage-health", payload: { PauseMonitor: { entity: "mock:payouts-stage-health" } } },
];

test("the fixture's rows are named in words, never by op or raw id", () => {
  const headings = FIXTURE_QUEUE.map((row) => words(row));
  expect(headings).toEqual([
    "Comment on PAY-231",
    "Comment on PAY-228",
    "Comment on PAY-300",
    "1h worklog on PAY-228",
    "Pause a monitor",
  ]);
  for (const [index, heading] of headings.entries()) {
    const row = FIXTURE_QUEUE[index]!;
    expect(heading).not.toContain(row.op);
    expect(heading).not.toContain(row.entity_id);
  }
});

test("only a key is set in mono, the words around it are not", () => {
  const parts = writeWords(FIXTURE_QUEUE[0]!, () => null);
  expect(parts).toEqual([{ text: "Comment on " }, { text: "PAY-231", code: true }]);
});

test("a monitor pause names the monitor, and says only what it is when the name is not known", () => {
  const pause = { op: "pause_monitor", entity_id: "kuma:8", payload: { PauseMonitor: { entity: "kuma:8" } } };
  expect(words(pause, { "kuma:8": "Payments API" })).toBe("Pause monitor Payments API");
  expect(words(pause)).toBe("Pause a monitor");
  const resume = { op: "resume_monitor", entity_id: "kuma:8", payload: { ResumeMonitor: { entity: "kuma:8" } } };
  expect(words(resume, { "kuma:8": "Payments API" })).toBe("Resume monitor Payments API");
});

test("every op this build knows reads as words", () => {
  const cases: [Pick<QueuedWrite, "op" | "entity_id" | "payload">, string][] = [
    [{ op: "transition", entity_id: "jira:PAY-231", payload: { Transition: { entity: "jira:PAY-231", status: "In Review" } } }, "Move PAY-231 to In Review"],
    [{ op: "create_ticket", entity_id: "jira:PAY", payload: { CreateTicket: { entity: "jira:PAY", title: "Retry cap", body: "", ticket_type: "Task" } } }, "New ticket “Retry cap”"],
    [{ op: "update_page", entity_id: "confluence:sepa", payload: { UpdatePage: { entity: "confluence:sepa", base_version: 4, body: "<p/>" } } }, "Edit page SEPA payout retry design"],
    [{ op: "create_page", entity_id: "confluence:sepa", payload: { CreatePage: { parent: "confluence:sepa", space: "PAY", title: "Runbook", body: "" } } }, "New page “Runbook”"],
    [{ op: "run_build", entity_id: "teamcity:Payout_Build", payload: { RunBuild: { entity: "teamcity:Payout_Build", branch: null, parameters: [] } } }, "Run Payout build"],
    [{ op: "commit_file", entity_id: "gitea:tidewater/payout-service", payload: COMMIT }, "Commit README.md to main"],
    [{ op: "something_new", entity_id: "x:1", payload: { SomethingNew: {} } as unknown as QueuedWrite["payload"] }, "A write"],
  ];
  const names = { "confluence:sepa": "SEPA payout retry design", "teamcity:Payout_Build": "Payout build" };
  for (const [row, expected] of cases) expect(words(row, names)).toBe(expected);
});

// -- a page edit names its section (#1520) -----------------------------------

/** A Confluence page as the mirror holds it: storage format and a version. */
const PAGE = '<p>How payouts retry.</p><h2>Context</h2><p>Nightly batch.</p><h2><strong>Risks &amp; limits</strong></h2><p>None yet.</p>';

/** A queued page edit whose whole body is `body`, made against `PAGE`. */
function pageEdit(body: string, snapshot: unknown = { op: "update_page", live: true, payload: { body: { storage: { value: PAGE } }, version: { number: 4 } } }) {
  return {
    op: "update_page",
    entity_id: "confluence:sepa",
    payload: { UpdatePage: { entity: "confluence:sepa", base_version: 4, body } },
    target_snapshot: snapshot,
  };
}

const SEPA = { "confluence:sepa": "SEPA payout retry design" };

test("a page edit names the one section it changes", () => {
  const context = pageEdit(PAGE.replace("Nightly batch.", "Nightly batch, 14 unsettled."));
  expect(words(context, SEPA)).toBe("Edit section “Context” of SEPA payout retry design");
  const last = pageEdit(PAGE.replace("None yet.", "<p>The PSP caps retries.</p>"));
  expect(words(last, SEPA), "markup and an entity in the heading read as words").toBe(
    "Edit section “Risks & limits” of SEPA payout retry design",
  );
  expect(words(context), "a page whose title is not known yet").toBe("Edit section “Context” of a page");
});

test("a page edit names the page when no one section is the change", () => {
  const cases: [string, ReturnType<typeof pageEdit>][] = [
    ["two sections", pageEdit(PAGE.replace("Nightly batch.", "Daily.").replace("None yet.", "Some."))],
    ["nothing", pageEdit(PAGE)],
    ["the words before the first heading", pageEdit(PAGE.replace("How payouts retry.", "How payouts are retried."))],
    ["a heading added", pageEdit(PAGE.replace("Nightly batch.", "Nightly batch.</p><h2>Ledger</h2><p>Drift."))],
    ["a heading's own words", pageEdit(PAGE.replace("<h2>Context</h2>", "<h2>Background</h2>"))],
    ["a later heading's own words", pageEdit(PAGE.replace("Risks &amp; limits", "Risks"))],
    ["a heading removed", pageEdit(PAGE.replace("<h2><strong>Risks &amp; limits</strong></h2>", ""))],
    ["no snapshot", pageEdit(PAGE.replace("Nightly", "Daily"), null)],
    ["a snapshot with no storage body", pageEdit(PAGE.replace("Nightly", "Daily"), { op: "update_page", live: true, payload: { body: "plain" } })],
    ["another op's snapshot", pageEdit(PAGE.replace("Nightly", "Daily"), { op: "comment", live: true, payload: { body: { storage: { value: PAGE } } } })],
  ];
  for (const [what, row] of cases) expect(words(row, SEPA), what).toBe("Edit page SEPA payout retry design");
});

/**
 * The classes the unported sheet sets in capitals, read out of it, so a rule
 * added there later is checked too.
 */
function uppercaseClasses(): string[] {
  const sheet = readFileSync(join(process.cwd(), "src/unported.css"), "utf8");
  const classes = new Set<string>();
  for (const rule of sheet.matchAll(/([^{}]+)\{[^}]*text-transform:\s*uppercase[^}]*\}/g)) {
    for (const name of rule[1]!.matchAll(/\.([A-Za-z][\w-]*)/g)) classes.add(name[1]!);
  }
  return [...classes];
}

test("the panel is a ticket-first dialog: its title, its counts, no capitals, every row's controls", async () => {
  rows = [
    held({ id: 2, entity_id: "jira:PAY-231" }),
    write({ id: 3, state: "refused", entity_id: "kuma:8", op: "pause_monitor", payload: { PauseMonitor: { entity: "kuma:8" } }, detail: "monitor is in maintenance" }),
    write({ id: 1, entity_id: "jira:PAY-300", payload: { Comment: { entity: "jira:PAY-300", body: "same root cause" } } }),
  ];
  counts = { pending: 1, held: 1, refused: 1 };
  const screen = await render({ "kuma:8": "Payments API" });
  await waitFor(() => expect(screen.text()).toContain("Pause monitor Payments API"));

  const [dialog] = screen.dialogs();
  const title = document.getElementById(dialog!.getAttribute("aria-labelledby")!);
  expect(title?.textContent).toBe("Queued writes");
  expect(dialog!.querySelector(".tf-dsub")?.textContent).toBe("1 waiting · 2 need you");
  expect(screen.text()).toContain("Retries the waiting writes. Held writes are untouched.");
  expect(screen.layers.open.map((layer) => layer.kind)).toEqual(["dialog"]);

  const capitals = uppercaseClasses();
  expect(capitals).toContain("lab");
  for (const name of capitals) expect(screen.target.querySelector(`.${name}`), name).toBeNull();

  const headings = [...screen.target.querySelectorAll(".tf-wq-row-h b")].map((node) => node.textContent);
  expect(headings).toEqual(["Comment on PAY-231", "Pause monitor Payments API", "Comment on PAY-300"]);
  expect(screen.text()).not.toContain("pause_monitor");
  expect(screen.text()).not.toContain("kuma:8");

  const decisions = screen.section("Needs your decision")!;
  const controls = (article: Element) => [...article.querySelectorAll("button")].map((b) => b.textContent?.trim());
  const [heldRow, refusedRow] = decisions.querySelectorAll("article");
  expect(controls(heldRow!)).toEqual(["Send mine anyway", "Edit…", "Discard"]);
  expect(controls(refusedRow!)).toEqual(["Discard"]);
  expect(controls(screen.section("Waiting")!.querySelector("article")!)).toEqual(["Cancel"]);
  screen.done();
});

/**
 * The panel is rung 1 of the `Esc` ladder (#1474): the shell's `escStep`
 * closes the topmost layer, and the panel's layer is its own close. The
 * shell's tests press the key; this pins the layer the key reaches.
 */
test("the panel's layer closes it, and goes when the panel does", async () => {
  let closes = 0;
  const screen = await render({}, () => (closes += 1));
  expect(screen.layers.open.map((layer) => layer.kind)).toEqual(["dialog"]);
  screen.layers.open[0]!.close();
  expect(closes).toBe(1);
  screen.done();
  expect(screen.layers.open, "the panel's layer outlived it").toEqual([]);
});

test("a snapshot is read per op, and says so when it cannot be", () => {
  expect(readSnapshot({ op: "comment", live: true, text: "hello" })).toEqual({
    text: "hello",
    live: true,
    raw: null,
    version: null,
  });
  // A target that is gone is not an unreadable snapshot -- it is a fact.
  expect(readSnapshot({ op: "comment", live: false, text: null })).toEqual({
    text: null,
    live: false,
    raw: null,
    version: null,
  });
  // A projection this build has no reader for is shown verbatim rather than
  // hidden: the reader is being asked to decide.
  const unknown = readSnapshot({ op: "transition", live: true, payload: { status: "done" } });
  expect(unknown.text).toBeNull();
  expect(unknown.raw).toContain("transition");
});

/**
 * **A held page edit shows which version each side is** (#286).
 *
 * The one write whose two sides can read alike while differing in what
 * matters: a page reformatted, moved, or whose macro re-rendered strips to the
 * same text. The number is what says the mirror moved, and it is read only out
 * of an `update_page` snapshot — a `version` key means different things in
 * different sources, so guessing from a payload's shape is the guess ADR-0007
 * forbids.
 */
test("a held page edit says which version each side is, and no other op does", () => {
  const page = (number: unknown) => ({
    op: "update_page",
    live: true,
    text: "base 30 s.",
    payload: { version: { number } },
  });
  expect(readSnapshot(page(3)).version).toBe(3);
  expect(readSnapshot(page(4)).version).toBe(4);
  // The text still renders: the number is beside it, never instead of it.
  expect(readSnapshot(page(3)).text).toBe("base 30 s.");

  // Every shape that is not a version number is a miss, not a guess.
  for (const number of ["3", null, 1.5, undefined]) {
    expect(readSnapshot(page(number)).version, JSON.stringify(number)).toBeNull();
  }
  expect(readSnapshot({ op: "update_page", live: true, text: "x" }).version).toBeNull();
  // Another op carrying the same path is not read at all.
  expect(readSnapshot({ op: "transition", live: true, payload: { version: { number: 3 } } }).version)
    .toBeNull();
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
  expect(patience!.textContent).toContain("The server did not answer");
  // And neither section contains the other's row.
  expect(decisions!.textContent).not.toContain("The server did not answer");
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
  expect(screen.text()).toContain("The source withdrew it");
  screen.done();
});

// -- the disabled-source reason (issue #204) ----------------------------------

/**
 * A write held because its source is off is a different fact with a different
 * remedy: no target changed, so no two-versions comparison and no *Send mine
 * anyway* — the source cannot take it, and offering the button would park the
 * write as silently "waiting" forever. Re-enabling is the remedy, and the row
 * says so.
 */
test("a write held while its source is off says so, not 'the target changed'", async () => {
  rows = [
    held({
      source_enabled: false,
      held_snapshot: { op: "comment", live: false, text: null },
    }),
  ];
  counts = { pending: 0, held: 1, refused: 0 };
  const screen = await render();

  const text = screen.text();
  expect(text).toContain("its source is turned off");
  expect(text).not.toContain("The target changed");
  expect(text).not.toContain("As it stands now");
  expect(screen.button("Send mine anyway")).toBeUndefined();
  // The exits that still work stay: the write can be edited or withdrawn.
  expect(screen.button("Discard")).toBeDefined();
  screen.done();
});

/**
 * #204's miss direction: a write held because its *target* changed, while the
 * source is on, must not blame the source — the wrong remedy sends the reader
 * to the Sources view for nothing.
 */
test("a write held for a changed target does not claim the source is off", async () => {
  rows = [held()];
  counts = { pending: 0, held: 1, refused: 0 };
  const screen = await render();

  const text = screen.text();
  expect(text).toContain("The target changed");
  expect(text).not.toContain("turned off");
  screen.done();
});

/**
 * The same fact on a merely pending row: the scheduler never flushes a
 * disabled source, so "waiting" with no reason would wait forever without
 * saying why. The reason line carries the remedy instead.
 */
test("a pending write against a disabled source says why nothing moves", async () => {
  rows = [write({ source_enabled: false })];
  counts = { pending: 1, held: 0, refused: 0 };
  const screen = await render();
  expect(screen.text()).toContain("Its source is turned off");
  screen.done();
});

/** Story 13. */
test("send mine anyway releases exactly that write", async () => {
  rows = [held()];
  counts = { pending: 0, held: 1, refused: 0 };
  const screen = await render();
  calls.length = 0;

  screen.button("Send mine anyway")!.click();
  await waitFor(() =>
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
  await waitFor(() => expect(calls.some((c) => c.command === "discard_write")).toBe(true));
  expect(calls.find((c) => c.command === "discard_write")!.args).toEqual({ id: 2 });

  // The panel refuses a second action while the first is in flight, so the
  // buttons are disabled until it settles. Waiting for that is not test
  // ceremony -- it *is* the guard against a double-fired discard.
  await waitFor(() => expect(screen.queue.busy).toBe(false));
  flushSync();

  calls.length = 0;
  screen.button("Cancel")!.click();
  await waitFor(() => expect(calls.some((c) => c.command === "discard_write")).toBe(true));
  expect(calls.find((c) => c.command === "discard_write")!.args).toEqual({ id: 1 });
  screen.done();
});

/** The pages' echoes follow this (#1583): withdrawn and sent both leave `pending_writes`, so only this says which. */
test("a withdrawal from the panel is announced to whoever marked the write", async () => {
  rows = [held(), write()];
  counts = { pending: 1, held: 1, refused: 0 };
  const screen = await render();
  const heard: string[] = [];
  const off = [onWithdrawn(2, () => heard.push("2")), onWithdrawn(1, () => heard.push("1"))];

  screen.button("Discard")!.click();
  await waitFor(() => expect(heard).toEqual(["2"]));
  off.forEach((stop) => stop());
  screen.done();
});

// -- withdrawing a worklog (issue #331) ---------------------------------------

/**
 * Which withdrawals need consent, as a decision rather than as markup.
 *
 * The op, and **only** the op. Scoping it to `pending` as well was the
 * obvious-looking narrowing and it is wrong twice over: Jira answers a
 * `Protocol` refusal for a worklog it *accepted but did not name*
 * (`knobas_source_jira::write::log_work`), and hold detection runs before the
 * send, so a write that arrived and lost its settle is held on the next
 * flush. `discard` releases the blocks of all three open states alike, so all
 * three are asked about.
 */
test("every open worklog's withdrawal needs consent, and no other op's", () => {
  const log = worklog();
  expect(withdrawnWorklog(log)).toEqual({
    entity: "jira:PAY-231",
    started: "2026-08-27T10:00:00Z",
    seconds: 8100,
    comment: "traced the payout retry",
  });

  // Every other op: today's behaviour, and the reasoning does not carry.
  expect(withdrawnWorklog(write())).toBeNull();
  const create = {
    CreateTicket: { entity: "jira:PAY", title: "a payout fails", body: "", ticket_type: "Bug" },
  } as const;
  expect(withdrawnWorklog(write({ op: "create_ticket", payload: create }))).toBeNull();

  // A refusal can be a worklog Jira took and did not name; a hold can be one
  // that arrived before its settle was lost. Neither state is evidence, and
  // neither is treated as any.
  expect(withdrawnWorklog(worklog({ state: "held" }))).not.toBeNull();
  expect(withdrawnWorklog(worklog({ state: "refused" }))).not.toBeNull();

  // An op named `log_work` whose payload this build cannot read is not a
  // worklog it can describe, and a dialog with blanks in it is worse than
  // none.
  const unreadable = { Comment: { entity: "jira:PAY-231", body: "on it" } } as const;
  expect(withdrawnWorklog(worklog({ payload: unreadable }))).toBeNull();

  // …and the *identifier* is what decides, never the payload's shape. `op` is
  // the stable name the backend and the panel already agree on (ADR-0006);
  // sniffing the payload instead would be a build confirming on an op it has
  // misread.
  expect(withdrawnWorklog(worklog({ op: "comment" }))).toBeNull();
});

/**
 * The consent moment #331 asks for, and the half of it that matters most:
 * **cancelling leaves the row exactly where it was.**
 *
 * Not "the command was not called" — the fake queue really does drop a
 * discarded row, so a panel still listing the write is the row surviving,
 * observed the way the reader observes it.
 */
test("discarding a queued worklog asks first, and cancelling leaves it queued", async () => {
  rows = [worklog()];
  counts = { pending: 1, held: 0, refused: 0 };
  const screen = await render();
  calls.length = 0;

  screen.button("Cancel")!.click();
  await settle();

  expect(calls.filter((c) => c.command === "discard_write")).toEqual([]);
  expect(screen.dialogs()).toHaveLength(2);

  // What is being withdrawn: the ticket, when the logged span began, and how
  // long it was — in minutes *and* in the seconds that cross the wire.
  const asking = screen.dialogs()[1]!.textContent!.replace(/\s+/g, " ");
  expect(asking).toContain("PAY-231");
  expect(asking).toContain("27 August 2026");
  expect(asking).toContain("2h 15m");
  expect(asking).toContain("8100 seconds");
  expect(asking).toContain("traced the payout retry");

  // The consequence in the user's words, and the guarantee in the ADR's.
  expect(asking).toContain("knobas cannot tell whether Jira already took it");
  expect(asking).toContain(adrSentence());

  screen.button("Keep it queued")!.click();
  await settle();

  expect(screen.dialogs()).toHaveLength(1);
  expect(calls.filter((c) => c.command === "discard_write")).toEqual([]);
  // The row itself: still in the queue the panel read, and still on screen.
  // Read after `settle`, so a *Keep it queued* that discarded would have
  // emptied both by now rather than being caught only by the call log.
  expect(screen.queue.rows.map((row) => row.id)).toEqual([4]);
  expect(screen.section("Waiting")!.textContent).toContain("worklog on PAY-231");
  screen.done();
});

/**
 * The same question from the other section, on the state that looks safest.
 *
 * A refused worklog is the one this nearly missed: Jira refuses with a
 * `Protocol` fault when it *took* the worklog and did not name it, so the row
 * that reads "the source refused this write" can be an hour already logged.
 */
test("a refused worklog asks too, from the decisions section", async () => {
  rows = [worklog({ state: "refused", wait_reason: null, detail: "worklog id missing" })];
  counts = { pending: 0, held: 0, refused: 1 };
  const screen = await render();
  calls.length = 0;

  screen.button("Discard")!.click();
  await settle();

  expect(calls.filter((c) => c.command === "discard_write")).toEqual([]);
  expect(screen.dialogs()).toHaveLength(2);
  expect(screen.dialogs()[1]!.textContent).toContain("knobas cannot tell whether Jira already");
  expect(screen.queue.rows.map((row) => row.id)).toEqual([4]);
  screen.done();
});

/** …and consent given is the discard that was asked for, on that row. */
test("confirming the withdrawal discards the worklog", async () => {
  rows = [worklog()];
  counts = { pending: 1, held: 0, refused: 0 };
  const screen = await render();
  calls.length = 0;

  screen.button("Cancel")!.click();
  flushSync();
  screen.button("Discard the worklog")!.click();

  await waitFor(() => expect(calls.some((c) => c.command === "discard_write")).toBe(true));
  expect(calls.find((c) => c.command === "discard_write")!.args).toEqual({ id: 4 });
  await waitFor(() => expect(screen.queue.busy).toBe(false));
  flushSync();

  expect(screen.dialogs()).toHaveLength(1);
  expect(screen.text()).toContain("Nothing is queued");
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
  await waitFor(() => expect(calls.some((c) => c.command === "amend_write")).toBe(true));
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

/**
 * A write that reached its source and was never answered (#1125) is held
 * with both possibilities stated: it may have arrived, so the reader checks
 * the source by name, then sends it again or discards it. *Send again* is the
 * release the backend calls `apply_held_write`.
 */
test("a write the source never answered says to check the source, then send again", async () => {
  rows = [
    held({
      op: "create_ticket",
      payload: { CreateTicket: { entity: "jira:PAY", title: "t", body: "b", ticket_type: "Task" } } as never,
      held_snapshot: {
        op: "create_ticket",
        live: false,
        unconfirmed: { source: "Jira (Tidewater)", detail: "unconfirmed: operation timed out" },
      },
    }),
  ];
  counts = { pending: 0, held: 1, refused: 0 };
  const screen = await render();

  const text = screen.text();
  expect(text).toContain("Jira (Tidewater) did not answer, so it may have arrived");
  expect(text).toContain("Check Jira (Tidewater), then send it again or discard it");
  expect(text).toContain("unconfirmed: operation timed out");
  expect(text).not.toContain("The target changed");
  expect(screen.button("Send mine anyway")).toBeUndefined();
  expect(screen.button("Discard")).toBeDefined();

  calls.length = 0;
  screen.button("Send again")!.click();
  await waitFor(() => expect(calls.some((c) => c.command === "apply_held_write")).toBe(true));
  expect(calls.find((c) => c.command === "apply_held_write")!.args).toEqual({ id: 2 });
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
  await waitFor(() => expect(calls.some((c) => c.command === "flush_writes")).toBe(true));

  expect(calls.filter((c) => c.command === "apply_held_write")).toEqual([]);
  expect(calls.find((c) => c.command === "flush_writes")!.args).toEqual({ sourceId: null });
  // The footer says which rows it moves, so pressing it is never mistaken for
  // answering the conflict above it.
  expect(screen.text()).toContain("Held writes are untouched");
  screen.done();
});

/** #2289: the canonical footer, its line on the far left, a ghost *Close*, then *Flush now* as the primary. */
test("the footer reads its line, a ghost Close, then Flush now as the primary", async () => {
  rows = [write()];
  counts = { pending: 1, held: 0, refused: 0 };
  const screen = await render();
  const footer = screen.dialogs()[0]!.querySelector("footer")!;
  expect([...footer.children].map((child) => child.className)).toEqual([
    "tf-wq-foot tf-first",
    "tf-btn tf-ghost",
    "tf-btn tf-primary",
  ]);
  expect([...footer.querySelectorAll("button")].map((b) => b.textContent?.trim())).toEqual(["Close", "Flush now"]);
  screen.done();
});

/**
 * The guarantee the queue cannot keep, said where a re-send is contemplated
 * (ADR-0012, issue #224). No transaction spans the send and the settle, so a
 * write in flight when knobas stopped may arrive twice. The sentence is the
 * ADR's own, verbatim -- read out of the ADR here rather than retyped (#331)
 * -- and it is not tied to a row: it holds whether the queue is empty or
 * full, so both are rendered.
 */
test("the panel states that a write may arrive twice, whatever it holds", async () => {
  const sentence = adrSentence();

  const empty = await render();
  expect(empty.text()).toContain("Nothing is queued");
  expect(empty.text()).toContain(sentence);
  empty.done();

  rows = [held(), write()];
  counts = { pending: 1, held: 1, refused: 0 };
  const full = await render();
  expect(full.text()).toContain(sentence);
  full.done();
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

// -- #746: an unread or failed queue is not an empty one -----------------------

async function renderUnread() {
  const queue = createWriteQueue();
  const target = document.createElement("div");
  document.body.append(target);
  const app = mount(WriteQueuePanel, { target, props: { queue, now: NOW, onclose: () => {} } });
  flushSync();
  return {
    queue,
    text: () => (target.textContent ?? "").replace(/\s+/g, " "),
    done: () => {
      unmount(app);
      target.remove();
    },
  };
}

test("a queue that has not been read never says 'Nothing is queued'", async () => {
  vi.useFakeTimers();
  try {
    const screen = await renderUnread();
    await vi.advanceTimersByTimeAsync(400);
    flushSync();
    expect(screen.text()).toContain("Reading the queue");
    expect(screen.text()).not.toContain("Nothing is queued");
    screen.done();
  } finally {
    vi.useRealTimers();
  }
});

test("a queue whose first read failed shows the reason, not 'Nothing is queued'", async () => {
  const screen = await renderUnread();
  reject = { code: "internal", message: "database is busy" };
  await screen.queue.refresh();
  flushSync();
  expect(screen.text()).toContain("database is busy");
  expect(screen.text()).not.toContain("Nothing is queued");
  screen.done();
});

/**
 * Only *Flush now* is disabled while a send runs (#775, #1671): it is the send,
 * and the panel's other answers -- withdraw, edit, send anyway -- stay
 * pressable while it is out.
 */
test("flush now disables only itself while the send it asked for is out", async () => {
  rows = [held(), write()];
  counts = { pending: 1, held: 1, refused: 0 };
  const screen = await render();
  flushHold = new Promise(() => {});
  try {
    screen.button("Flush now")!.click();
    await settle();
    expect(screen.queue.busy).toBe(false);
    // Its own button is off while it is out (#1671); the other answers are not.
    expect(screen.button("Flush now")!.disabled).toBe(true);
    expect(screen.button("Discard")!.disabled).toBe(false);
  } finally {
    flushHold = null;
  }
  screen.done();
});

/** #1671: a double click on *Flush now* is one send. */
test("double-clicking flush now sends one flush_writes", async () => {
  rows = [write()];
  counts = { pending: 1, held: 0, refused: 0 };
  const screen = await render();
  calls.length = 0;
  const button = screen.button("Flush now")!;
  button.click();
  button.click();
  await settle();
  expect(calls.filter((c) => c.command === "flush_writes")).toHaveLength(1);
  // …and the button is back once the send has answered.
  expect(screen.button("Flush now")!.disabled).toBe(false);
  button.click();
  await settle();
  expect(calls.filter((c) => c.command === "flush_writes")).toHaveLength(2);
  screen.done();
});

/**
 * #1671: an amend skipped because the store was busy keeps the editor and the
 * words typed in it. The Save button is disabled while busy, so the test lifts
 * that to reach the interleaving the button's state can lag behind.
 */
test("saving while the store is busy keeps the draft", async () => {
  rows = [held(), write({ id: 9 })];
  counts = { pending: 1, held: 1, refused: 0 };
  const screen = await render();
  screen.button("Edit")!.click();
  flushSync();
  const box = document.querySelector("textarea")!;
  box.value = "my careful words";
  box.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();

  applyHold = new Promise(() => {});
  try {
    void screen.queue.apply(9);
    expect(screen.queue.busy).toBe(true);
    calls.length = 0;
    const save = screen.button("Send this instead")!;
    save.disabled = false;
    save.click();
    await settle();
    expect(calls.some((c) => c.command === "amend_write")).toBe(false);
    expect(document.querySelector("textarea")!.value).toBe("my careful words");
  } finally {
    applyHold = null;
  }
  screen.done();
});

// -- a commit that lost a race (#845, on #822's hold) ---------------------------

const COMMIT = {
  CommitFile: {
    repo: "gitea:tidewater/payout-service",
    branch: "main",
    new_branch: null,
    path: "README.md",
    content: "# payout\nmine\n",
    message: "PAY-231: update README.md",
    base_sha: "blob-1",
  },
};

function movedCommit(): QueuedWrite {
  return held({
    id: 7,
    source_id: "gitea",
    entity_id: "gitea:tidewater/payout-service",
    op: "commit_file",
    payload: COMMIT,
    target_snapshot: { op: "commit_file", live: true },
    held_snapshot: { op: "commit_file", live: true, text: "# payout\ntheirs\n", base: "blob-2" },
  });
}

test("a held commit's text is its file, and its edit is made against the version the source holds now", () => {
  expect(editableBody(COMMIT)).toBe("# payout\nmine\n");
  expect(withBody(COMMIT, "# payout\nboth\n", { op: "commit_file", live: true, text: "x", base: "blob-2" })).toEqual({
    CommitFile: { ...COMMIT.CommitFile, content: "# payout\nboth\n", base_sha: "blob-2" },
  });
  // Held for another reason (no `base`): the base it was made against stays.
  expect(withBody(COMMIT, "y", { op: "commit_file", live: true })).toEqual({ CommitFile: { ...COMMIT.CommitFile, content: "y" } });
  expect(movedFile(movedCommit())).toEqual({ path: "README.md", mine: "# payout\nmine\n", theirs: "# payout\ntheirs\n" });
  expect(movedFile(held())).toBeNull();
});

test("a commit over a file that moved shows both versions of the file, and Edit sends against the new one", async () => {
  rows = [movedCommit()];
  counts = { pending: 0, held: 1, refused: 0 };
  const screen = await render();
  const sides = [...document.querySelectorAll(".tf-wq-sides > div")].map((side) => (side.textContent ?? "").replace(/\s+/g, " ").trim());
  expect(sides).toEqual(["Yours, README.md # payout mine", "On the source now # payout theirs"]);
  expect(screen.button("Send mine anyway")).toBeDefined();
  expect(screen.button("Discard")).toBeDefined();

  screen.button("Edit")!.click();
  flushSync();
  const box = document.querySelector("textarea")!;
  expect(box.value).toBe("# payout\nmine\n");
  box.value = "# payout\nmine and theirs\n";
  box.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  calls.length = 0;
  screen.button("Send this instead")!.click();
  await waitFor(() => expect(calls.some((c) => c.command === "amend_write")).toBe(true));
  expect(calls.find((c) => c.command === "amend_write")!.args).toEqual({
    id: 7,
    payload: { CommitFile: { ...COMMIT.CommitFile, content: "# payout\nmine and theirs\n", base_sha: "blob-2" } },
  });
  screen.done();
});
