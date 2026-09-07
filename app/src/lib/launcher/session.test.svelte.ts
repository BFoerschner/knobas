/**
 * The three rules a launcher box gets wrong, pinned one at a time.
 *
 * Debounce, sequencing and selection reset are all invisible when they work
 * and all indistinguishable from "the app is slow" or "the app is flaky" when
 * they do not. None of them is observable from a screenshot, so none of them
 * can be QA'd by driving the app — which is exactly why they live in a module
 * with no DOM in it.
 */
import { flushSync } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { LauncherHome, SearchQuery, SearchResponse } from "../ipc";
import { Session } from "./session.svelte";

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

/** A response carrying one hit per title, so a test can tell answers apart. */
function answer(raw: string, titles: string[] = ["one"]): SearchResponse {
  return {
    interpreted: {
      text: raw,
      prefix: null,
      filters: { sources: [], kinds: [], updated_within_days: null, mine: false, authors: [] },
      unknown_tokens: [],
    },
    groups: [
      {
        kind: "ticket",
        label: "Ticket",
        plural: "Tickets",
        monogram: "TK",
        total: titles.length,
        hits: titles.map((title, i) => ({
          entity_id: `jira:${title}-${i}`,
          kind: "ticket",
          source_id: "jira",
          updated_at: null,
          synced_at: "2026-08-25T12:00:00Z",
          path: null,
          title,
          rank: 1,
          snippet: [],
        })),
      },
    ],
    total: titles.length,
    took_ms: 1,
    coverage: [],
  };
}

const EMPTY_HOME: LauncherHome = {
  smart_lists: [],
  recent: [],
  sources: [],
  pending_writes: 0,
};

function ports(search: (q: SearchQuery) => Promise<SearchResponse>) {
  return { search, launcherHome: async () => EMPTY_HOME };
}

test("a word typed one letter at a time is one query, not five", async () => {
  const search = vi.fn(async (q: SearchQuery) => answer(q.raw));
  const session = new Session(ports(search));

  for (const raw of ["s", "se", "sep", "sepa"]) {
    session.type(raw);
    // Four keystrokes inside one debounce window.
    await vi.advanceTimersByTimeAsync(20);
  }
  expect(search).not.toHaveBeenCalled();

  await vi.advanceTimersByTimeAsync(90);
  expect(search).toHaveBeenCalledTimes(1);
  expect(search.mock.calls[0]?.[0]?.raw).toBe("sepa");
});

/**
 * The bug this exists for: a broad query is slow, the next keystroke narrows
 * it, the narrow answer paints — and then the broad one lands and replaces it.
 * The rows under the cursor become an answer to a question nobody is asking,
 * and it only happens on a corpus big enough for two queries to overtake.
 */
test("an answer overtaken by a newer one is dropped", async () => {
  const gates: (() => void)[] = [];
  const search = vi.fn(
    (q: SearchQuery) =>
      new Promise<SearchResponse>((resolve) => {
        gates.push(() => resolve(answer(q.raw, [q.raw])));
      }),
  );
  const session = new Session(ports(search));

  session.set("slow");
  session.set("fast");
  expect(search).toHaveBeenCalledTimes(2);

  // The *newer* request answers first…
  gates[1]?.();
  await vi.advanceTimersByTimeAsync(0);
  expect(session.response?.interpreted.text).toBe("fast");

  // …and the older one lands afterwards and must change nothing.
  gates[0]?.();
  await vi.advanceTimersByTimeAsync(0);
  expect(
    session.response?.interpreted.text,
    "a stale answer overwrote a newer one — the box will read as jumping",
  ).toBe("fast");
});

test("a failure from an overtaken request does not clear a good answer", async () => {
  const gates: { resolve: (r: SearchResponse) => void; reject: (e: unknown) => void }[] = [];
  const search = vi.fn(
    () => new Promise<SearchResponse>((resolve, reject) => gates.push({ resolve, reject })),
  );
  const session = new Session(ports(search));

  session.set("slow");
  session.set("fast");
  gates[1]?.resolve(answer("fast"));
  await vi.advanceTimersByTimeAsync(0);

  gates[0]?.reject({ code: "internal", message: "boom" });
  await vi.advanceTimersByTimeAsync(0);

  expect(session.error).toBeNull();
  expect(session.response?.interpreted.text).toBe("fast");
});

