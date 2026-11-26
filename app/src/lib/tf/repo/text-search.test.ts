import { afterEach, beforeEach, expect, test, vi } from "vitest";
import type { TextFound, TextQuery } from "../../ipc/entity";
import { askText, takeAsked, TextSearch, type TextPort } from "./text-search.svelte";

function found(over: Partial<TextFound> = {}): TextFound {
  return { from: "clone", commit: "abc", branch: null, files: [], names: [], searched: 1, skipped: 0, cut: false, invalid: null, why: null, stale: false, ...over };
}

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

test("Search in repository hands its words to that repository's Files tab once", () => {
  askText("gitea:acme/pay", "retry_on");
  expect(takeAsked("gitea:acme/other")).toBeNull();
  expect(takeAsked("gitea:acme/pay")).toBe("retry_on");
  expect(takeAsked("gitea:acme/pay")).toBeNull();
});

test("typing asks once it rests, with the switches, at the tree the tab shows", async () => {
  const asked: { at: string; commit: string | null; working: boolean; branch: string | null; query: TextQuery }[] = [];
  const port: TextPort = (at, commit, working, branch, query) => {
    asked.push({ at, commit, working, branch, query });
    return Promise.resolve(found());
  };
  const search = new TextSearch(port, () => ({ at: "gitea:acme/pay:feature", commit: "c0ffee", working: false, branch: "feature" }));
  search.typed = "re";
  search.ask();
  search.typed = "retry";
  search.caseSensitive = true;
  search.ask();
  expect(asked).toEqual([]);
  expect(search.reading).toBe(true);
  await vi.advanceTimersByTimeAsync(200);
  expect(asked).toEqual([
    { at: "gitea:acme/pay:feature", commit: "c0ffee", working: false, branch: "feature", query: { text: "retry", case_sensitive: true, regex: false, names: false } },
  ]);
  expect(search.found?.searched).toBe(1);
  expect(search.reading).toBe(false);
});

test("an older answer landing after a newer one, and a stale one, are dropped", async () => {
  const answers: ((value: TextFound) => void)[] = [];
  const port: TextPort = () => new Promise((resolve) => answers.push(resolve));
  const search = new TextSearch(port, () => ({ at: "r", commit: null, working: true, branch: null }));
  search.typed = "a";
  search.ask(true);
  await vi.advanceTimersByTimeAsync(0);
  search.typed = "ab";
  search.ask(true);
  await vi.advanceTimersByTimeAsync(0);
  answers[1]!(found({ searched: 2 }));
  await vi.advanceTimersByTimeAsync(0);
  answers[0]!(found({ searched: 1 }));
  await vi.advanceTimersByTimeAsync(0);
  expect(search.found?.searched).toBe(2);
  search.ask(true);
  await vi.advanceTimersByTimeAsync(0);
  answers[2]!(found({ searched: 3, stale: true }));
  await vi.advanceTimersByTimeAsync(0);
  expect(search.found?.searched).toBe(2);
  // The newest question stopped by another search is asked once more, and answered.
  expect(answers).toHaveLength(4);
  expect(search.reading).toBe(true);
  answers[3]!(found({ searched: 4 }));
  await vi.advanceTimersByTimeAsync(0);
  expect(search.found?.searched).toBe(4);
  expect(search.reading).toBe(false);
});

test("a newest question stopped twice says so and stops waiting", async () => {
  const answers: ((value: TextFound) => void)[] = [];
  const port: TextPort = () => new Promise((resolve) => answers.push(resolve));
  const search = new TextSearch(port, () => ({ at: "r", commit: null, working: false, branch: null }));
  search.typed = "a";
  search.ask(true);
  await vi.advanceTimersByTimeAsync(0);
  answers[0]!(found({ stale: true }));
  await vi.advanceTimersByTimeAsync(0);
  answers[1]!(found({ stale: true }));
  await vi.advanceTimersByTimeAsync(0);
  expect(answers).toHaveLength(2);
  expect(search.reading).toBe(false);
  expect(search.failed).toContain("Press Enter to search again");
});

test("clearing the words clears the answer and asks nothing", async () => {
  const port = vi.fn<TextPort>(() => Promise.resolve(found()));
  const search = new TextSearch(port, () => ({ at: "r", commit: null, working: false, branch: null }));
  search.typed = "x";
  search.ask(true);
  await vi.advanceTimersByTimeAsync(0);
  expect(search.found).not.toBeNull();
  search.typed = "  ";
  search.ask();
  await vi.advanceTimersByTimeAsync(500);
  expect(port).toHaveBeenCalledTimes(1);
  expect(search.found).toBeNull();
  expect(search.reading).toBe(false);
});

test("a failed read says why and keeps nothing stale", async () => {
  const search = new TextSearch(() => Promise.reject(new Error("no database")), () => ({ at: "r", commit: null, working: false, branch: null }));
  search.typed = "x";
  search.ask(true);
  await vi.advanceTimersByTimeAsync(0);
  expect(search.failed).toContain("no database");
  expect(search.found).toBeNull();
});
