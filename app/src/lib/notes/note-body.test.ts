import { expect, test } from "vitest";
import type { LinkEnd, NoteRef } from "../ipc/entity";
import { activeRef, carryCaret, insertRef, replaceSpan, tokenise } from "./note-body";

function end(over: Partial<LinkEnd> = {}): LinkEnd {
  return {
    entity_id: "mock:PAY-231",
    kind: "ticket",
    title: "Payments retry storm",
    deleted_at: null,
    ...over,
  };
}

function ref(target_id: string, target: LinkEnd | null): NoteRef {
  return { target_id, target };
}

test("a reference the backend named becomes a chip and the text around it stays text", () => {
  const tokens = tokenise("off-by-one in [[mock:PAY-231]], see above", [
    ref("mock:PAY-231", end()),
  ]);
  expect(tokens).toEqual([
    { kind: "text", text: "off-by-one in " },
    {
      kind: "ref",
      text: "[[mock:PAY-231]]",
      ref: ref("mock:PAY-231", end()),
    },
    { kind: "text", text: ", see above" },
  ]);
});

/**
 * Story 10: a ref that resolves to nothing is still a ref — visible, marked,
 * and not silently plain text. The backend lists it; only `target` is null.
 */
test("an unresolved reference is a chip too, carrying no target", () => {
  const tokens = tokenise("see [[mock:NOPE-1]]", [ref("mock:NOPE-1", null)]);
  expect(tokens).toEqual([
    { kind: "text", text: "see " },
    { kind: "ref", text: "[[mock:NOPE-1]]", ref: ref("mock:NOPE-1", null) },
  ]);
});

/**
 * The whole reason this module does not decide for itself.
 *
 * Every one of these is a span the Rust scanner refuses — an unclosed bracket,
 * a line break inside, an empty pair — so none of them is in `refs`, so none of
 * them is drawn as a chip. A second scanner here with its own opinion is how
 * the body and the links panel start disagreeing.
 */
test("brackets the backend did not name are drawn as the text they are", () => {
  for (const body of [
    "an unclosed [[mock:PAY-231",
    "[[mock:PAY-231\nand more]]",
    "[[]] and [[   ]]",
    "[a link](https://example.invalid)",
    "[[mock:PAY-999]]",
  ]) {
    expect(tokenise(body, []), body).toEqual([{ kind: "text", text: body }]);
  }
  // ...including beside one that *is* named, so the scan does not give up.
  expect(tokenise("[[mock:PAY-231]] but not [[mock:PAY-999]]", [ref("mock:PAY-231", end())])).toEqual([
    { kind: "ref", text: "[[mock:PAY-231]]", ref: ref("mock:PAY-231", end()) },
    { kind: "text", text: " but not [[mock:PAY-999]]" },
  ]);
});

test("an empty body has nothing to draw", () => {
  expect(tokenise("", [])).toEqual([]);
});

test("the same reference twice is a chip twice", () => {
  const tokens = tokenise("[[mock:PAY-231]] and [[mock:PAY-231]]", [ref("mock:PAY-231", end())]);
  expect(tokens.filter((token) => token.kind === "ref")).toHaveLength(2);
});

/** A reference written with spaces inside is the same reference. */
test("a reference is matched after trimming, the way the backend matches it", () => {
  const tokens = tokenise("[[  mock:PAY-231  ]]", [ref("mock:PAY-231", end())]);
  expect(tokens).toEqual([
    { kind: "ref", text: "[[  mock:PAY-231  ]]", ref: ref("mock:PAY-231", end()) },
  ]);
});

test("typing after [[ is a completion in progress", () => {
  const body = "off-by-one in [[pay";
  expect(activeRef(body, body.length)).toEqual({ start: 14, end: 19, query: "pay" });
  // `[[` alone is a completion too — that is the moment the picker opens.
  expect(activeRef("see [[", 6)).toEqual({ start: 4, end: 6, query: "" });
});

/**
 * Each of these is a caret that is *not* completing anything, and an
 * autocomplete that thought otherwise would follow the writer down the page.
 */
test("a caret outside an open [[ is not completing a reference", () => {
  expect(activeRef("no brackets here", 5)).toBeNull();
  // Already closed.
  expect(activeRef("[[mock:PAY-231]] and more", 25)).toBeNull();
  // Abandoned, and a line break says so.
  expect(activeRef("[[pay\nnext line", 15)).toBeNull();
  // Before the brackets exist.
  expect(activeRef("see [[pay", 3)).toBeNull();
});

test("accepting a completion writes the whole reference and puts the caret after it", () => {
  const body = "off-by-one in [[pay";
  const active = activeRef(body, body.length)!;
  const written = insertRef(body, active, "mock:PAY-231");
  expect(written.body).toBe("off-by-one in [[mock:PAY-231]]");
  expect(written.caret).toBe(written.body.length);
});

/**
 * The editor helpfully closes brackets in some setups, and a pasted `]]` is
 * ordinary. Leaving it would write `[[mock:PAY-231]]]]`, which is a reference
 * followed by two brackets of litter.
 */
test("a closing pair already after the caret is consumed rather than doubled", () => {
  const body = "see [[pay]] then";
  const active = activeRef(body, 9)!;
  const written = insertRef(body, active, "mock:PAY-231");
  expect(written.body).toBe("see [[mock:PAY-231]] then");
  expect(written.body.slice(written.caret)).toBe(" then");
});

/**
 * A paste and the swap that follows it are both a span of the body being
 * replaced, and the caret always lands just past what was written — the same
 * rule `insertRef` follows, which is why they share this.
 */
test("replacing a span writes over it and leaves the caret past what was written", () => {
  const body = "see  then";
  const laid = replaceSpan(body, 4, 4, "https://jira.example/browse/PAY-231");
  expect(laid.body).toBe("see https://jira.example/browse/PAY-231 then");
  expect(laid.body.slice(laid.caret)).toBe(" then");

  // A selection the paste lands on top of is what the span covers.
  const over = replaceSpan("see the URL then", 4, 11, "[[mock:PAY-231]]");
  expect(over.body).toBe("see [[mock:PAY-231]] then");
  expect(over.body.slice(over.caret)).toBe(" then");
});

/**
 * The swap a pasted URL becomes lands a round trip late, so the caret it finds
 * is the reader's rather than its own — {@link carryCaret} is what keeps it
 * there.
 */
test("a caret carried across a replaced span stays where the reader put it", () => {
  const url = "https://jira.example/browse/PAY-231";
  const ref = "[[mock:PAY-231]]".length;

  // Just past the pasted URL, which is where a reader who pasted and stopped
  // is: it follows the reference, exactly as `replaceSpan` would have said.
  expect(carryCaret(4 + url.length, 4, 4 + url.length, ref)).toBe(4 + ref);
  // Writing on: the caret keeps its distance from the end of the reference.
  expect(carryCaret(4 + url.length + 18, 4, 4 + url.length, ref)).toBe(4 + ref + 18);
  // Before the paste: untouched, because nothing in front of it moved.
  expect(carryCaret(2, 4, 4 + url.length, ref)).toBe(2);
  expect(carryCaret(4, 4, 4 + url.length, ref)).toBe(4);
  // Inside the URL, which the reference replaced: past what was written, since
  // there is no position inside it left to keep.
  expect(carryCaret(20, 4, 4 + url.length, ref)).toBe(4 + ref);
});
