/**
 * The §3a generic projection.
 *
 * *"An unknown kind gets a generic detail view — title, metadata fields
 * projected from the raw payload, body text, the links panel… A new ticket
 * system is browsable on day one."* This is the projector for that: it turns
 * whatever an adapter mirrored into rows a person can read, without knowing
 * anything about the adapter.
 *
 * Everything it produces is **text**. The projector never renders, never
 * escapes, and never pre-formats markup — `PayloadView` interpolates, and that
 * is the whole defence (gotcha 7).
 */
import { expect, test } from "vitest";

import { LONG_TEXT, projectPayload, type Projected } from "./payload";

/** The `text` of a field that has one — `undefined` for a nested value. */
const textOf = (field: Projected | undefined) =>
  field && "text" in field ? field.text : undefined;

test("projects scalars in declaration order", () => {
  const out = projectPayload({
    key: "PAY-231",
    status: "In Progress",
    story_points: 3,
    blocked: false,
  });
  expect(
    out.map((f) => [f.key, f.kind === "nested" ? f.summary : f.text]),
  ).toEqual([
    ["key", "PAY-231"],
    ["status", "In Progress"],
    ["story_points", "3"],
    ["blocked", "false"],
  ]);
  expect(out.every((f) => f.kind === "scalar")).toBe(true);
});

test("humanises keys without losing them", () => {
  expect(projectPayload({ story_points: 1 })[0]?.label).toBe("Story points");
  expect(projectPayload({ story_points: 1 })[0]?.key).toBe("story_points");
  expect(projectPayload({ "affected-services": 1 })[0]?.label).toBe(
    "Affected services",
  );
});

test("long strings become text blocks, nested values become summaries", () => {
  const long = "x".repeat(200);
  const out = projectPayload({
    desc: long,
    comments: [{ by: "mara" }, { by: "jonas" }],
    fields: { a: 1 },
  });
  expect(out[0]?.kind).toBe("text");
  expect(out[1]).toMatchObject({ kind: "nested", summary: "2 items" });
  expect(out[2]).toMatchObject({ kind: "nested", summary: "1 field" });
});

/**
 * The boundary, from both sides.
 *
 * A cutoff that had been dropped altogether would keep "200 characters is a
 * text block" green while turning every description into a one-line table
 * cell, so the short side is the half that matters.
 */
test("the long-string cutoff is a cutoff", () => {
  expect(projectPayload({ d: "x".repeat(LONG_TEXT) })[0]?.kind).toBe("scalar");
  expect(projectPayload({ d: "x".repeat(LONG_TEXT + 1) })[0]?.kind).toBe(
    "text",
  );
});

/** One is one, not "1 items". */
test("summaries count in the singular too", () => {
  expect(projectPayload({ a: ["one"] })[0]).toMatchObject({
    summary: "1 item",
  });
  expect(projectPayload({ a: [] })[0]).toMatchObject({ summary: "0 items" });
  expect(projectPayload({ a: { x: 1, y: 2 } })[0]).toMatchObject({
    summary: "2 fields",
  });
  expect(projectPayload({ a: {} })[0]).toMatchObject({ summary: "0 fields" });
});

/** A nested value keeps its whole self, pretty-printed, for the reader. */
test("a nested value carries its json", () => {
  const out = projectPayload({ fields: { a: 1 } });
  expect(out[0]?.kind === "nested" && out[0].json).toBe('{\n  "a": 1\n}');
});

test("null and undefined render as an em dash, not as the word null", () => {
  expect(textOf(projectPayload({ assignee: null })[0])).toBe("—");
  expect(textOf(projectPayload({ assignee: undefined })[0])).toBe("—");
  expect(projectPayload({ assignee: null })[0]?.kind).toBe("scalar");
});

test("a payload that is not an object still projects", () => {
  expect(projectPayload("just a string")).toEqual([
    { key: "payload", label: "Payload", kind: "text", text: "just a string" },
  ]);
  expect(projectPayload(42)).toEqual([
    { key: "payload", label: "Payload", kind: "text", text: "42" },
  ]);
  expect(projectPayload(null)).toEqual([]);
  expect(projectPayload(undefined)).toEqual([]);
});

/** A top-level array is one value, summarised like any other nested one. */
test("a top-level array is summarised rather than spread into keys", () => {
  expect(projectPayload(["a", "b"])[0]).toMatchObject({
    key: "payload",
    kind: "nested",
    summary: "2 items",
  });
});

test("markup in a payload stays text", () => {
  // The defence is that PayloadView interpolates; this asserts the projector
  // does not "helpfully" pre-render anything.
  expect(textOf(projectPayload({ desc: "<script>alert(1)</script>" })[0])).toBe(
    "<script>alert(1)</script>",
  );
  // ...including inside a nested value's JSON, which a `<details>` shows raw.
  const nested = projectPayload({ c: [{ text: "<img onerror=alert(1)>" }] })[0];
  expect(nested?.kind === "nested" && nested.json).toContain(
    "<img onerror=alert(1)>",
  );
});

/**
 * A payload is a source system's JSON, which is allowed to contain things
 * `JSON.stringify` refuses — a cycle cannot arrive over the IPC, but a
 * `BigInt` or a hand-built object in a test can, and a projector that threw
 * would take the whole detail view down with it.
 */
test("a value that will not stringify still yields a row", () => {
  const hostile = {
    toJSON() {
      throw new Error("no");
    },
  };
  const out = projectPayload({ weird: hostile });
  expect(out).toHaveLength(1);
  expect(out[0]?.kind).toBe("nested");
  expect(out[0]?.kind === "nested" && out[0].json).toContain(
    "could not be rendered",
  );
});
