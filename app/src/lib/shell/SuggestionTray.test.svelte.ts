/**
 * The tray, and the five things it exists to get right.
 *
 * A proposal has to say *what* is being connected (two kinds and two titles,
 * not two ids), *why* knobas thinks so, and *how much that is worth* — and it
 * has to be answerable in one press, with the answer visibly taken. Every one
 * of those is a sentence or an action a reader relies on, so every one is
 * asserted against the literal words rather than against a class name.
 *
 * The sixth thing is not visible at all and is the reason the read is mocked
 * rather than stubbed out: the tray is a **query**. Accepting must produce a
 * fresh read, because a tray that spliced the row out locally would be a second
 * copy of the truth, right until it disagreed with the first.
 */
import { flushSync, mount, unmount } from "svelte";
import { beforeEach, expect, test, vi } from "vitest";

import type { SuggestionEntry, SuggestionPage } from "../ipc/entity";

/**
 * Recorders rather than `vi.fn`s, for the reason `Tile.test.svelte.ts` gives:
 * a `vi.fn` keeps the promise its implementation returned so `settledResults`
 * can report it, and the derived rejected promise it leaves behind is reported
 * as an unrelated "Unknown Error" that looks exactly like the product bug these
 * tests exist to catch.
 */
const calls: string[] = [];
let reads: (() => Promise<SuggestionPage>)[] = [];
let answered: (() => Promise<void>) | null = null;

vi.mock("../ipc/entity", () => ({
  detectSuggestions: () => {
    calls.push("detect");
    return Promise.resolve(0);
  },
  roomSuggestions: (sources: string[], limit: number) => {
    calls.push(`read ${JSON.stringify(sources)} ${limit}`);
    const next = reads.shift() ?? (() => Promise.resolve(page([])));
    return next();
  },
  acceptSuggestion: (id: string) => {
    calls.push(`accept ${id}`);
    return answered ? answered() : Promise.resolve();
  },
  dismissSuggestion: (id: string) => {
    calls.push(`dismiss ${id}`);
    return answered ? answered() : Promise.resolve();
  },
}));

/** No Tauri bridge in jsdom; the tray's sync listener must still install. */
vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => {}),
}));

const { default: SuggestionTray } = await import("./SuggestionTray.svelte");

function entry(over: Partial<SuggestionEntry["link"]> & { fromTitle?: string; toTitle?: string } = {}): SuggestionEntry {
  const { fromTitle, toTitle, ...link } = over;
  return {
    link: {
      id: "00000000-0000-0000-0000-000000000001",
      from_id: "gitea:tidewater/payout#b1",
      to_id: "mock:PAY-231",
      relation: "related",
      origin: "suggested",
      note: null,
      created_by: "knobas",
      created_at: "2026-08-29T09:30:00Z",
      confirmed_at: null,
      rule: "branch_name_key",
      rule_class: "exact_key",
      reason: "the branch name contains PAY-231",
      ...link,
    },
    from: {
      entity_id: "gitea:tidewater/payout#b1",
      kind: "branch",
      title: fromTitle ?? "feature/PAY-231-sepa-retry",
      deleted_at: null,
    },
    to: {
      entity_id: "mock:PAY-231",
      kind: "ticket",
      title: toTitle ?? "Retry failed SEPA payouts",
      deleted_at: null,
    },
  };
}

function page(rows: SuggestionEntry[], total = rows.length): SuggestionPage {
  return { rows, total };
}

const opened: string[] = [];

function render(sources: string[] = []) {
  const target = document.createElement("div");
  document.body.append(target);
  const app = mount(SuggestionTray, {
    target,
    props: { sources, onopen: (hash: string) => opened.push(hash) },
  });
  flushSync();
  return { target, app };
}

/** Let every queued microtask settle, so a chain of awaits has finished. */
async function settle() {
  for (let i = 0; i < 8; i += 1) await Promise.resolve();
  flushSync();
}

beforeEach(() => {
  calls.length = 0;
  opened.length = 0;
  reads = [];
  answered = null;
  document.body.innerHTML = "";
});

test("runs a detection pass before its first read, scoped to the room", async () => {
  reads = [() => Promise.resolve(page([entry()]))];
  const { target, app } = render(["gitea"]);
  await settle();

  expect(calls[0]).toBe("detect");
  expect(calls[1]).toBe('read ["gitea"] 50');
  unmount(app);
  target.remove();
});

test("a row names both ends, says why, and says how much that is worth", async () => {
  reads = [() => Promise.resolve(page([entry()]))];
  const { target, app } = render();
  await settle();

  const row = target.querySelector(".row.sug");
  expect(row?.textContent).toContain("feature/PAY-231-sepa-retry");
  expect(row?.textContent).toContain("Retry failed SEPA payouts");
  expect(row?.textContent).toContain("the branch name contains PAY-231");
  // The class, which is what a reader calibrates trust on.
  expect(row?.textContent).toContain("exact");
  expect(target.querySelector(".cls.guess")).toBeNull();

  unmount(app);
  target.remove();
});