test("a real failure is reported, with the code's message", async () => {
  const session = new Session(
    ports(async () => {
      throw { code: "invalid", message: "query is 600 characters", source_id: null };
    }),
  );
  session.set("x");
  await vi.advanceTimersByTimeAsync(0);
  expect(session.error).toBe("query is 600 characters");
  expect(session.response).toBeNull();
});

test("the selection resets to the first row on every new answer", async () => {
  const session = new Session(ports(async (q) => answer(q.raw, ["a", "b", "c"])));

  session.set("first");
  await vi.advanceTimersByTimeAsync(0);
  flushSync();
  session.move(2);
  expect(session.selected).toBe(2);

  session.set("second");
  await vi.advanceTimersByTimeAsync(0);
  expect(
    session.selected,
    "the row under the cursor is not in the new list, so keeping the index points at a different item",
  ).toBe(0);
});

test("the cursor clamps at both ends rather than wrapping", async () => {
  const session = new Session(ports(async (q) => answer(q.raw, ["a", "b"])));
  session.set("x");
  await vi.advanceTimersByTimeAsync(0);
  flushSync();

  session.move(-1);
  expect(session.selected).toBe(0);
  session.move(5);
  expect(session.selected).toBe(1);
});

test("clearing the box drops the answer and goes back to the board", async () => {
  const search = vi.fn(async (q: SearchQuery) => answer(q.raw));
  const session = new Session(ports(search));

  session.set("sepa");
  await vi.advanceTimersByTimeAsync(0);
  flushSync();
  expect(session.mode).toBe("results");

  session.set("");
  await vi.advanceTimersByTimeAsync(0);
  flushSync();
  expect(session.mode).toBe("board");
  expect(session.response).toBeNull();
  // And an empty box asks the backend nothing: a query with neither text nor
  // a filter is one the engine refuses anyway.
  expect(search).toHaveBeenCalledTimes(1);
});

test("closing the box cancels a keystroke that has not fired yet", async () => {
  const search = vi.fn(async (q: SearchQuery) => answer(q.raw));
  const session = new Session(ports(search));

  session.type("sep");
  session.dispose();
  await vi.advanceTimersByTimeAsync(500);
  expect(search).not.toHaveBeenCalled();
});

/**
 * The panel is chosen from the *backend's* interpretation, never from the raw
 * text (ruling P2). A frontend that read the leading `?` itself would be a
 * second grammar, and the `?` card is the one surface where a second grammar
 * is guaranteed to be noticed last.
 */
test("the panel follows the backend's prefix, not the typed characters", async () => {
  const session = new Session(
    ports(async (q) => {
      const response = answer(q.raw, []);
      response.groups = [];
      response.interpreted.prefix = q.raw.startsWith("?") ? "help" : null;
      return response;
    }),
  );

  session.raw = "?";
  expect(session.mode, "no answer yet, so nothing has been interpreted").toBe("results");

  session.set("?");
  await vi.advanceTimersByTimeAsync(0);
  flushSync();
  expect(session.mode).toBe("help");
  expect(session.rows.length).toBeGreaterThan(8);
});

// -- a pasted link (#496) ----------------------------------------------------

/**
 * The link the session goes to instead of the search engine.
 *
 * Two rules live here and neither is visible on screen: which of the two
 * backend reads a keystroke reaches, and whether the resolver's answers are
 * sequenced the way the search's are. Both are exactly what this module
 * exists to hold.
 */
const PASTED = "https://jira.example/browse/PAY-231";
const FOUND = { entity_id: "jira:PAY-231", kind: "ticket" };

test("a pasted link goes to the resolver and never to the search engine", async () => {
  const search = vi.fn(async (q: SearchQuery) => answer(q.raw));
  const resolveUrl = vi.fn(async () => FOUND);
  const session = new Session({ ...ports(search), resolveUrl });

  session.type(PASTED);
  await vi.advanceTimersByTimeAsync(120);

  expect(resolveUrl).toHaveBeenCalledWith(PASTED);
  expect(search, "a link is not a query").not.toHaveBeenCalled();
  expect(session.mode).toBe("url");
  expect(session.urlAnswer).toEqual({ match: FOUND });
  // Nothing to walk: the answer is navigated to, not selected.
  expect(session.rows).toEqual([]);
});

