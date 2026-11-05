/**
 * The Tab sheet as a mode (#2426, #2427, #2429; moved here by #2458), driven by
 * the keys a box hands it, over a fake session and fake ports. `Launcher.sheet`
 * and `Launcher.keys` cover the same behaviour through the component; these pin
 * what the module owes its caller, and the races a component test can't pace.
 */
import { flushSync, tick } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { ActionKey, ListKey, SearchHit } from "../ipc";
import type { CheckoutSettings, ViewAction } from "../ipc/entity";
import { reset as resetToasts, toasts } from "../shell/toasts.svelte";
import { createKeys } from "./keys.svelte";
import type { LauncherRow } from "./rows";
import { SheetMode, type SheetHost } from "./sheet-mode.svelte";
import type { SheetPorts } from "./sheet-ports";

function hit(kind: string, entity_id: string, title: string, more: Partial<SearchHit> = {}): LauncherRow {
  const full = { kind, entity_id, title, source_id: "src", updated_at: null, synced_at: "2026-08-25T11:56:00Z", path: null, state: null, rank: 1, snippet: [], excerpts: [], ...more } as SearchHit;
  return { kind: "hit", id: `hit:${entity_id}`, group: { kind, label: kind, plural: kind, monogram: "XX", total: 1, hits: [full] }, hit: full };
}
const TICKET = hit("ticket", "jira:PAY-1", "Retry payouts");
const OTHER = hit("ticket", "jira:PAY-2", "Other");
const REPO = hit("repo", "gitea:acme/svc", "acme/svc");
const PR = hit("pr", "gitea:acme/svc#7", "Backoff", { path: "acme/svc" });
const LIST = hit("list", "sepa", "sepa payouts", { source_id: "list", path: "#t sepa" });
const SYNTAX: LauncherRow = { kind: "syntax", id: "syntax:x", entry: { token: "#x", insert: "#x ", summary: "" } } as LauncherRow;

const APPS = {
  applications: [{ id: "code", name: "VS Code", command: "code {path}", file_command: null }],
  default_application: "code",
} as unknown as CheckoutSettings;

const MERGE = {
  write_op: "merge_pull_request",
  label: "Merge…",
  fields: [{ name: "style", label: "Style", kind: { type: "choice", options: [{ value: "squash", label: "Squash" }] }, value: "squash", required: true, placeholder: null }],
  confirm: null,
  primary: true,
  danger: false,
  disabled: null,
  with_settings: false,
  values: {},
  entity: null,
} as unknown as ViewAction;

const BRANCHES = [
  { name: "main", isDefault: true },
  { name: "feature", isDefault: false },
];

let stop: (() => void) | null = null;

/** A `SheetMode` over a fake session on `row`, inside an effect root as a component's init would be. */
function setUp(row: LauncherRow | null, options: { lists?: ListKey[]; actions?: ActionKey[]; job?: { entityId: string; label: string }; io?: Partial<SheetPorts> } = {}) {
  const session = $state({
    current: row,
    home: null,
    error: null as string | null,
    set: vi.fn((_raw: string) => {}),
    renameList: vi.fn(async (_id: string, _label: string) => true),
    deleteList: vi.fn(async (_id: string) => true),
    favourite: vi.fn(async () => true),
  });
  const keys = createKeys({
    keys: async () => ({ lists: options.lists ?? [], actions: options.actions ?? [] }),
    bindListKey: async () => {},
    bindActionKey: async () => {},
  });
  const io = {
    createLink: vi.fn(async () => {}),
    submitWrite: vi.fn(async () => {}),
    copyText: vi.fn(async () => {}),
    checkoutSettings: vi.fn(async () => APPS),
    linkSets: vi.fn(async () => []),
    mergeAction: vi.fn(async () => null),
    branches: vi.fn(async () => BRANCHES),
    ...options.io,
  } as unknown as SheetPorts & { [K in "createLink" | "copyText" | "checkoutSettings" | "linkSets" | "mergeAction" | "branches" | "submitWrite"]: ReturnType<typeof vi.fn> };
  const ontimer = vi.fn();
  const host = {
    session: session as unknown as SheetHost["session"],
    io,
    keys,
    job: options.job,
    ontimer,
    now: new Date("2026-08-25T12:00:00Z"),
    inboxOps: () => [],
    focus: vi.fn(),
    close: vi.fn(),
    navigate: vi.fn(),
  };
  let sheet!: SheetMode;
  stop = $effect.root(() => {
    sheet = new SheetMode(host);
  });
  flushSync();
  return { sheet, session, host, io, keys };
}