test("a guess is marked as one and an exact match is not", async () => {
  reads = [
    () =>
      Promise.resolve(
        page([
          entry(),
          entry({
            id: "00000000-0000-0000-0000-000000000002",
            rule: "similar_text",
            rule_class: "similarity",
            reason: "both mention backoff, jitter, retri",
          }),
        ]),
      ),
  ];
  const { target, app } = render();
  await settle();

  const badges = [...target.querySelectorAll(".cls")].map((node) => ({
    word: node.textContent?.trim(),
    guess: node.classList.contains("guess"),
  }));
  expect(badges).toEqual([
    { word: "exact", guess: false },
    { word: "guess", guess: true },
  ]);

  unmount(app);
  target.remove();
});

test("the heading answers whether it is worth looking", async () => {
  reads = [() => Promise.resolve(page([entry()], 7))];
  const { target, app } = render();
  await settle();

  // The room's total, not the page's — the tray holds one row and says seven.
  expect(target.querySelector(".cnt")?.textContent?.trim()).toBe("7 waiting");

  unmount(app);
  target.remove();
});

test("an empty tray keeps its heading and draws no rows", async () => {
  reads = [() => Promise.resolve(page([]))];
  const { target, app } = render();
  await settle();

  expect(target.querySelector(".cnt")?.textContent?.trim()).toBe("none waiting");
  expect(target.querySelectorAll(".row.sug").length).toBe(0);

  unmount(app);
  target.remove();
});

/**
 * A failed read must not look like a room with nothing to suggest — the two
 * are the same picture and only one of them is true.
 */
test("a failed read says so instead of rendering empty", async () => {
  reads = [() => Promise.reject({ code: "internal", message: "the database went away" })];
  const { target, app } = render();
  await settle();

  expect(target.querySelector(".fail")?.textContent).toContain("the database went away");

  unmount(app);
  target.remove();
});

test("accepting answers the proposal and then re-reads, rather than splicing it out", async () => {
  reads = [
    () => Promise.resolve(page([entry()])),
    // What the query says afterwards. The tray must show *this*, not its own
    // idea of what accepting did.
    () => Promise.resolve(page([])),
  ];
  const { target, app } = render();
  await settle();

  target.querySelector<HTMLButtonElement>(".btn.pri")?.click();
  await settle();

  expect(calls).toEqual([
    "detect",
    "read [] 50",
    "accept 00000000-0000-0000-0000-000000000001",
    "read [] 50",
  ]);
  expect(
    calls.filter((call) => call === "detect").length,
    "answering a proposal must not re-scan the mirror",
  ).toBe(1);
  expect(target.querySelectorAll(".row.sug").length).toBe(0);

  unmount(app);
  target.remove();
});

test("dismissing answers the proposal and then re-reads", async () => {
  reads = [() => Promise.resolve(page([entry()])), () => Promise.resolve(page([]))];
  const { target, app } = render();
  await settle();

  target.querySelector<HTMLButtonElement>(".btn.ghost")?.click();
  await settle();

  expect(calls).toContain("dismiss 00000000-0000-0000-0000-000000000001");
  expect(target.querySelectorAll(".row.sug").length).toBe(0);

  unmount(app);
  target.remove();
});

/**
 * Both ends, because #41's story 20 is about checking an ambiguous suggestion
 * *before* answering it — and a row that only opened one end would make the
 * cheap half of that impossible.
 */
test("either end of a proposal opens", async () => {
  reads = [() => Promise.resolve(page([entry()]))];
  const { target, app } = render();
  await settle();

  const ends = [...target.querySelectorAll<HTMLButtonElement>(".row.sug .open")];
  expect(ends.length).toBe(2);
  for (const end of ends) end.click();

  expect(opened.length).toBe(2);
  // Through `hashFor`, so the `#` and `/` an entity key carries are encoded --
  // an unencoded one truncates the fragment at the browser level. The kind is
  // the row's own, so each end opens as what it is.
  expect(opened[0]).toBe("#/branch/gitea:tidewater%2Fpayout%23b1");
  expect(opened[1]).toBe("#/ticket/mock:PAY-231");

  unmount(app);
  target.remove();
});

/**
 * A proposal may point at something the source withdrew between the pass that
 * found it and the reader looking at it. §5a keeps the entity so the link never
 * dangles; the row is marked rather than dropped.
 */
test("a withdrawn end is marked rather than hidden", async () => {
  reads = [
    () => {
      const one = entry();
      one.to.deleted_at = "2026-08-29T10:00:00Z";
      return Promise.resolve(page([one]));
    },
  ];
  const { target, app } = render();
  await settle();

  expect(target.querySelectorAll(".row.sug").length).toBe(1);
  expect(target.querySelector(".wd")?.textContent).toBe("withdrawn");

  unmount(app);
  target.remove();
});

/** A double press is one answer: the buttons go dead while the write is out. */
test("a proposal cannot be answered twice while its write is in flight", async () => {
  let release!: () => void;
  answered = () => new Promise<void>((resolve) => (release = resolve));
  reads = [() => Promise.resolve(page([entry()])), () => Promise.resolve(page([]))];
  const { target, app } = render();
  await settle();

  const accept = target.querySelector<HTMLButtonElement>(".btn.pri");
  accept?.click();
  await settle();
  expect(accept?.disabled).toBe(true);
  accept?.click();
  await settle();

  expect(calls.filter((call) => call.startsWith("accept")).length).toBe(1);
  release();
  await settle();

  unmount(app);
  target.remove();
});
