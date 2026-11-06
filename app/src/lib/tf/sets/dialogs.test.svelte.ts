/**
 * The keys of the dialogs around a link set (F-530): `↓` from a filter into its list, the arrows along it, and `Enter`
 * where the primary can go, in *Apply a link set…*, *Save as a link set…* and the set editor's *Add…*.
 */
import { flushSync, mount, tick, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { EntityRow, LinkSetRow } from "../../ipc/entity";
import TfApplySet from "../job/TfApplySet.svelte";
import TfSaveSet from "../job/TfSaveSet.svelte";
import { createLayers } from "../layers.svelte";
import TfSetPicker from "./TfSetPicker.svelte";

const set = (id: string, name: string): LinkSetRow => ({ id, name, description: "", members: [], patterns: [], rules: [], tickets: 0, mine: 0 });
const SETS = [set("a", "Finance docs"), set("b", "Ledger stack"), set("c", "On-call kit")];

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;

const press = (on: Element, key: string) => {
  const event = new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true });
  on.dispatchEvent(event);
  flushSync();
  return event;
};
const settle = async () => {
  await Promise.resolve();
  await tick();
  flushSync();
};

beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
});

afterEach(() => {
  if (view) void unmount(view);
  view = null;
  target.remove();
});

function apply(applied: string[] = []) {
  const onapply = vi.fn();
  view = mount(TfApplySet, {
    target,
    props: { jobKey: "PAY-231", applied: new Set(applied), read: async () => SETS, layers: createLayers(), onapply, onclose: () => {} },
  });
  return onapply;
}

test("Apply: ↓ goes from the filter into the list and picks, ↑ from the top comes back, and Enter applies", async () => {
  const onapply = apply(["a"]);
  await settle();
  const filter = target.querySelector<HTMLInputElement>("input[aria-label='Filter link sets']")!;
  expect(document.activeElement).toBe(filter);

  press(filter, "ArrowDown");
  const ledger = target.querySelector<HTMLInputElement>("input[value='b']")!;
  expect(document.activeElement, "past the set that already applies").toBe(ledger);
  expect(ledger.checked).toBe(true);

  press(ledger, "ArrowUp");
  expect(document.activeElement).toBe(filter);

  press(filter, "ArrowDown");
  press(document.activeElement!, "Enter");
  expect(onapply).toHaveBeenCalledWith(expect.objectContaining({ id: "b" }));
});

test("Apply: Enter on a row with nothing to apply does nothing", async () => {
  const onapply = apply(["a", "b", "c"]);
  await settle();
  const filter = target.querySelector<HTMLInputElement>("input[aria-label='Filter link sets']")!;
  press(filter, "ArrowDown");
  expect(document.activeElement, "no row can be picked").toBe(filter);
  expect(onapply).not.toHaveBeenCalled();
});

const ticket: Pick<EntityRow, "entity_id" | "source_id" | "path" | "title"> = { entity_id: "jira:PAY-231", source_id: "jira", path: "PAY / PAY-200 / PAY-231", title: "Retry" };

function save() {
  const saveLinkSet = vi.fn(async (draft: { name: string }) => ({ ...set("new", draft.name), shows_on: [] }));
  const onsaved = vi.fn();
  view = mount(TfSaveSet, {
    target,
    props: {
      ticket,
      links: [{ kind: "repo", entity_id: "gitea:a/b", title: "a/b" }] as never,
      ports: { saveLinkSet, previewLinkSetRule: async () => [], getEntity: async () => { throw new Error("none"); } } as never,
      layers: createLayers(),
      onsaved,
      onclose: () => {},
    },
  });
  return { saveLinkSet, onsaved };
}

test("Save: the primary and the name share a form, so Enter in the name saves", async () => {
  save();
  await settle();
  const name = target.querySelector<HTMLInputElement>("input[aria-label='Name']")!;
  const primary = [...target.querySelectorAll("button")].find((one) => one.textContent === "Save link set")!;
  expect(name.getAttribute("form")).toBe(primary.getAttribute("form"));
  expect(primary.getAttribute("type")).toBe("submit");
});

test("Save: Enter on a picked rule saves", async () => {
  const { saveLinkSet, onsaved } = save();
  await settle();
  const rule = target.querySelector<HTMLInputElement>("input[type='radio']")!;
  rule.focus();
  press(rule, "Enter");
  await settle();
  await settle();
  expect(saveLinkSet).toHaveBeenCalledTimes(1);
  expect(onsaved).toHaveBeenCalled();
});

test("Add…: ↓ goes into the list, the arrows walk the ticks, and ↑ from the top comes back", async () => {
  const candidates = [
    { entity_id: "x", name: "one", sub: "" },
    { entity_id: "y", name: "two", sub: "" },
  ];
  view = mount(TfSetPicker, {
    target,
    props: { kind: "repo", setName: "Payout stack", have: new Set<string>(), read: async () => candidates, onadd: () => {}, onclose: () => {} },
  });
  await settle();
  const filter = target.querySelector<HTMLInputElement>("input[aria-label='Filter']")!;
  press(filter, "ArrowDown");
  const [first, second] = [...target.querySelectorAll<HTMLInputElement>("input[type='checkbox']")];
  expect(document.activeElement).toBe(first);
  expect(first!.checked, "a tick waits for its own press").toBe(false);
  press(first!, "ArrowDown");
  expect(document.activeElement).toBe(second);
  press(second!, "ArrowUp");
  press(first!, "ArrowUp");
  expect(document.activeElement).toBe(filter);
});

test("Add…: Enter on a row adds what is ticked, and with nothing ticked sends nothing (F-538)", async () => {
  const candidates = [
    { entity_id: "x", name: "one", sub: "" },
    { entity_id: "y", name: "two", sub: "" },
  ];
  const added: string[][] = [];
  view = mount(TfSetPicker, {
    target,
    props: { kind: "repo", setName: "Payout stack", have: new Set<string>(), read: async () => candidates, onadd: (picked: { entity_id: string }[]) => added.push(picked.map((row) => row.entity_id)), onclose: () => {} },
  });
  await settle();
  const [first] = [...target.querySelectorAll<HTMLInputElement>("input[type='checkbox']")];
  press(first!, "Enter");
  expect(added).toEqual([]);
  first!.click();
  await settle();
  press(first!, "Enter");
  expect(added).toEqual([["x"]]);
});
