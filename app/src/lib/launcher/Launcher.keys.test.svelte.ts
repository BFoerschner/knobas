/**
 * Keys (#2429, spec #2419 stories 43 to 48): ⌘B on a selected action, or
 * *Assign key…* on a saved list, opens the panel that shows the keys as they
 * are pressed; Enter or OK keeps them, Backspace clears, Esc and Cancel leave
 * things as they were, and the box's own chords are refused. A bound action
 * chord runs the action on the selected row of its kind, opening the sheet
 * first for a flow or the link; a bound list chord applies the list.
 *
 * Driven through the keyboard with fake ports and a key store over a fake
 * backend that keeps what it is told; the assertions are what is drawn, what
 * was written and what ran.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { ActionKey, LauncherHome, ListKey, SearchHit, SearchQuery, SearchResponse } from "../ipc";
import type { CheckoutSettings, ViewAction } from "../ipc/entity";
import { reset as resetToasts, toasts } from "../shell/toasts.svelte";
import Launcher from "./Launcher.svelte";
import { createKeys } from "./keys.svelte";
import { DEBOUNCE_MS } from "./session.svelte";

let target: HTMLElement;
let app: ReturnType<typeof mount> | undefined;

const HITS: Record<string, Partial<SearchHit> & { kind: string; entity_id: string; title: string }> = {
  ticket: { kind: "ticket", entity_id: "jira:PAY-231", title: "Retry failed SEPA payouts" },
  pr: { kind: "pr", entity_id: "gitea:acme/svc#142", title: "Backoff", source_id: "gitea", path: "acme/svc" },
  repo: { kind: "repo", entity_id: "gitea:acme/svc", title: "acme/svc", source_id: "gitea" },
  list: { kind: "list", entity_id: "sepa-payouts", title: "sepa payouts", source_id: "list", path: "#t sepa" },
};

function response(raw: string): SearchResponse {
  const one = HITS[raw.trim()];
  const hits = one ? [{ source_id: "jira", updated_at: null, synced_at: "2026-08-25T11:56:00Z", path: null, state: null, rank: 1, snippet: [], ...one } as SearchHit] : [];
  return {
    interpreted: { text: raw.trim(), prefix: null, filters: { sources: [], kinds: [], updated_within_days: null, mine: false, authors: [] }, unknown_tokens: [] },
    groups: one ? [{ kind: one.kind, label: one.kind, plural: `${one.kind}s`, monogram: "XX", total: 1, hits }] : [],
    total: hits.length,
    took_ms: 1,
    coverage: [],
  };
}

const HOME = { recent: [], favourites: [], sources: [], pending_writes: 0, notes_shown: true } as unknown as LauncherHome;

const APPS = {
  applications: [
    { id: "idea", name: "IntelliJ", command: "idea {path}", file_command: null },
    { id: "code", name: "VS Code", command: "code {path}", file_command: null },
  ],
  default_application: "code",
} as unknown as CheckoutSettings;

const JOB = { entityId: "jira:PAY-999", label: "PAY-999" };

const MERGE = {
  write_op: "merge_pull_request",
  label: "Merge…",
  fields: [{ name: "style", label: "Style", kind: { type: "choice", options: [{ value: "merge", label: "Merge" }, { value: "squash", label: "Squash" }] }, value: "merge", required: true, placeholder: null }],
  confirm: null,
  primary: true,
  danger: false,
  disabled: null,
  with_settings: false,
  values: {},
  entity: null,
} as unknown as ViewAction;

/** A backend that keeps the keys it is told, by the tables' rules: one chord per list, one per action on a kind. */
function keyBackend(lists: { list_id: string; chord: string }[] = [], actions: ActionKey[] = []) {
  const backend = {
    lists: [...lists],
    actions: [...actions],
    keys: vi.fn(async () => ({
      lists: backend.lists.map((key): ListKey => ({ ...key, label: "sepa payouts", text: "#t sepa" })),
      actions: [...backend.actions],
    })),
    bindListKey: vi.fn(async (listId: string, chord: string | null) => {
      backend.lists = [...backend.lists.filter((key) => key.list_id !== listId && key.chord !== chord), ...(chord ? [{ list_id: listId, chord }] : [])];
    }),
    bindActionKey: vi.fn(async (kind: string, actionId: string, chord: string | null) => {
      backend.actions = [
        ...backend.actions.filter((key) => key.kind !== kind || (key.action_id !== actionId && key.chord !== chord)),
        ...(chord ? [{ kind, chord, action_id: actionId }] : []),
      ];
    }),
  };
  return backend;
}

