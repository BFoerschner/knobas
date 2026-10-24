/**
 * The worklog draft in the new shell (#826, R-13.1 to R-13.3): what it shows,
 * which buttons it offers for which opening, what it sends, and how `Esc`
 * leaves it.
 *
 * The ticking and comment rules are `DraftForm`'s and are witnessed by
 * `WorklogDraft.test.svelte.ts`; the one checked again here is the one the
 * prototype states on screen, that the comment follows the boxes until edited.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { Candidate, DayRecord, Draft, LoggedWork, ReaderDay, Worklog } from "../ipc/time";
import { createLayers, type Layers } from "./layers.svelte";
import TfWorklogDraft from "./TfWorklogDraft.svelte";
import { waitFor } from "../shell/test-wait";
import type { RefusedWritesPort, WriteAsk } from "../shell/refused-writes";

const TICKET = "jira:PAY-231";

function candidate(id: string, source: Candidate["source"], at: string, bullet: string, entity: string | null = TICKET): Candidate {
  return { id, source, at, entity_id: entity, bullet };
}

/** Two blocks, 09:40–11:50 and 13:58–14:32 Berlin: 2h 44m. */
const DRAFT: Draft = {
  entity_id: TICKET,
  day: "2026-09-03",
  started_at: "2026-09-03T07:40:00Z",
  ended_at: "2026-09-03T12:32:00Z",
  seconds: 164 * 60,
  block_ids: [7, 8],
  running_since: null,
  candidates: [
    candidate("item:gitea:c1", "mirror", "2026-09-03T08:02:00Z", "- commit a41f2c PAY-231 backoff jitter", "gitea:a41f2c"),
    candidate("write:97", "write", "2026-09-03T08:20:00Z", "- replied to Jonas on PR #142"),
  ],
  comment: "- commit a41f2c PAY-231 backoff jitter\n- replied to Jonas on PR #142",
};

const DAY: DayRecord = {
  past_horizon: false,
  open: [],
  blocks: [
    {
      title: "Retry SEPA payouts",
      block: { id: 7, started_at: "2026-09-03T07:40:00Z", ended_at: "2026-09-03T09:50:00Z", target: { kind: "entity", entity_id: TICKET }, kind: "manual", ended_by_relaunch: false, worklog_id: null },
    },
    {
      title: null,
      block: { id: 9, started_at: "2026-09-03T10:00:00Z", ended_at: "2026-09-03T11:00:00Z", target: { kind: "label", label: "lunch" }, kind: "manual", ended_by_relaunch: false, worklog_id: null },
    },
    {
      title: "Retry SEPA payouts",
      block: { id: 8, started_at: "2026-09-03T11:58:00Z", ended_at: "2026-09-03T12:32:00Z", target: { kind: "entity", entity_id: TICKET }, kind: "manual", ended_by_relaunch: false, worklog_id: null },
    },
  ],
};

const WORKLOG: Worklog = {
  id: 3,
  entity_id: TICKET,
  started_at: DRAFT.started_at,
  seconds: DRAFT.seconds,
  comment: DRAFT.comment,
  block_ids: DRAFT.block_ids,
  write_queue_id: 11,
  remote_id: null,
  created_at: "2026-09-03T12:33:00Z",
};

let target: HTMLDivElement;
let app: Record<string, unknown> | undefined;

interface RenderOpts {
  draft?: Draft;
  stopped?: boolean;
  logWork?: () => Promise<Worklog>;
  onbeforelog?: () => Promise<void>;
  layers?: Layers;
  refusedWrites?: RefusedWritesPort;
}

function render(opts: RenderOpts = {}) {
  const sent: ({ entityId: string } & ReaderDay & LoggedWork)[] = [];
  const outcomes: string[] = [];
  /** `stop` and `log`, in the order the dialog asked for them. */
  const order: string[] = [];
  const onlogged = vi.fn();
  app = mount(TfWorklogDraft, {
    target,
    props: {
      draft: opts.draft ?? DRAFT,
      stopped: opts.stopped ?? true,
      layers: opts.layers ?? createLayers(),
      onoutcome: (outcome: string) => void outcomes.push(outcome),
      onbeforelog: () => {
        order.push("stop");
        return (opts.onbeforelog ?? (() => Promise.resolve()))();
      },
      onlogged,
      ports: {
        logWork: (entityId: string, when: ReaderDay, work: LoggedWork) => {
          order.push("log");
          sent.push({ entityId, ...when, ...work });
          return (opts.logWork ?? (() => Promise.resolve(WORKLOG)))();
        },
        dayBlocks: () => Promise.resolve(DAY),
        ...(opts.refusedWrites ? { refusedWrites: opts.refusedWrites } : {}),
        listSources: () =>
          Promise.resolve([
            { id: "jira", display_name: "Jira" },
            { id: "gitea", display_name: "Gitea" },
          ]),
      },
    },
  });
  flushSync();
  return { sent, outcomes, order, onlogged };
}

