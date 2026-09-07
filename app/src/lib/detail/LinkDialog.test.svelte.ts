/**
 * *Link to…*, driven the way a person drives it: from the keyboard.
 *
 * The IPC is mocked at the module boundary, so what is exercised here is the
 * dialog's own behaviour — the picker's search, the default relation, the
 * arguments the write is given, and the one outcome that is not a fault
 * ("already linked"), which has to read as a sentence inside the dialog rather
 * than as a raw error somewhere else.
 */
import { flushSync, mount, unmount } from "svelte";
import { beforeEach, expect, test, vi } from "vitest";

import type { SearchQuery, SearchResponse } from "../ipc/search";

/** Every query the picker asked, in order. */
const queries: string[] = [];
let answer: (query: SearchQuery) => Promise<SearchResponse> = () => Promise.resolve(response([]));

vi.mock("../ipc/search", () => ({
  search: (query: SearchQuery) => {
    queries.push(query.raw);
    return answer(query);
  },
  launcherHome: () => Promise.reject(new Error("the dialog never loads the board")),
  noFilters: () => ({ sources: [], kinds: [], updated_within_days: null, mine: false, authors: [] }),
}));

/** Every `create_link` the dialog issued. */
const writes: {
  fromId: string;
  toId: string;
  relation: string | undefined;
  note: string | undefined;
}[] = [];
let writeFails: unknown = null;

vi.mock("../ipc/entity", () => ({
  // The ticket detail's status select (#179) reads the granted board and
  // queues through the write queue. Not what this file is about, so both
  // answer with nothing.
  miniBoard: () => Promise.resolve({ columns: [], sources: [] }),
  submitWrite: () => Promise.reject(new Error("no write in this test")),
  createLink: async (fromId: string, toId: string, relation?: string, note?: string) => {
    writes.push({ fromId, toId, relation, note });
    if (writeFails) throw writeFails;
    return {};
  },
}));

const { default: LinkDialog } = await import("./LinkDialog.svelte");

function hit(over: { id: string; kind?: string; title: string }) {
  return {
    entity_id: over.id,
    kind: over.kind ?? "ticket",
    source_id: "mock",
    updated_at: null,
    synced_at: "2026-08-28T09:30:00Z",
    path: null,
    title: over.title,
    rank: 1,
    snippet: [],
  };
}

function response(hits: ReturnType<typeof hit>[]): SearchResponse {
  return {
    interpreted: { prefix: "none", text: "", segments: [], filters: null } as never,
    groups: hits.length === 0
      ? []
      : [
          {
            kind: "ticket",
            label: "Ticket",
            plural: "Tickets",
            monogram: "TI",
            total: hits.length,
            hits,
          },
        ],
    total: hits.length,
    took_ms: 3,
    coverage: [],
  };
}

function render(over: { fromId?: string; fromTitle?: string } = {}) {
  const target = document.createElement("div");
  document.body.append(target);
  const onclose = vi.fn();
  const oncreated = vi.fn();
  const app = mount(LinkDialog, {
    target,
    props: {
      fromId: over.fromId ?? "mock:PAY-231",
      fromTitle: over.fromTitle ?? "Retry failed SEPA payouts",
      onclose,
      oncreated,
    },
  });
  flushSync();
  const field = (label: string) =>
    [...target.querySelectorAll<HTMLLabelElement>("label")]
      .filter((node) => node.textContent?.includes(label))
      .map((node) => target.querySelector<HTMLInputElement>(`#${node.htmlFor}`))[0]!;
  return {
    target,
    onclose,
    oncreated,
    picker: () => field("What to link to"),
    relation: () => field("How it relates"),
    note: () => field("Why"),
    text: () => (target.textContent ?? "").replace(/\s+/g, " "),
    options: () => [...target.querySelectorAll<HTMLElement>('[role="option"]')],
    button: (label: string) =>
      [...target.querySelectorAll<HTMLButtonElement>("button")].find(
        (node) => node.textContent?.trim() === label,
      )!,
    /** Type into an input the way a person does: the value, then the event. */
    type: (input: HTMLInputElement, value: string) => {
      input.value = value;
      input.dispatchEvent(new Event("input", { bubbles: true }));
      flushSync();
    },
    press: (node: HTMLElement, key: string) => {
      const event = new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true });
      node.dispatchEvent(event);
      flushSync();
      return event;
    },
    done: () => {
      unmount(app);
      target.remove();
    },
  };
}

beforeEach(() => {
  queries.length = 0;
  writes.length = 0;
  writeFails = null;
  answer = () => Promise.resolve(response([]));
});