function press(key: string, init: KeyboardEventInit = {}) {
  return new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true, ...init });
}
const labels = (sheet: SheetMode) => sheet.shown.map((action) => action.label);
const choose = (sheet: SheetMode, label: string) => {
  const action = sheet.shown.find((one) => one.label === label);
  if (!action) throw new Error(`no ${label} in ${labels(sheet).join(", ")}`);
  sheet.perform(action);
};
/** Let the answers of the fake reads land. */
const landed = async () => {
  for (let i = 0; i < 4; i += 1) await tick();
  flushSync();
};

beforeEach(() => resetToasts());
afterEach(() => {
  stop?.();
  stop = null;
  resetToasts();
});

test("it opens on the selected row, says what the box asks, and gives the box its focus", () => {
  const { sheet, host } = setUp(TICKET);
  expect(sheet.current).toBeNull();
  sheet.open();
  expect(sheet.current?.rowId).toBe(TICKET.id);
  expect(sheet.label).toBe("Actions");
  expect(sheet.placeholder).toBe("narrow the actions");
  expect(sheet.text).toBe("");
  expect(sheet.onActions).toBe(true);
  expect(host.focus).toHaveBeenCalledOnce();
});

test("a row with nothing to do opens nothing, and no row neither", () => {
  const onSyntax = setUp(SYNTAX);
  expect(onSyntax.sheet.opensOn(SYNTAX)).toBe(false);
  onSyntax.sheet.open();
  expect(onSyntax.sheet.current).toBeNull();
  stop?.();

  const onNothing = setUp(null);
  expect(onNothing.sheet.opensOn(null)).toBe(false);
  onNothing.sheet.open();
  expect(onNothing.sheet.current).toBeNull();
  expect(onNothing.io.checkoutSettings).not.toHaveBeenCalled();
});

test("the applications and the sets are read once per opening of the launcher; a search that opens no sheet costs no read", async () => {
  const { sheet, io } = setUp(TICKET);
  expect(sheet.opensOn(TICKET)).toBe(true);
  expect(io.checkoutSettings).not.toHaveBeenCalled();
  sheet.open();
  sheet.leave();
  sheet.open();
  expect(io.checkoutSettings).toHaveBeenCalledOnce();
  expect(io.linkSets).toHaveBeenCalledOnce();
  await landed();
  sheet.reset();
  sheet.open();
  expect(io.checkoutSettings).toHaveBeenCalledTimes(2);
});

test("a selection that leaves the row closes the sheet; one that stays keeps it", () => {
  const { sheet, session } = setUp(TICKET);
  sheet.open();
  session.current = { ...TICKET };
  flushSync();
  expect(sheet.current).not.toBeNull();
  session.current = OTHER;
  flushSync();
  expect(sheet.current).toBeNull();
});

test("typing narrows and goes back to the first row; Backspace leaves only on an empty box", () => {
  const { sheet, host } = setUp(TICKET);
  sheet.open();
  sheet.hover(2);
  sheet.type("copy");
  expect(sheet.text).toBe("copy");
  expect(sheet.current?.selected).toBe(0);
  expect(labels(sheet)).toEqual(["Copy key"]);
  sheet.key(press("Backspace"));
  expect(sheet.current).not.toBeNull();
  sheet.type("");
  host.focus.mockClear();
  sheet.key(press("Backspace"));
  expect(sheet.current).toBeNull();
  expect(host.focus).toHaveBeenCalledOnce();
});

test("Esc is the sheet's and goes no further; ⌘S and ⌘? do nothing here", () => {
  const { sheet } = setUp(TICKET);
  sheet.open();
  const save = press("s", { metaKey: true });
  sheet.key(save);
  expect(save.defaultPrevented).toBe(true);
  expect(sheet.current).not.toBeNull();
  const esc = press("Escape");
  const stopped = vi.spyOn(esc, "stopPropagation");
  sheet.key(esc);
  expect(stopped).toHaveBeenCalled();
  expect(sheet.current).toBeNull();
});

