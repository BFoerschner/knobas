/**
 * Per-source *Link handoff* settings section (ADR-0061, #2530):
 * toggling *Open matching links in knobas*, picking *Known items only* vs *Whole source*,
 * hinting when *Whole source* needs *Open its pages inside knobas*, and installing/removing
 * the Firefox Native Messaging host manifest.
 */
import { flushSync, mount, tick, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import { dismiss, reset as resetToasts, toasts } from "../../shell/toasts.svelte";

import type { LinkHandoffScope, LinkHandoffSetting, NativeHostStatus } from "../../ipc/app";
import TfLinkHandoffSection, { type LinkHandoffPorts } from "./TfLinkHandoffSection.svelte";

let target: HTMLElement;
let view: ReturnType<typeof mount> | undefined;
let saved: Array<{ id: string; enabled: boolean; scope: LinkHandoffScope }>;
let hostCalls: string[];

function hostOf(over: Partial<NativeHostStatus> = {}): NativeHostStatus {
  return {
    installed: false,
    supported: true,
    host_name: "dev.knobas.desktop",
    extension_id: "link-handoff@knobas.dev",
    manifest_path: "/Users/tester/Library/Application Support/Mozilla/NativeMessagingHosts/dev.knobas.desktop.json",
    executable_path: null,
    matches_current: false,
    ...over,
  };
}

function makePorts(
  initial: LinkHandoffSetting = { source: "jira", enabled: false, scope: "known_items" },
  initialHost: NativeHostStatus = hostOf(),
): LinkHandoffPorts {
  let state = { ...initial };
  let host = { ...initialHost };
  return {
    read: () => Promise.resolve({ ...state }),
    save: (id, enabled, scope) => {
      saved.push({ id, enabled, scope });
      state = { source: id, enabled, scope };
      return Promise.resolve({ ...state });
    },
    hostStatus: () => Promise.resolve({ ...host }),
    installHost: () => {
      hostCalls.push("install");
      host = hostOf({ installed: true, matches_current: true, executable_path: "/Applications/knobas.app/Contents/MacOS/knobas" });
      return Promise.resolve({ ...host });
    },
    uninstallHost: () => {
      hostCalls.push("uninstall");
      host = hostOf({ installed: false, matches_current: false, executable_path: null });
      return Promise.resolve({ ...host });
    },
  };
}

beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
  saved = [];
  hostCalls = [];
});

afterEach(() => {
  resetToasts();
  if (view) unmount(view);
  view = undefined;
  target.remove();
  document.body.replaceChildren();
});

const button = (label: string) =>
  [...target.querySelectorAll("button")].find((el) => el.textContent?.trim() === label || el.getAttribute("aria-label") === label);

async function settle() {
  await tick();
  await tick();
  flushSync();
}

test("defaults to disabled with Known items only, toggles on, and switches to Whole source", async () => {
  view = mount(TfLinkHandoffSection, {
    target,
    props: { source: "jira", name: "Tidewater Jira", openInside: false, ports: makePorts() },
  });
  await settle();

  const toggle = button("Open matching links in knobas")!;
  const known = button("Known items only")!;
  const whole = button("Whole source")!;
  expect(toggle.getAttribute("aria-checked")).toBe("false");
  expect(known.getAttribute("aria-pressed")).toBe("true");
  expect(whole.disabled).toBe(true);

  toggle.click();
  await settle();
  expect(saved).toEqual([{ id: "jira", enabled: true, scope: "known_items" }]);
  expect(toggle.getAttribute("aria-checked")).toBe("true");
  expect(whole.disabled).toBe(false);

  whole.click();
  await settle();
  expect(saved).toEqual([
    { id: "jira", enabled: true, scope: "known_items" },
    { id: "jira", enabled: true, scope: "whole_source" },
  ]);
  expect(whole.getAttribute("aria-pressed")).toBe("true");
  // Because openInside is false, the hint reminds the user to turn on the embedded site.
  expect(target.textContent).toContain("Turn on Open its pages inside knobas above");
});

test("installs and removes the Firefox Native Messaging host manifest", async () => {
  view = mount(TfLinkHandoffSection, {
    target,
    props: { source: "jira", name: "Tidewater Jira", openInside: true, ports: makePorts() },
  });
  await settle();

  expect(target.textContent).toContain("Not installed");
  button("Install host")!.click();
  await settle();
  expect(hostCalls).toEqual(["install"]);
  expect(target.textContent).toContain("Installed");

  button("Remove")!.click();
  await settle();
  // The removal rule: the row says so at once, the manifest goes when the toast ends.
  expect(target.textContent).toContain("Not installed");
  expect(hostCalls).toEqual(["install"]);
  const toast = toasts.items.find((t) => t.text === "Firefox Native Messaging host removed.")!;
  expect(toast.action?.label).toBe("Undo");
  dismiss(toast.id);
  await settle();
  expect(hostCalls).toEqual(["install", "uninstall"]);
  expect(target.textContent).toContain("Not installed");
});

test("Undo on the removal's toast keeps the Firefox host and never removes it", async () => {
  view = mount(TfLinkHandoffSection, {
    target,
    props: {
      source: "jira",
      name: "Tidewater Jira",
      openInside: true,
      ports: makePorts(undefined, hostOf({ installed: true, matches_current: true, executable_path: "/Applications/knobas.app/Contents/MacOS/knobas" })),
    },
  });
  await settle();

  button("Remove")!.click();
  await settle();
  expect(target.textContent).toContain("Not installed");
  const toast = toasts.items.find((t) => t.text === "Firefox Native Messaging host removed.")!;
  toast.action!.run();
  dismiss(toast.id);
  await settle();
  expect(hostCalls).toEqual([]);
  expect(button("Remove")).toBeDefined();
  expect(target.textContent).toContain("Installed");
});

/** Nothing moves once drawn: the Firefox host row is there before its status answers, so the rows under it keep their place. */
test("draws the Firefox host row before its status answers", async () => {
  const ports = { ...makePorts(), hostStatus: () => new Promise<NativeHostStatus>(() => {}) };
  view = mount(TfLinkHandoffSection, {
    target,
    props: { source: "jira", name: "Tidewater Jira", openInside: true, ports },
  });
  await settle();

  expect(target.textContent).toContain("Firefox host");
  expect(target.textContent).toContain("Checking…");
});
