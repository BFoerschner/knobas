/**
 * One opening of the key panel (#2429, moved here by #2458), driven by the
 * presses a box hands it, over a key store on a fake backend that keeps what it
 * is told. The assertions are what the panel draws from, what was written and
 * what was said; `Launcher.keys` covers the same panel through the component.
 */
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { ActionKey, ListKey } from "../ipc";
import { reset as resetToasts, toasts } from "../shell/toasts.svelte";
import { KeyPanelMode, type KeyTarget } from "./key-panel.svelte";
import { createKeys } from "./keys.svelte";

const ON_PR: KeyTarget = { kind: "action", on: "pr", actionId: "approve", label: "Approve" };
const ON_LIST: KeyTarget = { kind: "list", id: "sepa", label: "sepa payouts" };

function press(key: string, init: KeyboardEventInit = {}) {
  return new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true, ...init });
}
const chordO = () => press("O", { code: "KeyO", metaKey: true, shiftKey: true });

/** A panel on `target`, over a store that has read `lists` and `actions`. */
async function panelOn(target: KeyTarget, held: { lists?: ListKey[]; actions?: ActionKey[] } = {}) {
  const ports = {
    keys: vi.fn(async () => ({ lists: held.lists ?? [], actions: held.actions ?? [] })),
    bindListKey: vi.fn(async (_list: string, _chord: string | null) => {}),
    bindActionKey: vi.fn(async (_kind: string, _action: string, _chord: string | null) => {}),
  };
  const keys = createKeys(ports);
  await keys.load();
  const host = { keys, focus: vi.fn(), leave: vi.fn() };
  return { panel: new KeyPanelMode(target, host), host, ports };
}

beforeEach(() => resetToasts());
afterEach(() => resetToasts());

test("it opens on the chord the target has now", async () => {
  const { panel } = await panelOn(ON_PR, { actions: [{ kind: "pr", action_id: "approve", chord: "⌘⇧A" }] });
  expect(panel.chord).toBe("⌘⇧A");
  expect(panel.refused).toBeNull();
});

test("a pressed chord is drawn, and Enter keeps it: written once, said, and the panel left", async () => {
  const { panel, host, ports } = await panelOn(ON_PR);
  panel.press(chordO());
  expect(panel.chord).toBe("⌘⇧O");
  expect(ports.bindActionKey).not.toHaveBeenCalled();

  panel.press(press("Enter"));
  await vi.waitFor(() => expect(toasts.items).toHaveLength(1));
  expect(ports.bindActionKey).toHaveBeenCalledExactlyOnceWith("pr", "approve", "⌘⇧O");
  expect(toasts.items[0]?.text).toBe("⌘⇧O runs Approve on any pull request");
  expect(host.leave).toHaveBeenCalledOnce();
});

test("a list's chord is written to the list's table and said as opening it", async () => {
  const { panel, ports } = await panelOn(ON_LIST);
  panel.press(chordO());
  await panel.keep();
  expect(ports.bindListKey).toHaveBeenCalledExactlyOnceWith("sepa", "⌘⇧O");
  expect(ports.bindActionKey).not.toHaveBeenCalled();
  expect(toasts.items[0]?.text).toBe("⌘⇧O opens sepa payouts");
});

test("keeping the chord it opened on writes nothing and says nothing", async () => {
  const { panel, host, ports } = await panelOn(ON_PR, { actions: [{ kind: "pr", action_id: "approve", chord: "⌘⇧A" }] });
  await panel.keep();
  expect(ports.bindActionKey).not.toHaveBeenCalled();
  expect(toasts.items).toEqual([]);
  expect(host.leave).toHaveBeenCalledOnce();
});

test("Backspace clears, and keeping nothing takes the key away", async () => {
  const { panel, ports } = await panelOn(ON_PR, { actions: [{ kind: "pr", action_id: "approve", chord: "⌘⇧A" }] });
  panel.press(press("Backspace"));
  expect(panel.chord).toBeNull();
  await panel.keep();
  expect(ports.bindActionKey).toHaveBeenCalledExactlyOnceWith("pr", "approve", null);
  expect(toasts.items[0]?.text).toBe("Key removed from Approve");
});

test("Clear does what Backspace does, and hands the box its focus back from the button", async () => {
  const { panel, host } = await panelOn(ON_PR, { actions: [{ kind: "pr", action_id: "approve", chord: "⌘⇧A" }] });
  panel.clear();
  expect(panel.chord).toBeNull();
  expect(host.focus).toHaveBeenCalledOnce();
});

test("Esc and Cancel leave things as they were", async () => {
  const { panel, host, ports } = await panelOn(ON_PR);
  panel.press(chordO());
  const esc = press("Escape");
  panel.press(esc);
  expect(esc.defaultPrevented).toBe(true);
  expect(host.leave).toHaveBeenCalledOnce();
  panel.cancel();
  expect(host.leave).toHaveBeenCalledTimes(2);
  expect(ports.bindActionKey).not.toHaveBeenCalled();
});

test("a chord the box answers is refused, the chord pressed so far kept, and the press goes no further", async () => {
  const { panel } = await panelOn(ON_PR);
  panel.press(chordO());
  const refused = press("d", { code: "KeyD", metaKey: true });
  const stopped = vi.spyOn(refused, "stopPropagation");
  panel.press(refused);
  expect(stopped).toHaveBeenCalled();
  expect(refused.defaultPrevented).toBe(true);
  expect(panel.refused).not.toBeNull();
  expect(panel.chord).toBe("⌘⇧O");
  expect(panel.held).toEqual([]);
});

test("an action can't take a list's chord, nor a list an action's, on any kind", async () => {
  const list: ListKey = { list_id: "other", chord: "⌘⇧O", label: "other list", text: "#t x" };
  const onAction = await panelOn(ON_PR, { lists: [list] });
  onAction.panel.press(chordO());
  expect(onAction.panel.refused).toBe("⌘⇧O opens other list");
  expect(onAction.panel.chord).toBeNull();

  const onList = await panelOn(ON_LIST, { actions: [{ kind: "repo", action_id: "clone", chord: "⌘⇧O" }] });
  onList.panel.press(chordO());
  expect(onList.panel.refused).toBe("⌘⇧O runs an action on a repository");
});

test("a good press after a refused one clears the refusal", async () => {
  const { panel } = await panelOn(ON_PR);
  panel.press(press("d", { code: "KeyD", metaKey: true }));
  expect(panel.refused).not.toBeNull();
  panel.press(chordO());
  expect(panel.refused).toBeNull();
});

test("the modifiers are drawn while held and gone when let go", async () => {
  const { panel } = await panelOn(ON_PR);
  panel.press(press("Meta", { metaKey: true }));
  expect(panel.held).toEqual(["⌘"]);
  expect(panel.chord).toBeNull();
  panel.release(new KeyboardEvent("keyup", { key: "Meta" }));
  expect(panel.held).toEqual([]);
});

test("the backend's refusal is said; the panel has already left", async () => {
  const { panel, host, ports } = await panelOn(ON_PR);
  ports.bindActionKey.mockRejectedValueOnce(new Error("no"));
  panel.press(chordO());
  await panel.keep();
  expect(host.leave).toHaveBeenCalledOnce();
  expect(toasts.items[0]?.tone).toBe("err");
});