test("a link the mirror does not hold is a miss, not an empty answer", async () => {
  const resolveUrl = vi.fn(async () => null);
  const session = new Session({ ...ports(async (q) => answer(q.raw)), resolveUrl });

  session.type(PASTED);
  await vi.advanceTimersByTimeAsync(120);

  expect(session.urlAnswer).toEqual({ match: null });
  expect(session.error, "a miss is an answer, not a failure").toBeNull();
});

/**
 * The two `null`s are different answers, and the difference is what stops a
 * paste flashing *Not in the mirror* for the length of a round trip.
 */
test("while the resolver is being asked there is no answer yet", async () => {
  let settle: ((match: null) => void) | undefined;
  const session = new Session({
    ...ports(async (q) => answer(q.raw)),
    resolveUrl: () => new Promise((resolve) => (settle = resolve)),
  });

  session.type(PASTED);
  await vi.advanceTimersByTimeAsync(120);
  expect(session.mode, "the box already knows it holds a link").toBe("url");
  expect(session.urlAnswer, "and does not yet know what it names").toBeNull();
  expect(session.pending).toBe(true);

  settle!(null);
  await vi.advanceTimersByTimeAsync(0);
  expect(session.urlAnswer).toEqual({ match: null });
  expect(session.pending).toBe(false);
});

/**
 * The sequencing rule the search has, on the read that *navigates*: a slow
 * answer to a link the reader has already replaced must not be acted on, or
 * the launcher opens an entity nobody asked for.
 */
test("a slow answer to an older paste is dropped", async () => {
  const slow = { entity_id: "jira:OLD-1", kind: "ticket" };
  const answers: Record<string, { entity_id: string; kind: string }> = {
    "https://jira.example/browse/OLD-1": slow,
    [PASTED]: FOUND,
  };
  const held: (() => void)[] = [];
  const session = new Session({
    ...ports(async (q) => answer(q.raw)),
    resolveUrl: (url) =>
      new Promise((resolve) => held.push(() => resolve(answers[url] ?? null))),
  });

  session.set("https://jira.example/browse/OLD-1");
  session.set(PASTED);
  // The newer answer lands first, the older one after it.
  held[1]!();
  await vi.advanceTimersByTimeAsync(0);
  held[0]!();
  await vi.advanceTimersByTimeAsync(0);

  expect(session.urlAnswer).toEqual({ match: FOUND });
});

test("a session with no resolver searches a link like any other query", async () => {
  // The two link pickers built on this class: they choose a target from the
  // corpus, and a paste there is not a request to navigate anywhere.
  const search = vi.fn(async (q: SearchQuery) => answer(q.raw));
  const session = new Session(ports(search));

  session.type(PASTED);
  await vi.advanceTimersByTimeAsync(120);

  expect(search).toHaveBeenCalledTimes(1);
  expect(session.mode).toBe("results");
});

test("clearing the box forgets the paste", async () => {
  const session = new Session({
    ...ports(async (q) => answer(q.raw)),
    resolveUrl: async () => FOUND,
  });

  session.type(PASTED);
  await vi.advanceTimersByTimeAsync(120);
  expect(session.mode).toBe("url");

  session.type("");
  await vi.advanceTimersByTimeAsync(120);
  expect(session.mode).toBe("board");
  expect(session.url).toBeNull();
  expect(session.urlAnswer).toBeNull();
});

/**
 * The box is emptied on close, and only for a paste: what the launcher
 * navigates on must not be there to navigate on again, and a URL left in the
 * box with its answer gone is a box that says *Searching…* with nothing in
 * flight.
 */
test("closing after a paste leaves nothing to reopen onto", async () => {
  const session = new Session({
    ...ports(async (q) => answer(q.raw)),
    resolveUrl: async () => FOUND,
  });

  session.type(PASTED);
  await vi.advanceTimersByTimeAsync(120);
  session.dispose();

  expect(session.raw).toBe("");
  expect(session.mode).toBe("board");
  expect(session.urlAnswer).toBeNull();
});

test("closing after a search still comes back to it", async () => {
  const session = new Session({
    ...ports(async (q) => answer(q.raw)),
    resolveUrl: async () => FOUND,
  });

  session.type("sepa");
  await vi.advanceTimersByTimeAsync(120);
  session.dispose();

  expect(session.raw, "a query is what the reader was in the middle of").toBe("sepa");
  expect(session.mode).toBe("results");
});
