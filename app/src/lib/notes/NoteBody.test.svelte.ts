import { flushSync, mount, unmount } from "svelte";
import { expect, test, vi } from "vitest";

import type { LinkEnd, NoteRef } from "../ipc/entity";
import NoteBody from "./NoteBody.svelte";

function end(over: Partial<LinkEnd> = {}): LinkEnd {
  return {
    entity_id: "mock:PAY-231",
    kind: "ticket",
    title: "Payments retry storm",
    deleted_at: null,
    ...over,
  };
}

function render(body: string, refs: NoteRef[]) {
  const target = document.createElement("div");
  document.body.append(target);
  const onopen = vi.fn();
  const app = mount(NoteBody, { target, props: { body, refs, onopen } });
  flushSync();
  return {
    target,
    onopen,
    /** The panel's text with runs of whitespace collapsed. */
    text: () => (target.textContent ?? "").replace(/\s+/g, " "),
    chips: () => [...target.querySelectorAll<HTMLElement>(".chip")],
    done: () => {
      unmount(app);
      target.remove();
    },
  };
}

/** Story 7: a reference reads as the thing it points at, not as a key. */
test("a resolved reference is drawn as its target's kind and title", () => {
  const screen = render("off-by-one in [[mock:PAY-231]]", [
    { target_id: "mock:PAY-231", target: end() },
  ]);
  expect(screen.text()).toContain("off-by-one in");
  expect(screen.text()).toContain("Payments retry storm");
  expect(screen.text()).not.toContain("[[mock:PAY-231]]");
  screen.done();
});

/** Story 8: references are navigation. */
test("clicking a reference opens its target", () => {
  const screen = render("see [[mock:PAY-231]]", [{ target_id: "mock:PAY-231", target: end() }]);
  screen.chips()[0]!.click();
  expect(screen.onopen).toHaveBeenCalledWith("#/ticket/mock:PAY-231");
  screen.done();
});

/** Story 9: kept and marked, rather than dropped or silently normal. */
test("a reference whose target was withdrawn is still there, and says so", () => {
  const screen = render("was [[mock:PAY-198]]", [
    {
      target_id: "mock:PAY-198",
      target: end({
        entity_id: "mock:PAY-198",
        title: "Legacy payout reconciliation",
        deleted_at: "2026-08-28T09:30:00Z",
      }),
    },
  ]);
  expect(screen.text()).toContain("Legacy payout reconciliation");
  expect(screen.text()).toContain("withdrawn");
  expect(screen.chips()).toHaveLength(1);
  screen.done();
});

/** ...and a live target carries no marker, or the marker means nothing. */
test("a live target is not marked withdrawn", () => {
  const screen = render("see [[mock:PAY-231]]", [{ target_id: "mock:PAY-231", target: end() }]);
  expect(screen.text()).not.toContain("withdrawn");
  screen.done();
});

/**
 * Story 10: a typo is discoverable. It is shown as a reference that found
 * nothing — not hidden, and not passed off as ordinary text.
 */
test("an unresolved reference names what it could not find and says it is unresolved", () => {
  const screen = render("see [[mock:NOPE-1]]", [{ target_id: "mock:NOPE-1", target: null }]);
  expect(screen.text()).toContain("mock:NOPE-1");
  expect(screen.text()).toContain("unresolved");
  // Not navigation: there is nowhere to go.
  expect(screen.target.querySelectorAll("button")).toHaveLength(0);
  screen.done();
});

/**
 * Story 16's rule, one layer up from the search snippet: the body is a
 * document the user typed, and it is drawn as text. There is no `{@html}` in
 * this app (`shell/house-rules.test.ts`); this is what that means where it
 * matters most.
 */
test("markdown and markup in the body are text, whatever is in them", () => {
  const screen = render("## Runbook\n<script>alert(1)</script> and <em>emphasis</em>", []);
  expect(screen.text()).toContain("## Runbook");
  expect(screen.text()).toContain("<em>emphasis</em>");
  expect(screen.target.querySelector("em")).toBeNull();
  expect(screen.target.querySelector("h2")).toBeNull();
  screen.done();
});

/** A title a source system supplied is text too (gotcha 7). */
test("a target's title is text, whatever a source put in it", () => {
  const screen = render("[[mock:PAY-231]]", [
    { target_id: "mock:PAY-231", target: end({ title: "<em>Retry</em> storm" }) },
  ]);
  expect(screen.text()).toContain("<em>Retry</em> storm");
  expect(screen.target.querySelector("em")).toBeNull();
  screen.done();
});

/**
 * The backend is what decides a reference, and this is the visible half of
 * that: brackets its `refs` does not name are the text they are, so the body
 * and the links panel can never say different things.
 */
test("brackets the backend did not name are drawn as text", () => {
  const screen = render("an unclosed [[mock:PAY-231 and [[mock:PAY-999]]", []);
  expect(screen.text()).toContain("[[mock:PAY-231");
  expect(screen.text()).toContain("[[mock:PAY-999]]");
  expect(screen.chips()).toHaveLength(0);
  screen.done();
});
