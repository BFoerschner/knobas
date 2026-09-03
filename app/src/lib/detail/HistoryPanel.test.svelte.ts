/**
 * The history panel, and the one distinction it exists to draw: a line the
 * *reader* caused versus a line the *mirror* caused.
 *
 * Spec §12.1 wants a change history per item. A history that rendered
 * `sync:mock` and `user` identically would be a list of events with no
 * attribution, which is the part a person actually reads it for.
 */
import { flushSync, mount, unmount } from "svelte";
import { expect, test } from "vitest";

import type { ActivityRow } from "../ipc/entity";
import HistoryPanel from "./HistoryPanel.svelte";
import { parseActor } from "./actor";

function line(over: Partial<ActivityRow> & { id: number }): ActivityRow {
  return {
    at: "2026-08-22T14:30:00Z",
    actor: "user",
    verb: "opened",
    entity_id: "mock:PAY-231",
    detail: {},
    ...over,
  };
}

function render(activity: ActivityRow[]) {
  const target = document.createElement("div");
  document.body.append(target);
  const app = mount(HistoryPanel, { target, props: { activity } });
  flushSync();
  return {
    target,
    text: () => target.textContent ?? "",
    monograms: () =>
      [...target.querySelectorAll(".mg")].map((m) => m.textContent),
    done: () => {
      unmount(app);
      target.remove();
    },
  };
}

test("a synced line and a user line are told apart", () => {
  const screen = render([
    line({
      id: 2,
      actor: "sync:mock",
      verb: "synced",
      detail: { upserted: 9 },
    }),
    line({ id: 1, actor: "user", verb: "opened" }),
  ]);

  expect(screen.monograms()).toEqual(["MO", "ME"]);
  expect(screen.text()).toContain("synced by mock");
  expect(screen.text()).toContain("you");

  screen.done();
});

/** The verb is a stored string and is rendered as text. */
test("a verb is text, whatever a writer put in it", () => {
  const screen = render([line({ id: 1, verb: "<em>linked</em>" })]);

  expect(screen.text()).toContain("<em>linked</em>");
  expect(screen.target.querySelector("em")).toBeNull();

  screen.done();
});

/**
 * The `detail` object is opened on demand, and only when there is something
 * in it — `activity::record` stores a JSON null as `{}`, so most lines carry
 * nothing and a disclosure on every one of them would be noise.
 */
test("a line with a detail object gets a disclosure; an empty one does not", () => {
  const screen = render([
    line({ id: 2, verb: "synced", detail: { upserted: 9 } }),
    line({ id: 1, verb: "opened", detail: {} }),
  ]);

  expect(screen.target.querySelectorAll("details")).toHaveLength(1);
  expect(screen.text()).toContain("Upserted");
  expect(screen.text()).toContain("9");

  screen.done();
});

test("an item with no history says so rather than rendering nothing", () => {
  const screen = render([]);
  expect(screen.text()).toContain("No recorded activity for this item yet");
  expect(screen.target.querySelectorAll(".row")).toHaveLength(0);
  screen.done();
});

/** Every actor spelling the column can hold renders as something. */
test("an actor knobas does not recognise still renders", () => {
  expect(parseActor("user")).toMatchObject({ kind: "user", monogram: "ME" });
  expect(parseActor("sync:gitea")).toMatchObject({
    kind: "sync",
    monogram: "GI",
    label: "synced by gitea",
  });
  expect(parseActor("scheduler")).toMatchObject({
    kind: "other",
    monogram: "SC",
    label: "scheduler",
  });
  expect(parseActor("")).toMatchObject({ kind: "other", monogram: "??" });
  expect(parseActor("sync:")).toMatchObject({ kind: "sync", monogram: "??" });
});