function mountIt(backend = keyBackend()) {
  const calls = {
    navigate: vi.fn(),
    createLink: vi.fn(async () => ({})),
    submitWrite: vi.fn(async () => ({})),
    openCheckout: vi.fn(async () => ["code", "/src/svc"]),
    branches: vi.fn(async () => [{ name: "main", isDefault: true }]),
    mergeAction: vi.fn(async (): Promise<ViewAction | null> => null),
    checkoutSettings: vi.fn(async () => APPS),
    backend,
  };
  app = mount(Launcher, {
    target,
    props: {
      open: true,
      onnavigate: calls.navigate,
      onclose: () => {},
      ontimer: vi.fn(),
      openEntity: JOB,
      keys: createKeys(backend),
      ports: { search: async (query: SearchQuery) => response(query.raw), launcherHome: async () => HOME },
      sheetPorts: {
        createLink: calls.createLink,
        submitWrite: calls.submitWrite,
        copyText: vi.fn(async () => {}),
        openCheckout: calls.openCheckout,
        checkoutSettings: calls.checkoutSettings,
        repoOf: vi.fn(async () => "gitea:acme/svc"),
        branches: calls.branches,
        linkSets: vi.fn(async () => []),
        mergeAction: calls.mergeAction,
      },
    },
  });
  flushSync();
  return calls;
}

async function settle() {
  for (let i = 0; i < 8; i += 1) await Promise.resolve();
  flushSync();
}

const input = () => target.querySelector("input")!;

async function type(text: string) {
  input().value = text;
  input().dispatchEvent(new Event("input", { bubbles: true }));
  vi.advanceTimersByTime(DEBOUNCE_MS);
  await settle();
}

function narrow(text: string) {
  input().value = text;
  input().dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
}

function press(key: string, options: KeyboardEventInit = {}) {
  const event = new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true, ...options });
  input().dispatchEvent(event);
  flushSync();
  return event;
}

/** A chord as a keyboard sends it: the key's character and its code. */
function chord(letter: string, mods: KeyboardEventInit = { metaKey: true }) {
  const code = /^[0-9]$/.test(letter) ? `Digit${letter}` : `Key${letter.toUpperCase()}`;
  return press(mods.shiftKey ? letter.toUpperCase() : letter.toLowerCase(), { code, ...mods });
}

const sheet = () => target.querySelector<HTMLElement>('[aria-label="Actions on the selected result"]');
const labels = () => [...(sheet()?.querySelectorAll(".tf-ln-nm") ?? [])].map((el) => el.textContent?.trim());
const modeLabel = () => target.querySelector(".tf-ln-mode")?.textContent ?? null;
const panel = () => target.querySelector<HTMLElement>(".tf-ln-bind");
const bigKeys = () => [...(panel()?.querySelectorAll(".tf-ln-bigkey") ?? [])].map((el) => el.textContent);
const refused = () => panel()?.querySelector(".tf-ln-bind-refused")?.textContent ?? null;
const lastToast = () => toasts.items.at(-1)?.text;
const button = (text: string) => [...target.querySelectorAll<HTMLButtonElement>("button")].find((el) => el.textContent?.trim() === text)!;
/** The chord drawn on the sheet's action row named `label`. */
const rowChord = (label: string) =>
  [...(sheet()?.querySelectorAll(".tf-ln-opt") ?? [])].find((el) => el.querySelector(".tf-ln-nm")?.textContent?.trim() === label)?.querySelector(".tf-ln-chord")?.textContent ?? null;

async function sheetOn(word: string) {
  await type(word);
  press("Tab");
  await settle();
}

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