test("an address closes the launcher first, so it lands on a shell with nothing over it", () => {
  const { sheet, host } = setUp(TICKET);
  sheet.open();
  choose(sheet, "Work on this");
  expect(host.close).toHaveBeenCalledOnce();
  expect(host.navigate.mock.calls[0]?.[0]).toContain("#/job/");
  expect(host.close.mock.invocationCallOrder[0]!).toBeLessThan(host.navigate.mock.invocationCallOrder[0]!);
});

test("a write that lands is said and closes; one that fails says why and keeps the sheet", async () => {
  const good = setUp(TICKET);
  good.sheet.open();
  choose(good.sheet, "Copy key");
  await landed();
  expect(good.io.copyText).toHaveBeenCalledExactlyOnceWith("PAY-1");
  expect(toasts.items.at(-1)?.text).toBe("Copied PAY-1");
  expect(good.host.close).toHaveBeenCalledOnce();
  stop?.();

  const bad = setUp(TICKET, { io: { copyText: async () => Promise.reject(new Error("denied")) } });
  bad.sheet.open();
  choose(bad.sheet, "Copy key");
  await landed();
  expect(toasts.items.at(-1)?.tone).toBe("err");
  expect(bad.host.close).not.toHaveBeenCalled();
  expect(bad.sheet.current).not.toBeNull();
});

test("← → walk the link's relation round the list, and Enter writes it against the job the host names", async () => {
  const { sheet, io, host } = setUp(OTHER, { job: { entityId: "jira:PAY-9", label: "PAY-9" } });
  sheet.open();
  expect(labels(sheet)[0]).toBe("Link to PAY-9");
  sheet.key(press("ArrowLeft"));
  expect(sheet.current?.relation).toBe(5);
  sheet.key(press("ArrowRight"));
  sheet.key(press("ArrowRight"));
  expect(sheet.current?.relation).toBe(1);
  sheet.key(press("Enter"));
  await landed();
  expect(io.createLink).toHaveBeenCalledExactlyOnceWith("jira:PAY-9", "jira:PAY-2", "implements");
  expect(toasts.items.at(-1)?.text).toBe("Linked PAY-2 to PAY-9 as implements");
  expect(host.close).toHaveBeenCalledOnce();
});

test("← → on a row that is not the link leave the relation and the caret alone", () => {
  const { sheet } = setUp(TICKET);
  sheet.open();
  const left = press("ArrowLeft");
  sheet.key(left);
  expect(left.defaultPrevented).toBe(false);
  expect(sheet.current?.relation).toBe(0);
});

test("a flow reads each step, steps back without a read, and its last pick goes through what it finishes as", async () => {
  const { sheet, io, host } = setUp(REPO);
  sheet.open();
  choose(sheet, "Create pull request…");
  expect(sheet.label).toBe("From branch");
  expect(sheet.placeholder).toBe("narrow the choices");
  expect(sheet.onActions).toBe(false);
  expect(sheet.shown).toEqual([]);
  expect(sheet.active).toBeNull();
  await landed();
  expect(sheet.choices.map((one) => one.id)).toEqual(["main", "feature"]);
  expect(sheet.active).toBe(0);

  sheet.key(press("ArrowDown"));
  sheet.key(press("Enter"));
  expect(sheet.label).toBe("Into branch");
  await landed();
  expect(sheet.choices.map((one) => one.id)).toEqual(["main"]);
  expect(io.branches).toHaveBeenCalledTimes(2);

  sheet.key(press("Escape"));
  expect(sheet.label).toBe("From branch");
  expect(sheet.choices.map((one) => one.id)).toEqual(["main", "feature"]);
  expect(io.branches).toHaveBeenCalledTimes(2);

  sheet.pick({ id: "feature", label: "feature", detail: "" });
  await landed();
  sheet.key(press("Enter"));
  expect(host.navigate).toHaveBeenCalledOnce();
  expect(sheet.current?.flow).toBeNull();
});

test("Esc on a flow's first step goes back to the actions, and Tab leaves the sheet from any step", async () => {
  const { sheet } = setUp(REPO);
  sheet.open();
  choose(sheet, "Create pull request…");
  await landed();
  sheet.key(press("Escape"));
  expect(sheet.current).not.toBeNull();
  expect(sheet.current?.flow).toBeNull();
  expect(sheet.label).toBe("Actions");
  choose(sheet, "Create pull request…");
  sheet.key(press("Tab"));
  expect(sheet.current).toBeNull();
});