function button(name: string): HTMLButtonElement | undefined {
  return [...target.querySelectorAll<HTMLButtonElement>("button")].find((b) => (b.textContent ?? "").trim() === name);
}

const boxes = () => [...target.querySelectorAll<HTMLInputElement>("input[type=checkbox]")];
const comment = () => target.querySelector<HTMLTextAreaElement>("textarea")!;
/** The dialog's text with template whitespace folded, as a reader sees it. */
const text = () => (target.textContent ?? "").replace(/\s+/g, " ");

beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
});

afterEach(() => {
  if (app) unmount(app);
  app = undefined;
  target.remove();
});

test("it shows the key, the ticket, the duration and the intervals it came from", async () => {
  render();
  await waitFor(() => expect(target.textContent).toContain("09:40 to 11:50, and 13:58 to 14:32"));
  expect(target.querySelector("h2")?.textContent).toBe("Log time to PAY-231");
  expect(target.textContent).toContain("Retry SEPA payouts");
  expect(target.querySelector(".tf-dur b")?.textContent).toBe("2h 44m");
  expect(target.textContent).toContain("from 2 blocks");
});

test("each candidate is a ticked box with its time and source", async () => {
  render();
  await waitFor(() => expect(target.textContent).toContain("10:02, Gitea"));
  expect(boxes().map((box) => box.checked)).toEqual([true, true]);
  expect(target.textContent).toContain("commit a41f2c PAY-231 backoff jitter");
  expect(target.textContent).toContain("10:20, you wrote");
});

test("the comment follows the boxes until it is edited", () => {
  render();
  boxes()[1]!.click();
  flushSync();
  expect(comment().value).toBe("- commit a41f2c PAY-231 backoff jitter");
  comment().value = "Jitter.";
  comment().dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  boxes()[1]!.click();
  flushSync();
  expect(comment().value).toBe("Jitter.");
});

test("it says where the worklog goes, and quotes ADR-0012", async () => {
  render();
  await waitFor(() =>
    expect(text()).toContain(
      "Goes to Jira through the write queue. The comment follows the boxes you tick until you edit it.",
    ),
  );
  expect(text()).toContain(
    "A write knobas was sending when it stopped may arrive twice. knobas re-sends rather than guess; it never merges or drops what you wrote.",
  );
});

test("opened by a stop it offers Stop, log later, Keep running, Discard and Log", () => {
  render({ stopped: true });
  expect(button("Stop, log later")).toBeDefined();
  expect(button("Keep running")).toBeDefined();
  expect(button("Discard")).toBeDefined();
  expect(button("Log 2h 44m to PAY-231")).toBeDefined();
});

test("opened without a stop it offers Discard and Log, and nothing about a clock", () => {
  render({ stopped: false });
  expect(button("Discard")).toBeDefined();
  expect(button("Stop, log later")).toBeUndefined();
  expect(button("Keep running")).toBeUndefined();
});

test("each button answers with its outcome", () => {
  const { outcomes } = render();
  button("Keep running")!.click();
  button("Stop, log later")!.click();
  button("Discard")!.click();
  expect(outcomes).toEqual(["keep", "later", "discard"]);
});

test("Log sends the work for the reader's day and answers logged", async () => {
  const { sent, outcomes, onlogged } = render();
  button("Log 2h 44m to PAY-231")!.click();
  await waitFor(() => expect(outcomes).toEqual(["logged"]));
  expect(sent[0]).toMatchObject({ entityId: TICKET, day: "2026-09-03", startedAt: DRAFT.started_at, seconds: DRAFT.seconds });
  expect(onlogged).toHaveBeenCalledWith(WORKLOG);
});

test("Log sends the offset at the draft day's midnight, not today's", async () => {
  vi.useFakeTimers({ toFake: ["Date"] });
  try {
    vi.setSystemTime(new Date(2026, 10, 5, 12));
    const { sent, outcomes } = render();
    button("Log 2h 44m to PAY-231")!.click();
    await waitFor(() => expect(outcomes).toEqual(["logged"]));
    expect(sent[0]!.offsetMinutes).toBe(120);
  } finally {
    vi.useRealTimers();
  }
});

const ODD: Draft = { ...DRAFT, seconds: 163 };

test("a 163 s draft reads Log 2m and sends 120 s, never the 163", async () => {
  const { sent, outcomes } = render({ draft: ODD });
  button("Log 2m to PAY-231")!.click();
  await waitFor(() => expect(outcomes).toEqual(["logged"]));
  expect(sent[0]!.seconds).toBe(120);
});