test("⌘B on a selected action opens the panel, labelled Keys; it shows the modifiers while held and the chord at once, and Enter keeps it", async () => {
  const calls = mountIt();
  await settle();
  await sheetOn("repo");
  narrow("vs code");
  expect(chord("b").defaultPrevented).toBe(true);
  expect(modeLabel()).toBe("Keys");
  expect(panel()?.textContent).toContain("Open in VS Code");
  expect(panel()?.textContent).toContain("repository");

  press("Meta", { code: "MetaLeft", metaKey: true });
  expect(bigKeys()).toEqual(["⌘"]);
  press("Shift", { code: "ShiftLeft", metaKey: true, shiftKey: true });
  expect(bigKeys()).toEqual(["⌘", "⇧"]);
  chord("o", { metaKey: true, shiftKey: true });
  expect(bigKeys()).toEqual(["⌘", "⇧", "O"]);
  expect(calls.backend.bindActionKey).not.toHaveBeenCalled();

  press("Enter");
  await settle();
  expect(calls.backend.bindActionKey).toHaveBeenCalledWith("repo", "open-code", "⌘⇧O");
  expect(lastToast()).toBe("⌘⇧O runs Open in VS Code on any repository");
  expect(panel()).toBeNull();
  expect(modeLabel()).toBe("Actions");
  expect(rowChord("Open in VS Code")).toBe("⌘⇧O");
  expect(rowChord("Open in IntelliJ")).toBeNull();
});

test("Backspace clears the keys and OK then removes the key; Esc and Cancel leave things as they were", async () => {
  const calls = mountIt(keyBackend([], [{ kind: "repo", chord: "⌘⇧O", action_id: "open-code" }]));
  await settle();
  await sheetOn("repo");
  narrow("vs code");
  chord("b");
  expect(bigKeys(), "the panel opens on the chord bound now").toEqual(["⌘", "⇧", "O"]);

  // Esc: nothing written, the chord as it was.
  chord("1");
  expect(bigKeys()).toEqual(["⌘", "1"]);
  const esc = press("Escape");
  expect(esc.defaultPrevented).toBe(true);
  expect(target.querySelector(".tf-launch"), "Esc never reaches the dialog").not.toBeNull();
  expect(panel()).toBeNull();
  expect(calls.backend.bindActionKey).not.toHaveBeenCalled();
  expect(rowChord("Open in VS Code")).toBe("⌘⇧O");

  // Cancel: the same.
  chord("b");
  chord("2");
  button("Cancel").click();
  flushSync();
  expect(panel()).toBeNull();
  expect(calls.backend.bindActionKey).not.toHaveBeenCalled();

  // Backspace clears, and OK keeps the cleared state: the key is gone.
  chord("b");
  press("Backspace", { code: "Backspace" });
  expect(bigKeys()).toEqual([]);
  button("OK").click();
  await settle();
  expect(calls.backend.bindActionKey).toHaveBeenCalledWith("repo", "open-code", null);
  expect(lastToast()).toBe("Key removed from Open in VS Code");
  expect(rowChord("Open in VS Code")).toBeNull();
});

test("the box's own chords are refused with a message, and none of them does its usual thing in the panel", async () => {
  const calls = mountIt();
  await settle();
  await sheetOn("repo");
  narrow("clone");
  chord("b");
  for (const [letter, said] of [
    ["k", "⌘K is the launcher's own"],
    ["s", "⌘S is the launcher's own"],
    ["b", "⌘B is the launcher's own"],
    ["d", "⌘D is the launcher's own"],
  ] as const) {
    expect(chord(letter).defaultPrevented, letter).toBe(true);
    expect(refused()).toBe(said);
    expect(bigKeys()).toEqual([]);
  }
  press("?", { code: "Slash", metaKey: true, shiftKey: true });
  expect(refused()).toBe("⌘? is the launcher's own");
  // ⌘D did not switch to Everything, and ⌘? opened no filters.
  expect(target.querySelector(".tf-ln-scope button")?.textContent).toBe("Titles");
  expect(modeLabel()).toBe("Keys");
  // A free chord is taken and the message goes.
  chord("o", { metaKey: true, shiftKey: true });
  expect(refused()).toBeNull();
  expect(bigKeys()).toEqual(["⌘", "⇧", "O"]);
  expect(calls.backend.bindActionKey).not.toHaveBeenCalled();
});

test("an action's chord runs it on the selected row of its kind without opening the sheet", async () => {
  const calls = mountIt(keyBackend([], [{ kind: "repo", chord: "⌘⇧O", action_id: "open-code" }]));
  await settle();
  await type("repo");
  expect(chord("o", { metaKey: true, shiftKey: true }).defaultPrevented).toBe(true);
  await settle();
  expect(calls.openCheckout).toHaveBeenCalledWith("gitea:acme/svc", { application: "code" });
  expect(lastToast()).toBe("Started VS Code: code /src/svc");
});

