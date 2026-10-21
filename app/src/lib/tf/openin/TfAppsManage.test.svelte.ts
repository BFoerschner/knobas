/** The Manage applications dialog's intro (#1443; R-8.22 retired the split button). */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, expect, test } from "vitest";

import { createLayers } from "../layers.svelte";
import { createApps } from "./apps.svelte";
import TfAppsManage from "./TfAppsManage.svelte";

let view: ReturnType<typeof mount> | null = null;

afterEach(() => {
  if (view) unmount(view);
  view = null;
  document.querySelectorAll(".tf-modal").forEach((node) => node.remove());
});

test("the intro describes the one Open in… ▾ button and the round default control", async () => {
  const apps = createApps({
    checkoutSettings: () =>
      Promise.resolve({
        clones_root: null,
        git: { status: "found", path: "/usr/bin/git" },
        default_application: "vscode",
        applications: [{ id: "vscode", name: "VS Code", command: "code {path}", file_command: null }],
      }),
    setCheckoutApplications: () => Promise.reject(new Error("unused")),
    openCheckout: () => Promise.resolve([]),
  });
  await apps.ensure();
  view = mount(TfAppsManage, { target: document.body, props: { apps, sample: null, layers: createLayers(), onclose: () => {} } });
  flushSync();
  const dialog = document.querySelector("[role='dialog']")!;
  const intro = dialog.querySelector(".tf-dsub")!.textContent!.replace(/\s+/g, " ").trim();
  expect(intro).toContain("Open in… ▾ lists these, the default first. Pick the default with the round button; ↑ orders the rest.");
  const all = dialog.textContent!;
  expect(all).not.toMatch(/first button/);
  expect(all).not.toMatch(/one click away/);
});
