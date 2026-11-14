/** `drafts`: typed words by key, for the session. */
import { afterEach, expect, test } from "vitest";

import { createDrafts, drafts } from "./drafts.svelte";

afterEach(() => {
  drafts.reset();
});

test("words stay under their own key", () => {
  drafts.put("comment:a", "one");
  drafts.put("comment:b", "two");
  expect(drafts.get("comment:a")).toBe("one");
  expect(drafts.get("comment:b")).toBe("two");
  expect(drafts.get("comment:c")).toBe("");
});

test("blank words are no draft", () => {
  drafts.put("comment:a", "one");
  drafts.put("comment:a", "  ");
  expect(drafts.has("comment:a")).toBe(false);
});

test("forget drops one and leaves the rest", () => {
  drafts.put("comment:a", "one");
  drafts.put("comment:b", "two");
  drafts.forget("comment:a");
  expect(drafts.has("comment:a")).toBe(false);
  expect(drafts.get("comment:b")).toBe("two");
});

test("a store of its own holds any shape, and null drops it", () => {
  const forms = createDrafts<{ name: string }>();
  forms.put("x", { name: "a" });
  expect(forms.held("x")).toEqual({ name: "a" });
  forms.put("x", null);
  expect(forms.has("x")).toBe(false);
});