test("one chord means another action on another kind, and nothing on a kind it is not bound on", async () => {
  const calls = mountIt(keyBackend([], [{ kind: "repo", chord: "⌘⇧O", action_id: "open-code" }]));
  await settle();
  await type("pr");
  expect(chord("o", { metaKey: true, shiftKey: true }).defaultPrevented, "not bound on a pull request: the key is left alone").toBe(false);
  await settle();
  expect(calls.openCheckout).not.toHaveBeenCalled();
  expect(calls.submitWrite).not.toHaveBeenCalled();
  expect(sheet()).toBeNull();

  // Bound on pull requests too: ⌘⇧O approves one.
  press("Tab");
  await settle();
  narrow("approve");
  chord("b");
  chord("o", { metaKey: true, shiftKey: true });
  press("Enter");
  await settle();
  expect(calls.backend.bindActionKey).toHaveBeenCalledWith("pr", "approve", "⌘⇧O");
  press("Escape");
  chord("o", { metaKey: true, shiftKey: true });
  await settle();
  expect(calls.submitWrite).toHaveBeenCalledWith({ Approve: { entity: "gitea:acme/svc#142", body: "" } });
  expect(calls.openCheckout).not.toHaveBeenCalled();
});

test("a flow's chord opens the sheet first and starts the flow there", async () => {
  mountIt(keyBackend([], [{ kind: "repo", chord: "⌘⇧N", action_id: "newpr" }]));
  await settle();
  await type("repo");
  chord("n", { metaKey: true, shiftKey: true });
  await settle();
  expect(sheet()).not.toBeNull();
  expect(modeLabel()).not.toBe("Actions");
  expect(labels()).toContain("main");
});

test("Merge…'s chord waits for the pull request's declared merge, then opens its flow in the sheet", async () => {
  const calls = mountIt(keyBackend([], [{ kind: "pr", chord: "⌘⇧G", action_id: "merge" }]));
  calls.mergeAction.mockImplementation(async () => MERGE);
  await settle();
  await type("pr");
  chord("g", { metaKey: true, shiftKey: true });
  await settle();
  expect(calls.mergeAction).toHaveBeenCalledWith("gitea:acme/svc#142");
  expect(modeLabel()).toBe("Merge as");
});

test("the link's chord opens the sheet on the link row, so the relation is picked before Enter writes it", async () => {
  const calls = mountIt(keyBackend([], [{ kind: "ticket", chord: "⌘⇧L", action_id: "link" }]));
  await settle();
  await type("ticket");
  chord("l", { metaKey: true, shiftKey: true });
  await settle();
  expect(modeLabel()).toBe("Actions");
  expect(sheet()?.querySelector(".tf-ln-on .tf-ln-nm")?.textContent).toBe("Link to PAY-999");
  expect(calls.createLink).not.toHaveBeenCalled();
  press("Enter");
  await settle();
  expect(calls.createLink).toHaveBeenCalled();
});

test("Assign key… on a saved list binds the chord that applies it; the list row shows it", async () => {
  const calls = mountIt();
  await settle();
  await sheetOn("list");
  expect(labels()).toContain("Assign key…");
  narrow("assign");
  press("Enter");
  flushSync();
  expect(modeLabel()).toBe("Keys");
  expect(panel()?.textContent).toContain("sepa payouts");
  chord("1");
  press("Enter");
  await settle();
  expect(calls.backend.bindListKey).toHaveBeenCalledWith("sepa-payouts", "⌘1");
  expect(lastToast()).toBe("⌘1 opens sepa payouts");
  press("Escape");
  expect(target.querySelector(".tf-ln-res .tf-ln-chord")?.textContent).toBe("⌘1");

  // From another search, ⌘1 applies the list.
  await type("repo");
  expect(chord("1").defaultPrevented).toBe(true);
  expect(input().value).toBe("#t sepa ");
});

test("a chord a list holds is refused for an action, and the other way round", async () => {
  mountIt(keyBackend([{ list_id: "sepa-payouts", chord: "⌘1" }], [{ kind: "repo", chord: "⌘⇧O", action_id: "open-code" }]));
  await settle();
  await sheetOn("repo");
  narrow("clone");
  chord("b");
  chord("1");
  expect(refused()).toBe("⌘1 opens sepa payouts");
  expect(bigKeys()).toEqual([]);
  press("Escape");
  press("Escape");
  await sheetOn("list");
  narrow("assign");
  press("Enter");
  flushSync();
  chord("o", { metaKey: true, shiftKey: true });
  expect(refused()).toBe("⌘⇧O runs an action on a repository");
});
