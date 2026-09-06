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
import { noFilters, type ResultGroup, type SearchHit, type SearchQuery, type SearchResponse } from "../ipc/search";
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
    path: null,
  };
}

/** Two legal targets with a stored context between them. */
const RECENT: EntityRow[] = [
  row("jira:PAY-231", "ticket", "Payments retry storm"),
  row("ctx:5b1c0f1e", "ctx", "SEPA migration"),
  row("note:9f21", "note", "Standup 2026-09-03"),
];

/** One asset hit, as the estate's browse answers it (#437). */
function asset(entityId: string, title: string, path: string | null): SearchHit {
  return {
    entity_id: entityId,
    kind: "asset",
    source_id: "asset",
    updated_at: "2026-09-06T09:00:00Z",
    synced_at: "2026-09-06T09:00:00Z",
    title,
    path,
    rank: 0,
    snippet: [],
  };
}

/** A search answer holding one asset group, or none at all. */
function estateOf(hits: SearchHit[]): SearchResponse {
  const groups: ResultGroup[] =
    hits.length === 0
      ? []
      : [
          {
            kind: "asset",
            label: "Asset",
            plural: "Assets",
            monogram: "AS",
            total: hits.length,
            hits,
          },
        ];
  return {
    interpreted: { text: "", prefix: null, filters: noFilters(), unknown_tokens: [] },
    groups,
    total: hits.length,
    took_ms: 1,
    coverage: [],
  };
}

/** An estate with nothing in it — a fresh install, and every test's default. */
const NO_ESTATE = estateOf([]);

let target: HTMLDivElement;
let app: Record<string, unknown> | undefined;

function render(recent: EntityRow[] = RECENT, estate: SearchResponse = NO_ESTATE) {
  const picked: TimerTarget[] = [];
  const asked: SearchQuery[] = [];
  const onclose = vi.fn();
  app = mount(TimerPicker, {
    target,
    props: {
      onpick: (chosen: TimerTarget) => picked.push(chosen),
      onclose,
      recent: () => Promise.resolve(recent),
      estate: (query: SearchQuery) => {
        asked.push(query);
        return Promise.resolve(estate);
      },
    },
  });
  flushSync();
  return { picked, onclose, asked };
}

/** Every button in the recents list, by the text it shows. */
function offered(): string[] {
  return [...target.querySelectorAll(".recent:not(.estate) .row")].map((button) =>
    (button.textContent ?? "").trim().replace(/\s+/g, " "),
  );
}

/** Every button in the estate's list, by the text it shows (#437). */
function estateOffered(): string[] {
  return [...target.querySelectorAll(".estate .row")].map((button) =>
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
      estate: () => Promise.resolve(NO_ESTATE),
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

/**
 * **The estate is offered, under its own heading** (#437, story 46).
 *
 * A second list rather than more of the recents: an asset is knobas' own and
 * is in no mirror, so it can never arrive through `launcher_home` however
 * recently it was touched. Each row is the asset's **name** with the path it
 * sits under — two containers both called `postgres` are told apart by nothing
 * else — and never by the `asset:<uuid>` id the recents' rows show, which
 * names nothing a reader knows.
 */
test("the estate's assets are offered beside the recents, by name and path", async () => {
  const { asked } = render(RECENT, estateOf([
    asset("asset:9f3c", "vm-db-01", "hel1"),
    asset("asset:1a2b", "postgres", "hel1 / vm-db-01"),
  ]));
  await vi.waitFor(() => expect(estateOffered()).toHaveLength(2));

  // What was *asked for*, not only what was drawn: nothing typed and the one
  // dimension narrowed is what makes this the estate's browse rather than the
  // launcher board, and no fake can say it for the picker.
  expect(asked).toHaveLength(1);
  expect(asked[0]!.raw).toBe("");
  expect(asked[0]!.filters.kinds).toEqual(["asset"]);
  expect(asked[0]!.limit).toBe(20);

  expect(estateOffered()).toEqual(["vm-db-01 hel1", "postgres hel1 / vm-db-01"]);
  expect(target.textContent).toContain("…or an asset");
  expect(target.textContent, "the picker showed a reader a uuid").not.toContain("9f3c");
  // The recents are still there: the estate is a list beside them, not
  // instead of them.
  expect(offered()).toHaveLength(2);
});

/** Choosing one starts the clock on that asset's entity id. */
test("choosing an asset answers with it as an entity target", async () => {
  const { picked } = render(RECENT, estateOf([asset("asset:9f3c", "vm-db-01", "hel1")]));
  await vi.waitFor(() => expect(estateOffered()).toHaveLength(1));

  target.querySelector<HTMLButtonElement>(".estate .row")!.click();
  flushSync();
  expect(picked).toEqual([{ kind: "entity", entity_id: "asset:9f3c" }]);
});

/**
 * An estate nobody has filled in yet draws **no heading**, not an empty one.
 *
 * A fresh install has no assets at all, and a *…or an asset* over nothing
 * would be the picker promising a list it does not have — the rule the recents
 * heading already follows.
 */
test("an empty estate is no heading rather than an empty list", async () => {
  render();
  await vi.waitFor(() => expect(offered()).toHaveLength(2));
  expect(target.textContent).not.toContain("…or an asset");
});

/**
 * An estate that cannot be read is silent, and costs the recents nothing.
 *
 * Two commands, two effects: `search` rejecting `not_ready` must not take the
 * recents list down with it, and it says nothing on screen because an estate
 * with nothing in it is the ordinary state of a fresh install — a fault
 * reported where there is none is worse than the missing list.
 */
test("an estate that cannot be read leaves the recents alone and says nothing", async () => {
  const picked: TimerTarget[] = [];
  app = mount(TimerPicker, {
    target,
    props: {
      onpick: (chosen: TimerTarget) => picked.push(chosen),
      onclose: () => {},
      recent: () => Promise.resolve(RECENT),
      estate: () => Promise.reject(new Error("not_ready")),
    },
  });
  flushSync();
  await vi.waitFor(() => expect(offered()).toHaveLength(2));

  expect(estateOffered()).toEqual([]);
  expect(target.textContent).not.toContain("…or an asset");
  expect(target.textContent).not.toContain("could not be read");
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