test("a 163 s draft whose minutes are edited to 5 sends 300 s", async () => {
  const { sent, outcomes } = render({ draft: ODD });
  const field = target.querySelector<HTMLInputElement>('input[aria-label="Minutes"]')!;
  field.value = "5";
  field.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  button("Log 5m to PAY-231")!.click();
  await waitFor(() => expect(outcomes).toEqual(["logged"]));
  expect(sent[0]!.seconds).toBe(300);
});

test("a draft under a minute floors to 0m and cannot be sent", () => {
  render({ draft: { ...DRAFT, seconds: 59 } });
  expect(button("Log 0m to PAY-231")!.disabled).toBe(true);
});

test("Log stops the clock before it sends, so the running stretch is a block log_work can find", async () => {
  const { order, outcomes } = render();
  button("Log 2h 44m to PAY-231")!.click();
  await waitFor(() => expect(outcomes).toEqual(["logged"]));
  expect(order).toEqual(["stop", "log"]);
});

test("a clock that will not stop is said, and nothing is sent", async () => {
  const { order, outcomes } = render({ onbeforelog: () => Promise.reject(new Error("no timer to stop")) });
  button("Log 2h 44m to PAY-231")!.click();
  await waitFor(() => expect(target.textContent).toContain("no timer to stop"));
  expect(order).toEqual(["stop"]);
  expect(outcomes).toEqual([]);
  expect(button("Keep running")?.disabled).toBe(false);
});

test("over a running clock the open stretch is one of the blocks, and it runs to now", async () => {
  // Block 7 (09:40 to 11:50 Berlin) closed, and the clock on since 13:58.
  const draft: Draft = {
    ...DRAFT,
    ended_at: "2026-09-03T12:32:00Z",
    block_ids: [7],
    running_since: "2026-09-03T11:58:00Z",
  };
  render({ draft });
  await waitFor(() => expect(target.textContent).toContain("from 2 blocks on"));
  expect(text()).toContain(": 09:40 to 11:50, and 13:58 to now");
});

test("a refused log is said, and the draft stays open", async () => {
  const { outcomes } = render({ logWork: () => Promise.reject(new Error("already logged")) });
  button("Log 2h 44m to PAY-231")!.click();
  await waitFor(() => expect(target.textContent).toContain("already logged"));
  expect(outcomes).toEqual([]);
});

test("Esc closes it as a layer: a stop's draft leaves the clock stopped, any other is discarded", () => {
  for (const [stopped, expected] of [[true, "later"], [false, "discard"]] as const) {
    const layers = createLayers();
    const { outcomes } = render({ stopped, layers });
    expect(layers.open.map((layer) => layer.kind)).toEqual(["dialog"]);
    layers.open[0]!.close();
    expect(outcomes).toEqual([expected]);
    unmount(app!);
    app = undefined;
    expect(layers.open, "the dialog's layer outlived it").toEqual([]);
  }
});

test("leaving while the log is in flight does nothing: the log answers for itself", async () => {
  let resolve: (worklog: Worklog) => void = () => {};
  const layers = createLayers();
  const { outcomes, order } = render({ layers, logWork: () => new Promise((done) => (resolve = done)) });
  button("Log 2h 44m to PAY-231")!.click();
  flushSync();
  layers.open[0]!.close();
  expect(outcomes, "Esc while the clock was stopping said the time was unlogged").toEqual([]);
  await waitFor(() => expect(order).toEqual(["stop", "log"]));
  layers.open[0]!.close();
  expect(outcomes, "Esc during a send said the time was unlogged").toEqual([]);
  expect(button("Stop, log later")?.disabled).toBe(true);
  resolve(WORKLOG);
  await waitFor(() => expect(outcomes).toEqual(["logged"]));
});

test("a worklog the source's credential cannot make draws Log disabled saying why, and sends nothing (#2052)", async () => {
  const asked: WriteAsk[][] = [];
  const reason = "This Jira user cannot log work in PAY: it lacks the Work On Issues permission";
  const { sent, order, outcomes } = render({
    refusedWrites: (asks) => {
      asked.push(asks);
      return Promise.resolve([{ entity: TICKET, op: "log_work", reason }]);
    },
  });
  await waitFor(() => expect(button("Log 2h 44m to PAY-231")?.getAttribute("aria-disabled")).toBe("true"));
  expect(asked).toEqual([[{ entity: TICKET, op: "log_work" }]]);
  expect(button("Log 2h 44m to PAY-231")?.title).toBe(reason);
  expect(text()).toContain(reason);
  button("Log 2h 44m to PAY-231")!.click();
  await Promise.resolve();
  flushSync();
  // Neither the clock stopped for it nor a worklog sent; the other answers stay.
  expect(order).toEqual([]);
  expect(sent).toEqual([]);
  button("Stop, log later")!.click();
  expect(outcomes).toEqual(["later"]);
});
