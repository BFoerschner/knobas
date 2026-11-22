/**
 * Passwords in the launcher (ADR-0056, the decisions' section 26): a password
 * entry's preview draws its title, folder, username and address and nothing
 * else of it; its sheet copies the password, the username or the code through
 * the Passwords page's own commands, and *Copy password* on an entry that asks
 * a usage comment opens the entry instead, reading nothing.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { LauncherHome, SearchHit, SearchResponse } from "../ipc";
import type { PwEntry } from "../ipc/passwords";
import { reset as resetToasts, toasts } from "../shell/toasts.svelte";
import Launcher from "./Launcher.svelte";
import { PREVIEW_DEBOUNCE_MS } from "./Preview.svelte";
import { createScope } from "./scope.svelte";
import { DEBOUNCE_MS } from "./session.svelte";

let target: HTMLElement;
let app: ReturnType<typeof mount> | undefined;

function hit(entity_id: string, kind: string, title: string, path: string | null): SearchHit {
  return { entity_id, kind, source_id: "pw", updated_at: null, synced_at: "2026-11-22T10:22:00Z", path, state: null, title, rank: 1, snippet: [], excerpts: [] };
}

function response(raw: string): SearchResponse {
  return {
    interpreted: { text: raw, prefix: null, filters: { sources: [], kinds: [], updated_within_days: null, mine: false, authors: [] }, unknown_tokens: [], left_out_tokens: [] },
    groups: [{ kind: "password_entry", label: "Password entry", plural: "Password entries", monogram: "PW", total: 1, hits: [hit("pw:e1", "password_entry", "payout db", "Payments / Prod")] }],
    total: 1,
    took_ms: 2,
    coverage: [],
  };
}

function record(over: Partial<PwEntry> = {}): PwEntry {
  return {
    id: "pw:e1", source: "pw", title: "payout db", username: "payout_svc", address: "https://pay.example", folder: "pw:prod",
    notes: "Rotated quarterly by Priya.", fields: [{ name: "Port", value: "5432" }], otp: { format: "keepass", digits: 6, period: 30, algorithm: "SHA1" },
    expires: null, changed: null, changed_by: null, comment: { read: false, modify: false }, approval: { state: "none" }, attachments: { state: "none" },
    can_read: true, can_modify: false, ...over,
  };
}

const HOME = { recent: [], favourites: [], sources: [], pending_writes: 0, notes_shown: true } as unknown as LauncherHome;

function mountIt(entry: PwEntry = record()) {
  const kept = new Map<string, string>([["knobas.launcher.scope", "everything"]]);
  const calls = {
    navigate: vi.fn(),
    entry: vi.fn(async () => entry),
    copy: vi.fn(async () => ({ kind: "copied" as const, read_at: "2026-11-22T10:23:00Z", until: "2026-11-22T10:23:30Z" })),
    copyCode: vi.fn(async () => ({ kind: "code" as const, code: "482913", digits: 6, period: 30, left_ms: 12_000 })),
    copyPlain: vi.fn(async () => {}),
  };
  app = mount(Launcher, {
    target,
    props: {
      open: true,
      places: [],
      onnavigate: calls.navigate,
      onclose: () => {},
      ontimer: vi.fn(),
      openEntity: { entityId: "jira:PAY-999", label: "PAY-999" },
      scope: createScope(() => ({ getItem: (k: string) => kept.get(k) ?? null, setItem: (k: string, v: string) => void kept.set(k, v), removeItem: (k: string) => void kept.delete(k) })),
      ports: { search: async (query) => response(query.raw), launcherHome: async () => HOME },
      sheetPorts: {
        checkoutSettings: async () => ({ applications: [], default_application: null }) as never,
        linkSets: async () => [],
        passwords: { entry: calls.entry, copy: calls.copy, copyCode: calls.copyCode, copyPlain: calls.copyPlain },
      },
    },
  });
  flushSync();
  return calls;
}

async function settle() {
  for (let i = 0; i < 6; i += 1) await Promise.resolve();
  flushSync();
}

const input = () => target.querySelector("input")!;

async function type(text: string) {
  input().value = text;
  input().dispatchEvent(new Event("input", { bubbles: true }));
  vi.advanceTimersByTime(DEBOUNCE_MS);
  await settle();
}

function press(key: string) {
  input().dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true }));
  flushSync();
}

const pane = () => target.querySelector<HTMLElement>('[aria-label="Where it matches"]');
const labels = () => [...(target.querySelector('[aria-label="Actions on the selected result"]')?.querySelectorAll(".tf-ln-nm") ?? [])].map((el) => el.textContent?.trim());
const lastToast = () => toasts.items.at(-1)?.text;

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
  target = document.createElement("div");
  document.body.append(target);
});

afterEach(() => {
  if (app) unmount(app);
  app = undefined;
  vi.useRealTimers();
  target.remove();
  resetToasts();
});

test("a password entry's preview draws its title, folder, username and address, and nothing else of it", async () => {
  const calls = mountIt();
  await settle();
  await type("rotated");
  vi.advanceTimersByTime(PREVIEW_DEBOUNCE_MS);
  await settle();
  expect(calls.entry).toHaveBeenCalledWith("pw:e1");
  const text = [...pane()!.querySelectorAll("p, dt, dd")].map((el) => el.textContent!.trim()).join(" ");
  expect(text).toContain("payout db");
  expect(text).toContain("Folder Payments / Prod");
  expect(text).toContain("Username payout_svc");
  expect(text).toContain("Address https://pay.example");
  expect(text).not.toContain("Rotated");
  expect(text).not.toContain("5432");
  expect(text).not.toContain("No passage to quote");
});

test("the sheet copies the password through the page's command, and offers no link or timer", async () => {
  const calls = mountIt();
  await settle();
  await type("payout");
  press("Tab");
  await settle();
  expect(labels()).toEqual(["Copy password", "Copy username", "Copy one-time code", "Open", "Favourite", "Copy title"]);
  press("Enter");
  await settle();
  expect(calls.copy).toHaveBeenCalledWith("pw:e1", { kind: "password" }, null);
  expect(lastToast()).toBe("Copied the password of payout db. It clears in 30 seconds.");
  expect(calls.navigate).not.toHaveBeenCalled();
});

test("Copy password on an entry that asks a usage comment opens the entry and reads nothing", async () => {
  const calls = mountIt(record({ comment: { read: true, modify: false } }));
  await settle();
  await type("payout");
  press("Tab");
  await settle();
  press("Enter");
  await settle();
  expect(calls.copy).not.toHaveBeenCalled();
  expect(calls.navigate).toHaveBeenCalledWith("#/passwords/entry/pw%3Ae1");
});

test("Copy username and Copy one-time code copy what the mirror and the keychain hold", async () => {
  const calls = mountIt();
  await settle();
  await type("payout");
  press("Tab");
  await settle();
  press("ArrowDown");
  press("Enter");
  await settle();
  expect(calls.copyPlain).toHaveBeenCalledWith("payout_svc");
  expect(lastToast()).toBe("Copied the username of payout db");
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "k", metaKey: true, bubbles: true, cancelable: true }));
  await settle();
  await type("payout");
  press("Tab");
  await settle();
  press("ArrowDown");
  press("ArrowDown");
  press("Enter");
  await settle();
  expect(calls.copyCode).toHaveBeenCalledWith("pw:e1");
  expect(calls.copy).not.toHaveBeenCalled();
});
