/**
 * The Connections drawer's own behaviour: the scroll to a group a set's save or
 * apply added (F-550), and the words a held row keeps (F-064's rule for the drawer).
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { createLayers } from "../layers.svelte";
import type { JobGroup } from "./connections";
import TfConnections from "./TfConnections.svelte";

const GROUP: JobGroup = {
  key: "set:payout-stack",
  label: "Payout stack",
  via: "via epic PAY-200",
  href: "#/sets/payout-stack",
  setId: "payout-stack",
  rows: [{ id: "gitea:tidewater/payout-service", title: "tidewater/payout-service", href: null, section: "code", kind: "Repository", state: null }],
};
const NEW_GROUP: JobGroup = { ...GROUP, key: "set:sepa-kit", label: "SEPA payouts kit", href: "#/sets/sepa-kit", setId: "sepa-kit" };

let view: ReturnType<typeof mount> | null = null;
let scrolled: Element[] = [];
const real = Element.prototype.scrollIntoView;

function open() {
  const props = $state({
    jobKey: "PAY-231",
    links: [],
    groups: [GROUP],
    layers: createLayers(),
    onclose: () => {},
    onunlink: () => {},
    onlinkto: () => {},
  });
  view = mount(TfConnections, { target: document.body, props });
  flushSync();
  return props;
}

const actionNamed = (name: string) => [...document.querySelectorAll<HTMLButtonElement>(".tf-dacts button")].find((button) => button.textContent === name)!;

beforeEach(() => {
  scrolled = [];
  Element.prototype.scrollIntoView = vi.fn(function (this: Element) {
    scrolled.push(this);
  });
});

afterEach(() => {
  if (view) unmount(view);
  view = null;
  Element.prototype.scrollIntoView = real;
});

test("the group a save adds comes into view, and focus stays on the opener (F-550)", () => {
  const props = open();
  const opener = actionNamed("Save as a link set…");
  opener.focus();
  opener.click();
  flushSync();
  expect(scrolled).toEqual([]);

  props.groups = [GROUP, NEW_GROUP];
  flushSync();
  expect(scrolled.map((head) => head.textContent)).toEqual([expect.stringContaining("From SEPA payouts kit")]);
  expect(document.activeElement).toBe(opener);
});

test("Apply brings its group into view too (F-550)", () => {
  const props = open();
  actionNamed("Apply a link set…").click();
  flushSync();
  props.groups = [GROUP, NEW_GROUP];
  flushSync();
  expect(scrolled).toHaveLength(1);
});

test("a group that comes back with no Save or Apply pressed leaves the list where it is", () => {
  const props = open();
  props.groups = [GROUP, NEW_GROUP];
  flushSync();
  expect(scrolled).toEqual([]);
});

test("a row a pointer took away keeps its words, with nothing to press", () => {
  const props = open();
  document
    .querySelector<HTMLButtonElement>("button[aria-label='Hide here: tidewater/payout-service']")!
    .dispatchEvent(new MouseEvent("click", { bubbles: true, detail: 1 }));
  props.groups = [{ ...GROUP, rows: [] }];
  flushSync();
  const held = document.querySelector(".tf-lk-held")!;
  expect(held.textContent?.replace(/\s+/g, " ").trim()).toBe("tidewater/payout-service Repository");
  expect(held.querySelector(".tf-lk-name")?.classList.contains("tf-mono")).toBe(true);
  expect(held.querySelectorAll("a, button")).toHaveLength(0);
  expect(held.getAttribute("aria-hidden")).toBe("true");
});
