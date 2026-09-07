/**
 * One source row's **actions**, and the one that would otherwise be
 * unreachable (#452).
 *
 * The rest of the row is covered from the view that mounts it
 * (`SourcesView.test.svelte.ts`), which is the house shape. This file is about
 * a *prop* — whether the source's adapter declares it can carry a second
 * credential — and driving that through the view would mean loading the
 * module-level kind registry to get one boolean into one component.
 *
 * **The direction that matters is a healthy source.** The credential strip is
 * offered to a source that is *failing*: `unauthorized`, `missing_secret`, or a
 * test the far end refused. A working key-only Uptime Kuma is none of those,
 * and it is exactly the source story 69's "adding the account" describes — so
 * without a door on a healthy row, the flip is a thing a reader could only do
 * by breaking their Kuma first.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { CredentialHealth, SourceSummary } from "../ipc/sources";
import SourceRow from "./SourceRow.svelte";

const NOW = new Date("2026-09-07T12:00:00Z");

function source(over: Partial<SourceSummary> = {}): SourceSummary {
  return {
    id: "kuma",
    adapter_kind: "kuma",
    display_name: "Uptime Kuma",
    base_url: "http://127.0.0.1:3001",
    enabled: true,
    sync_interval_secs: 60,
    config: {},
    health: {
      source_id: "kuma",
      state: "ok",
      checked_at: "2026-09-07T11:59:00Z",
      detail: null,
      secret_expires_at: null,
    } satisfies CredentialHealth,
    last_run: null,
    next_run_at: null,
    item_count: 8,
    auth_kind: "ApiToken",
    kinds: [
      { id: "monitor", label: "Monitor", plural: "Monitors", monogram: "MO", full_sync_exhaustive: true },
    ],
    ...over,
  };
}

let target: HTMLDivElement;
let app: Record<string, unknown> | undefined;
const reenters: number[] = [];

function render(over: Partial<SourceSummary>, acceptsAccount: boolean) {
  app = mount(SourceRow, {
    target,
    props: {
      source: source(over),
      health: null,
      status: null,
      now: NOW,
      acceptsAccount,
      onsync: () => {},
      ontest: () => {},
      onreenter: () => reenters.push(1),
      ondelete: () => {},
    },
  });
  flushSync();
}

/** Every action button's label, in the order the row draws them. */
function actions(): string[] {
  return [...target.querySelectorAll<HTMLButtonElement>(".acts button")].map((button) =>
    (button.textContent ?? "").trim(),
  );
}

function press(label: string) {
  [...target.querySelectorAll<HTMLButtonElement>(".acts button")]
    .find((button) => button.textContent?.trim() === label)!
    .click();
  flushSync();
}

beforeEach(() => {
  reenters.length = 0;
  target = document.createElement("div");
  document.body.append(target);
});

afterEach(() => {
  if (app) unmount(app);
  app = undefined;
  target.remove();
});

test("a healthy source whose adapter takes an account is offered one", () => {
  render({}, true);
  expect(actions()).toEqual(["Test", "Account", "Sync now", "Delete"]);
  // The same strip *Re-enter* opens — one door, two words for the two states
  // a reader can be in.
  press("Account");
  expect(reenters).toEqual([1]);
});

test("a healthy source whose adapter takes none grows no button for one", () => {
  render({}, false);
  expect(actions()).toEqual(["Test", "Sync now", "Delete"]);
});

/**
 * A failing source keeps *Re-enter* and does not also get *Account*: the two
 * are one door, and a row offering both would be asking a reader to choose
 * between two words for the same strip.
 */
test("a failing source is offered Re-enter and not both", () => {
  render(
    {
      health: {
        source_id: "kuma",
        state: "unauthorized",
        checked_at: "2026-09-07T11:59:00Z",
        detail: "401",
        secret_expires_at: null,
      },
    },
    true,
  );
  expect(actions()).toEqual(["Test", "Re-enter", "Delete"]);
});

/** *Sync now* is still the thing a disabled or running source does not get. */
test("a disabled source keeps Account and loses Sync now", () => {
  render({ enabled: false }, true);
  expect(actions()).toEqual(["Test", "Account", "Delete"]);
});
