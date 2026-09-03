/**
 * ⌘T's picker: a free-text label is a target, and **a stored context is
 * refused** (issue #278, stories 9 and 15).
 *
 * The refusal is the load-bearing half, and it is asserted as a refusal rather
 * than as an absence: the fixture below puts a real context row — kind `ctx`,
 * id `ctx:<uuid>`, which is exactly what `knobas_core::context` writes into
 * `knobas.entity` — **into the recents the picker reads**, beside two rows
 * that must survive. A picker that offered everything fails; one that offered
 * nothing fails too.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { EntityRow } from "../ipc/entity";
import type { TimerTarget } from "../ipc/time";
import TimerPicker from "./TimerPicker.svelte";

function row(entity_id: string, kind: string, title: string): EntityRow {
  return {
    entity_id,
    kind,
    source_id: entity_id.slice(0, entity_id.indexOf(":")),
    title,
    updated_at: "2026-09-03T09:00:00Z",
    synced_at: "2026-09-03T09:01:00Z",
  };
}

/** Two legal targets with a stored context between them. */
const RECENT: EntityRow[] = [
  row("jira:PAY-231", "ticket", "Payments retry storm"),
  row("ctx:5b1c0f1e", "ctx", "SEPA migration"),
  row("note:9f21", "note", "Standup 2026-09-03"),
];

let target: HTMLDivElement;
let app: Record<string, unknown> | undefined;

function render(recent: EntityRow[] = RECENT) {
  const picked: TimerTarget[] = [];
  const onclose = vi.fn();
  app = mount(TimerPicker, {
    target,
    props: {
      onpick: (chosen: TimerTarget) => picked.push(chosen),
      onclose,
      recent: () => Promise.resolve(recent),
    },
  });
  flushSync();
  return { picked, onclose };
}

/** Every button in the recents list, by the text it shows. */
function offered(): string[] {
  return [...target.querySelectorAll(".recent .row")].map((button) =>
    (button.textContent ?? "").trim().replace(/\s+/g, " "),
  );
}

function labelField(): HTMLInputElement {
  const field = target.querySelector<HTMLInputElement>("input[type=text]");
  expect(field, "the picker has no label field").not.toBeNull();
  return field!;
}

function type(text: string) {
  const field = labelField();
  field.value = text;
  field.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
}

beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
});

afterEach(() => {
  if (app) unmount(app);
  app = undefined;
  target.remove();
});

test("a free-text label is a legal target and is what the picker answers with", async () => {
  const { picked } = render();
  await vi.waitFor(() => expect(offered()).toHaveLength(2));

  type("  DB config for the migration  ");
  target.querySelector<HTMLButtonElement>("button[type=submit]")!.click();
  flushSync();

  // Trimmed here as well as in the backend: what the reader sees quoted back
  // in the strip must be what they meant, not what their trailing space said.
  expect(picked).toEqual([{ kind: "label", label: "DB config for the migration" }]);
});

test("a blank label cannot be started", () => {
  render();
  const start = target.querySelector<HTMLButtonElement>("button[type=submit]")!;
  expect(start.disabled).toBe(true);

  type("   ");
  expect(start.disabled, "whitespace is not a name for what the time was on").toBe(true);

  type("something");
  expect(start.disabled).toBe(false);
});

/**
 * **The refusal.** The context was in the list the picker read and is not in
 * the list it drew; the two rows around it are.
 */
test("a stored context in the recents is not offered as a target", async () => {
  render();
  await vi.waitFor(() => expect(offered()).toHaveLength(2));

  const rows = offered();
  expect(rows.some((text) => text.includes("Payments retry storm"))).toBe(true);
  expect(rows.some((text) => text.includes("Standup 2026-09-03"))).toBe(true);
  expect(
    rows.some((text) => text.includes("SEPA migration") || text.includes("ctx:")),
    "a stored context was offered as a timer target",
  ).toBe(false);
});

/**
 * ...and the same fixture the other way round, so the assertion above cannot
 * be satisfied by a picker that draws nothing: choosing a legal row answers
 * with that row's entity.
 */
test("a legal recent row is answered with as an entity target", async () => {
  const { picked } = render();
  await vi.waitFor(() => expect(offered()).toHaveLength(2));

  target.querySelectorAll<HTMLButtonElement>(".recent .row")[0]!.click();
  flushSync();
  expect(picked).toEqual([{ kind: "entity", entity_id: "jira:PAY-231" }]);
});

/**
 * Recents that cannot be read are said, not swallowed: the label field still
 * works, and a reader who expected their list deserves to know why it is not
 * there rather than concluding knobas has forgotten them.
 */
test("recents that cannot be read still leave a working label field", async () => {
  const { picked } = render();
  unmount(app!);
  app = mount(TimerPicker, {
    target,
    props: {
      onpick: (chosen: TimerTarget) => picked.push(chosen),
      onclose: () => {},
      recent: () => Promise.reject(new Error("not_ready")),
    },
  });
  flushSync();
  await vi.waitFor(() =>
    expect(target.textContent).toContain("Recent items could not be read"),
  );

  type("DB config");
  target.querySelector<HTMLButtonElement>("button[type=submit]")!.click();
  flushSync();
  expect(picked).toEqual([{ kind: "label", label: "DB config" }]);
});

/** Esc closes it — `Modal`'s rung, reached through the picker. */
test("Escape closes the picker", () => {
  const { onclose } = render();
  target
    .querySelector<HTMLElement>('[role="dialog"]')!
    .dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  flushSync();
  expect(onclose).toHaveBeenCalled();
});