test("a step's answer that lands after a step back is dropped, and a refused read says why and goes back to the actions", async () => {
  let answer: (value: typeof BRANCHES) => void = () => {};
  const slow = setUp(REPO, { io: { branches: () => new Promise((resolve) => (answer = resolve)) } });
  slow.sheet.open();
  choose(slow.sheet, "Create pull request…");
  slow.sheet.key(press("Escape"));
  answer(BRANCHES);
  await landed();
  expect(slow.sheet.current?.flow).toBeNull();
  stop?.();

  const refused = setUp(REPO, { io: { branches: async () => Promise.reject(new Error("offline")) } });
  refused.sheet.open();
  choose(refused.sheet, "Create pull request…");
  await landed();
  expect(toasts.items.at(-1)?.tone).toBe("err");
  expect(refused.sheet.current?.flow).toBeNull();
  expect(refused.sheet.current).not.toBeNull();
});

test("a rename asks in the box with the text selected; Enter keeps it, Esc leaves the name as it was", async () => {
  const { sheet, session, host } = setUp(LIST);
  sheet.open();
  choose(sheet, "Rename");
  expect(sheet.label).toBe("New name");
  expect(sheet.text).toBe("sepa payouts");
  expect(sheet.active).toBeNull();
  await landed();
  expect(host.focus).toHaveBeenLastCalledWith({ select: true });

  sheet.key(press("Escape"));
  expect(sheet.current?.renaming).toBeNull();
  expect(session.renameList).not.toHaveBeenCalled();

  choose(sheet, "Rename");
  sheet.type("  ");
  sheet.key(press("Enter"));
  expect(session.renameList).not.toHaveBeenCalled();
  sheet.type("payouts");
  sheet.key(press("Enter"));
  expect(sheet.current).toBeNull();
  await landed();
  expect(session.renameList).toHaveBeenCalledExactlyOnceWith("sepa", "payouts");
  expect(toasts.items.at(-1)?.text).toBe("Renamed to payouts");
});

test("⌘B opens the panel on the selected action; it takes no text, hears key-up, and gives back to the actions", () => {
  const { sheet } = setUp(TICKET);
  sheet.open();
  sheet.type("copy");
  sheet.key(press("b", { code: "KeyB", metaKey: true }));
  const panel = sheet.current?.panel;
  expect(panel?.target).toEqual({ kind: "action", on: "ticket", actionId: "copy", label: "Copy key" });
  expect(sheet.label).toBe("Keys");
  expect(sheet.placeholder).toBe("press the keys");
  expect(sheet.text).toBe("");
  expect(sheet.onActions).toBe(false);
  sheet.type("x");
  expect(sheet.current?.filter).toBe("copy");

  flushSync();
  panel?.press(press("Meta", { metaKey: true }));
  expect(panel?.held).toEqual(["⌘"]);
  window.dispatchEvent(new KeyboardEvent("keyup", { key: "Meta" }));
  expect(panel?.held).toEqual([]);

  panel?.cancel();
  expect(sheet.current?.panel).toBeNull();
  expect(sheet.current).not.toBeNull();
  // The listener went with the panel.
  flushSync();
  panel?.press(press("Meta", { metaKey: true }));
  window.dispatchEvent(new KeyboardEvent("keyup", { key: "Meta" }));
  expect(panel?.held).toEqual(["⌘"]);
});

test("⌘B opens no panel in a flow or a rename, and *Assign key…* opens the list's own", async () => {
  const onRepo = setUp(REPO);
  onRepo.sheet.open();
  choose(onRepo.sheet, "Create pull request…");
  await landed();
  onRepo.sheet.key(press("b", { code: "KeyB", metaKey: true }));
  expect(onRepo.sheet.current?.panel).toBeNull();
  stop?.();

  const onList = setUp(LIST);
  onList.sheet.open();
  choose(onList.sheet, "Assign key…");
  expect(onList.sheet.current?.panel?.target).toEqual({ kind: "list", id: "sepa", label: "sepa payouts" });
});

