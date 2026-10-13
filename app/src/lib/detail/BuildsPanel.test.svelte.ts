/**
 * The builds of the commit a pull request, a branch or a commit stands at
 * (#710): what the mirror holds, in the order the read gave (failing first),
 * each line opening its build, and nothing at all where there is none.
 */
import { flushSync, mount, unmount } from "svelte";
import { expect, test, vi } from "vitest";

import type { EntityRow } from "../ipc/entity";
import { parseHash } from "../shell/router.svelte";
import BuildsPanel from "./BuildsPanel.svelte";

function build(key: string, text: string, tone: "err" | "info" | "ok"): EntityRow {
  return {
    entity_id: `teamcity:${key}`,
    kind: "build",
    source_id: "teamcity",
    title: `Payout / Build #${key}`,
    updated_at: "2026-10-13T09:00:00Z",
    synced_at: "2026-10-13T09:01:00Z",
    path: "Build",
    state: { text, tone },
  };
}

function render(builds: EntityRow[]) {
  const target = document.createElement("div");
  document.body.append(target);
  const onopen = vi.fn<(hash: string) => void>();
  const panel = mount(BuildsPanel, { target, props: { builds, onopen } });
  flushSync();
  return {
    target,
    onopen,
    done: () => {
      unmount(panel);
      target.remove();
    },
  };
}

test("the builds draw in the order they came, each with its state", () => {
  const { target, done } = render([
    build("1187", "FAILURE", "err"),
    build("1188", "running", "info"),
    build("1186", "SUCCESS", "ok"),
  ]);
  const lines = [...target.querySelectorAll("button.row")].map((row) => row.textContent ?? "");
  expect(lines).toHaveLength(3);
  expect(lines[0]).toContain("Payout / Build #1187");
  expect(lines[0]).toContain("FAILURE");
  expect(lines[1]).toContain("running");
  expect(lines[2]).toContain("SUCCESS");
  expect(target.querySelector(".sec-h")?.textContent).toContain("Builds");
  done();
});

test("the header says how many failed, and only when some did", () => {
  const failing = render([
    build("1187", "FAILURE", "err"),
    build("1185", "FAILURE", "err"),
    build("1188", "running", "info"),
    build("1186", "SUCCESS", "ok"),
  ]);
  expect(failing.target.querySelector(".sec-h")?.textContent).toContain("2 failing");
  failing.done();

  const green = render([build("1186", "SUCCESS", "ok")]);
  expect(green.target.querySelector(".sec-h")?.textContent).not.toContain("failing");
  green.done();
});

test("a line opens its build", () => {
  const { target, onopen, done } = render([build("1187", "FAILURE", "err")]);
  target.querySelector<HTMLButtonElement>("button.row")?.click();
  expect(onopen).toHaveBeenCalledOnce();
  const route = parseHash(onopen.mock.calls[0]?.[0] ?? "");
  expect(route).toMatchObject({ detail: { kind: "build", entityId: "teamcity:1187" } });
  done();
});

test("no build draws nothing at all", () => {
  const { target, done } = render([]);
  expect(target.textContent?.trim()).toBe("");
  expect(target.querySelector(".sec")).toBeNull();
  done();
});