/** Story 4: a quick link costs no extra decisions. */
test("the relation is pre-filled with related, and the curated list is offered", () => {
  const screen = render();

  expect(screen.relation().value).toBe("related");
  const offered = [...screen.target.querySelectorAll("datalist option")].map((o) =>
    o.getAttribute("value"),
  );
  expect(offered).toContain("blocks");
  expect(offered).toContain("documents");
  expect(offered).toContain("depends-on");

  screen.done();
});

/** Story 2: the picker is the launcher — it debounces and it asks the backend. */
test("the target picker searches what is typed", async () => {
  answer = () => Promise.resolve(response([hit({ id: "mock:PAY-228", title: "Payout retries pile up" })]));
  const screen = render();

  screen.type(screen.picker(), "payout");
  await vi.waitFor(() => expect(screen.options()).toHaveLength(1));
  flushSync();

  expect(queries).toEqual(["payout"]);
  expect(screen.text()).toContain("Payout retries pile up");

  screen.done();
});

/**
 * Story 1 and 23 together: found by typing, picked with the arrow keys and
 * Enter, written with the arguments the dialog was holding — no mouse.
 */
test("the whole flow is keyboard-only, and writes what was chosen", async () => {
  answer = () =>
    Promise.resolve(
      response([
        hit({ id: "mock:PAY-228", title: "Payout retries pile up" }),
        hit({ id: "mock:PAY-300", title: "Payout ledger drift" }),
      ]),
    );
  const screen = render();

  screen.type(screen.picker(), "payout");
  await vi.waitFor(() => expect(screen.options()).toHaveLength(2));
  flushSync();

  // Down to the second result, then Enter to take it.
  screen.press(screen.picker(), "ArrowDown");
  screen.press(screen.picker(), "Enter");
  expect(screen.text()).toContain("Payout ledger drift");

  screen.type(screen.relation(), "blocks");
  screen.type(screen.note(), "the ledger cannot settle until this lands");
  // Enter in a field is the same as pressing Link.
  screen.press(screen.note(), "Enter");

  await vi.waitFor(() => expect(writes).toHaveLength(1));
  expect(writes[0]).toEqual({
    fromId: "mock:PAY-231",
    toId: "mock:PAY-300",
    relation: "blocks",
    note: "the ledger cannot settle until this lands",
  });
  expect(screen.oncreated).toHaveBeenCalled();

  screen.done();
});

/** Story 3: a relation nobody curated is still a relation. */
test("a free-typed relation is sent as typed", async () => {
  answer = () => Promise.resolve(response([hit({ id: "mock:PAY-228", title: "Payout retries" })]));
  const screen = render();

  screen.type(screen.picker(), "payout");
  await vi.waitFor(() => expect(screen.options()).toHaveLength(1));
  screen.press(screen.picker(), "Enter");

  screen.type(screen.relation(), "supersedes-eventually");
  screen.button("Link").click();

  await vi.waitFor(() => expect(writes).toHaveLength(1));
  expect(writes[0]!.relation).toBe("supersedes-eventually");

  screen.done();
});

/**
 * Story 14. `conflict` is a state, not a fault, and the reader is still
 * standing in the dialog — so the sentence belongs there, the dialog stays
 * open, and the backend's own message (which names ids and a constraint) is
 * not what is shown.
 */
test("already linked is surfaced inline, and the dialog stays open", async () => {
  answer = () => Promise.resolve(response([hit({ id: "mock:PAY-228", title: "Payout retries" })]));
  writeFails = {
    code: "conflict",
    message: 'duplicate key value violates unique constraint "link_active_idx"',
    source_id: null,
  };
  const screen = render();

  screen.type(screen.picker(), "payout");
  await vi.waitFor(() => expect(screen.options()).toHaveLength(1));
  screen.press(screen.picker(), "Enter");
  screen.button("Link").click();

  await vi.waitFor(() => expect(screen.text()).toContain("Already linked"));
  flushSync();

  expect(screen.target.querySelector('[role="alert"]')?.textContent).toContain("Already linked");
  expect(screen.text()).not.toContain("link_active_idx");
  expect(screen.onclose).not.toHaveBeenCalled();
  expect(screen.oncreated).not.toHaveBeenCalled();
  // ...and the dialog is still usable: the target is still chosen.
  expect(screen.text()).toContain("Payout retries");

  screen.done();
});