test("what an action row draws: its key on this kind, and on *Assign key…* the list's own", async () => {
  const lists: ListKey[] = [{ list_id: "sepa", chord: "⌘⇧L", label: "sepa payouts", text: "#t sepa" }];
  const actions: ActionKey[] = [{ kind: "list", action_id: "rename", chord: "⌘⇧R" }];
  const { sheet, keys } = setUp(LIST, { lists, actions });
  await keys.load();
  sheet.open();
  const of = (label: string) => sheet.boundTo(sheet.shown.find((one) => one.label === label)!);
  expect(of("Assign key…")).toBe("⌘⇧L");
  expect(of("Rename")).toBe("⌘⇧R");
  expect(of("Delete")).toBeNull();
});

const CHORD = () => press("O", { code: "KeyO", metaKey: true, shiftKey: true });

test("a list's chord closes the sheet and puts the list's text in the box", async () => {
  const lists: ListKey[] = [{ list_id: "sepa", chord: "⌘⇧O", label: "sepa payouts", text: "#t sepa" }];
  const { sheet, session, keys } = setUp(TICKET, { lists });
  await keys.load();
  sheet.open();
  const chord = CHORD();
  expect(sheet.runChord(chord)).toBe(true);
  expect(chord.defaultPrevented).toBe(true);
  expect(sheet.current).toBeNull();
  expect(session.set).toHaveBeenCalledExactlyOnceWith("#t sepa ");
});

test("an action's chord runs it on the selected row with the sheet never drawn; a chord nothing holds goes on", async () => {
  const actions: ActionKey[] = [{ kind: "ticket", action_id: "copy", chord: "⌘⇧O" }];
  const { sheet, io, keys } = setUp(TICKET, { actions });
  await keys.load();
  expect(sheet.runChord(press("P", { code: "KeyP", metaKey: true, shiftKey: true }))).toBe(false);
  expect(sheet.runChord(CHORD())).toBe(true);
  await landed();
  expect(sheet.current).toBeNull();
  expect(io.copyText).toHaveBeenCalledExactlyOnceWith("PAY-1");
});

test("an action's chord that asks something opens the sheet under it, and none runs in a flow or a rename", async () => {
  const actions: ActionKey[] = [{ kind: "repo", action_id: "newpr", chord: "⌘⇧O" }];
  const { sheet, keys } = setUp(REPO, { actions });
  await keys.load();
  expect(sheet.runChord(CHORD())).toBe(true);
  await landed();
  expect(sheet.current?.flow).not.toBeNull();
  expect(sheet.runChord(CHORD())).toBe(false);
});

test("an action's chord on the link opens the sheet on that row, for ← → to pick the relation", async () => {
  const actions: ActionKey[] = [{ kind: "ticket", action_id: "link", chord: "⌘⇧O" }];
  const { sheet, io, keys } = setUp(OTHER, { actions, job: { entityId: "jira:PAY-9", label: "PAY-9" } });
  await keys.load();
  sheet.runChord(CHORD());
  await landed();
  expect(sheet.current?.selected).toBe(0);
  expect(labels(sheet)[0]).toBe("Link to PAY-9");
  expect(io.createLink).not.toHaveBeenCalled();
});

test("a chord on an action the reads add waits for them, and says so when it does not come", async () => {
  const actions: ActionKey[] = [{ kind: "pr", action_id: "merge", chord: "⌘⇧O" }];
  const has = setUp(PR, { actions, io: { mergeAction: async () => MERGE } });
  await has.keys.load();
  has.sheet.runChord(CHORD());
  await landed();
  expect(has.sheet.label).toBe("Merge as");
  stop?.();

  const hasNot = setUp(PR, { actions });
  await hasNot.keys.load();
  hasNot.sheet.runChord(CHORD());
  await landed();
  expect(hasNot.sheet.current).toBeNull();
  expect(toasts.items.at(-1)?.text).toBe("⌘⇧O runs nothing on Backoff now");
});

test("a chord still waiting when the selection moves runs nothing", async () => {
  const actions: ActionKey[] = [{ kind: "pr", action_id: "merge", chord: "⌘⇧O" }];
  let answer: (action: ViewAction) => void = () => {};
  const { sheet, session, keys } = setUp(PR, { actions, io: { mergeAction: () => new Promise((resolve) => (answer = resolve)) } });
  await keys.load();
  sheet.runChord(CHORD());
  session.current = OTHER;
  flushSync();
  answer(MERGE);
  await landed();
  expect(sheet.current).toBeNull();
  expect(toasts.items).toEqual([]);
});
