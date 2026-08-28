/**
 * The links panel, and the four things it exists to get right.
 *
 * A row has to say *what* was linked (a kind and a title, not an id), *which
 * way round* the link runs (the inverse-label wording), that a target the
 * source withdrew is still there, and it has to be removable in one action.
 * Every one of those is a sentence a reader acts on, so every one is asserted
 * against the literal words rather than against a class name.
 */
import { flushSync, mount, unmount } from "svelte";
import { expect, test, vi } from "vitest";

import type { LinkEntry } from "../ipc/entity";
import LinksPanel from "./LinksPanel.svelte";

const VIEWED = "mock:PAY-231";

function entry(over: {
  id?: string;
  relation?: string;
  from?: string;
  to?: string;
  kind?: string;
  title?: string;
  note?: string | null;
  deleted?: string | null;
}): LinkEntry {
  const to = over.to ?? "mock:PAY-228";
  const from = over.from ?? VIEWED;
  const other = from === VIEWED ? to : from;
  return {
    link: {
      id: over.id ?? "00000000-0000-0000-0000-000000000001",
      from_id: from,
      to_id: to,
      relation: over.relation ?? "related",
      origin: "manual",
      note: over.note ?? null,
      created_by: "user",
      created_at: "2026-08-28T09:30:00Z",
    },
    other: {
      entity_id: other,
      kind: over.kind ?? "ticket",
      title: over.title ?? "Retry storm postmortem",
      deleted_at: over.deleted ?? null,
    },
  };
}

function render(links: LinkEntry[]) {
  const target = document.createElement("div");
  document.body.append(target);
  const onopen = vi.fn();
  const onunlink = vi.fn();
  const app = mount(LinksPanel, {
    target,
    props: { entityId: VIEWED, links, onopen, onunlink },
  });
  flushSync();
  return {
    target,
    onopen,
    onunlink,
    /** The panel's text with runs of whitespace collapsed (see `Detail.test`). */
    text: () => (target.textContent ?? "").replace(/\s+/g, " "),
    buttons: (label: string) =>
      [...target.querySelectorAll<HTMLButtonElement>("button")].filter((button) =>
        button.textContent?.includes(label),
      ),
    headings: () => [...target.querySelectorAll(".row.hd span:nth-child(2)")].map((h) => h.textContent),
    done: () => {
      unmount(app);
      target.remove();
    },
  };
}

/** Story 9: an id is not something a person recognises. */
test("a row names the other end's kind and title, not its id", () => {
  const screen = render([entry({ title: "Retry storm postmortem", kind: "page" })]);

  expect(screen.text()).toContain("Retry storm postmortem");
  expect(screen.text()).toContain("Page");

  screen.done();
});

/**
 * Story 7. The same stored word, drawn from opposite ends, must not read the
 * same — which is the assertion an implementation that printed
 * `link.relation` would fail.
 */
test("rows read from the viewed entity's side, so one relation gives two headings", () => {
  const screen = render([
    entry({ id: "out", relation: "blocks", from: VIEWED, to: "mock:PAY-228" }),
    entry({ id: "in", relation: "blocks", from: "mock:PAY-400", to: VIEWED }),
  ]);

  expect(screen.headings()).toEqual(["blocks", "blocked by"]);

  screen.done();
});

/** A relation knobas has never seen reads as typed, from either end. */
test("a user-typed relation is not inverted into language nobody wrote", () => {
  const screen = render([entry({ relation: "supersedes", from: "mock:PAY-400", to: VIEWED })]);

  expect(screen.headings()).toEqual(["supersedes"]);

  screen.done();
});

/**
 * Story 8: the address, not an id and not an index. The full hash is asserted,
 * because the kind segment is what makes it the *other end's* address rather
 * than the alias.
 */
test("clicking a row navigates to the other end's stable address", () => {
  const screen = render([entry({ to: "mock:PAY-228", kind: "ticket" })]);

  screen.buttons("Retry storm postmortem")[0]!.click();
  flushSync();

  expect(screen.onopen).toHaveBeenCalledWith("#/ticket/mock:PAY-228");

  screen.done();
});

/** Story 10: kept and marked, rather than dropped or silently normal. */
test("a target the source withdrew is still listed, and says so", () => {
  const screen = render([
    entry({ title: "Legacy payout reconciliation", deleted: "2026-08-20T09:00:00Z" }),
  ]);

  expect(screen.text()).toContain("Legacy payout reconciliation");
  expect(screen.text()).toContain("withdrawn");
  expect(screen.buttons("Legacy payout reconciliation")).toHaveLength(1);

  screen.done();
});

/** ...and a live one is not marked, or the marker means nothing. */
test("a live target carries no withdrawn marker", () => {
  const screen = render([entry({ deleted: null })]);
  expect(screen.text()).not.toContain("withdrawn");
  screen.done();
});

/** Story 11: one action, and the row it names is the row that was pressed. */
test("unlink is one action with no confirmation, and names its own row", () => {
  const first = entry({ id: "first", title: "First" });
  const second = entry({ id: "second", title: "Second", to: "mock:PAY-300" });
  const screen = render([first, second]);

  const unlinks = screen.buttons("Unlink");
  expect(unlinks).toHaveLength(2);
  unlinks[1]!.click();
  flushSync();

  expect(screen.onunlink).toHaveBeenCalledTimes(1);
  expect(screen.onunlink.mock.calls[0]![0]).toBe(second);
  // Nothing else was asked of the reader on the way.
  expect(screen.target.querySelector('[role="dialog"]')).toBeNull();

  screen.done();
});

/** Story 5: the reason the link exists, where the reader is looking. */
test("a note is displayed, and a link without one draws no empty line", () => {
  const screen = render([
    entry({ id: "noted", note: "the retry storm postmortem" }),
    entry({ id: "bare", to: "mock:PAY-300", note: null }),
  ]);

  expect(screen.text()).toContain("the retry storm postmortem");
  expect(screen.target.querySelectorAll(".lnote")).toHaveLength(1);

  screen.done();
});

/**
 * Story 25. The M1 caveat said writing was impossible; it is not, and an empty
 * state that explains why you cannot act is worse than one that offers the
 * action.
 */
test("the empty state invites linking and carries no read-only caveat", () => {
  const screen = render([]);

  expect(screen.text()).toContain("Nothing linked yet");
  expect(screen.text()).toContain("Link this to");
  // The caveat is gone from the text *and* from the markup: it lived in a
  // `title` attribute, where `textContent` would never have seen it.
  expect(screen.text()).not.toContain("does not yet write");
  expect(screen.target.innerHTML).not.toContain("M2");

  screen.done();
});

/** A title a source system supplied is text, whatever is in it (gotcha 7). */
test("a title is text, whatever a source put in it", () => {
  const screen = render([entry({ title: "<em>Retry</em> storm" })]);

  expect(screen.text()).toContain("<em>Retry</em> storm");
  expect(screen.target.querySelector("em")).toBeNull();

  screen.done();
});