/** Any other refusal keeps its own words — `not_found` names what has not synced. */
test("a refusal that is not a conflict keeps the backend's sentence", async () => {
  answer = () => Promise.resolve(response([hit({ id: "mock:PAY-228", title: "Payout retries" })]));
  writeFails = {
    code: "not_found",
    message: "mock:PAY-228 is not in the local index",
    source_id: null,
  };
  const screen = render();

  screen.type(screen.picker(), "payout");
  await vi.waitFor(() => expect(screen.options()).toHaveLength(1));
  screen.press(screen.picker(), "Enter");
  screen.button("Link").click();

  await vi.waitFor(() => expect(screen.text()).toContain("is not in the local index"));

  screen.done();
});

/**
 * Story 23's ladder: one rung per press. A search somebody typed is a step, so
 * the first Esc undoes it and the dialog stays; only an empty box lets the key
 * through to `Modal`.
 */
test("Esc clears the search first and only then closes the dialog", async () => {
  answer = () => Promise.resolve(response([hit({ id: "mock:PAY-228", title: "Payout retries" })]));
  const screen = render();

  screen.type(screen.picker(), "payout");
  await vi.waitFor(() => expect(screen.options()).toHaveLength(1));
  flushSync();

  const first = screen.press(screen.picker(), "Escape");
  expect(first.defaultPrevented, "the first Esc is the picker's own rung").toBe(true);
  expect(screen.onclose).not.toHaveBeenCalled();
  expect(screen.picker().value).toBe("");

  screen.press(screen.picker(), "Escape");
  expect(screen.onclose).toHaveBeenCalledTimes(1);

  screen.done();
});

/** Nothing can be written before a target is chosen. */
test("Link is unavailable until a target is picked", async () => {
  answer = () => Promise.resolve(response([hit({ id: "mock:PAY-228", title: "Payout retries" })]));
  const screen = render();

  expect(screen.button("Link").disabled).toBe(true);

  screen.type(screen.picker(), "payout");
  await vi.waitFor(() => expect(screen.options()).toHaveLength(1));
  screen.press(screen.picker(), "Enter");

  expect(screen.button("Link").disabled).toBe(false);
  expect(writes).toHaveLength(0);

  screen.done();
});

/** A hit's title is source text, and is rendered as text (gotcha 7). */
test("a result title is text, whatever a source put in it", async () => {
  answer = () => Promise.resolve(response([hit({ id: "mock:PAY-228", title: "<em>Payout</em>" })]));
  const screen = render();

  screen.type(screen.picker(), "payout");
  await vi.waitFor(() => expect(screen.options()).toHaveLength(1));
  flushSync();

  expect(screen.text()).toContain("<em>Payout</em>");
  expect(screen.target.querySelector("em")).toBeNull();

  screen.done();
});

/**
 * Issue #445: attaching a monitor **from the monitor's end**.
 *
 * One relation reaches the database — `monitored-by`, the word the import
 * writes and every estate read filters on — and the row runs from the asset,
 * because that is the direction the sentence goes. The dialog was opened over
 * the monitor, so the ends come back swapped: a dialog that stored `monitors`,
 * or stored `monitored-by` from the monitor, would put a link in the panel
 * that reads *this monitor is monitored by that container*.
 */
test("picking monitors from a monitor writes monitored-by, drawn from the asset", async () => {
  answer = () =>
    Promise.resolve(response([hit({ id: "asset:knobas-gitea", title: "knobas-gitea" })]));
  const screen = render({ fromId: "kuma:7", fromTitle: "gitea" });

  screen.type(screen.picker(), "gitea");
  await vi.waitFor(() => expect(screen.options()).toHaveLength(1));
  screen.press(screen.picker(), "Enter");

  screen.button("monitors").click();
  screen.button("Link").click();

  await vi.waitFor(() => expect(writes).toHaveLength(1));
  expect(writes[0]).toEqual({
    fromId: "asset:knobas-gitea",
    toId: "kuma:7",
    relation: "monitored-by",
    note: "",
  });

  screen.done();
});

/** The same attachment asked for from the asset, which needs no swap. */
test("picking monitored-by from an asset writes the same row", async () => {
  answer = () => Promise.resolve(response([hit({ id: "kuma:7", title: "gitea" })]));
  const screen = render({ fromId: "asset:knobas-gitea", fromTitle: "knobas-gitea" });

  screen.type(screen.picker(), "gitea");
  await vi.waitFor(() => expect(screen.options()).toHaveLength(1));
  screen.press(screen.picker(), "Enter");

  screen.button("monitored-by").click();
  screen.button("Link").click();

  await vi.waitFor(() => expect(writes).toHaveLength(1));
  expect(writes[0]).toEqual({
    fromId: "asset:knobas-gitea",
    toId: "kuma:7",
    relation: "monitored-by",
    note: "",
  });

  screen.done();
});
