/**
 * The builds of the commit a pull request, a branch or a commit stands at,
 * as one line above the tabs (#732): a count per state, toned only where a
 * build failed or is running, opening in place into the builds themselves,
 * failing first, each opening its build. Nothing at all where there is none.
 */
import { flushSync, mount, unmount } from "svelte";
import { expect, test, vi } from "vitest";

import type { EntityRow, Tone } from "../ipc/entity";
import { parseHash } from "../shell/router.svelte";
import BuildsSummary from "./BuildsSummary.svelte";

function build(key: string, text: string, tone: Tone | null): EntityRow {
  return {
    entity_id: `teamcity:${key}`,
    kind: "build",
    source_id: "teamcity",
    title: `Payout / Build #${key}`,
    updated_at: "2026-10-13T09:00:00Z",
    synced_at: "2026-10-13T09:01:00Z",
    path: "Build",
    state: tone ? { text, tone } : null,
  };
}

function render(builds: EntityRow[]) {
  const target = document.createElement("div");
  document.body.append(target);
  const onopen = vi.fn<(hash: string) => void>();
  const summary = mount(BuildsSummary, { target, props: { builds, onopen } });
  flushSync();
  const line = () => target.querySelector<HTMLButtonElement>("button.bs-line");
  return {
    target,
    onopen,
    line,
    rows: () => [...target.querySelectorAll("button.row")].map((row) => row.textContent ?? ""),
    done: () => {
      unmount(summary);
      target.remove();
    },
  };
}

/** The line's text, whitespace folded. */
function text(el: Element | null): string {
  return (el?.textContent ?? "").replace(/\s+/g, " ").trim();
}

test("the line counts the builds by state, failing first", () => {
  const screen = render([
    build("1187", "FAILURE", "err"),
    build("1188", "running", "info"),
    build("1186", "SUCCESS", "ok"),
    build("1185", "SUCCESS", "ok"),
    build("1184", "SUCCESS", "ok"),
    build("1183", "UNKNOWN", "muted"),
  ]);
  expect(text(screen.line())).toBe("Builds · 1 failing · 1 running · 3 passed · 1 other");
  screen.done();
});

test("only a failing or running count is toned", () => {
  const screen = render([build("1187", "FAILURE", "err"), build("1188", "queued", "info"), build("1186", "SUCCESS", "ok")]);
  const tone = (word: string) =>
    [...(screen.line()?.querySelectorAll(".bs-n") ?? [])].find((n) => n.textContent?.includes(word))?.className ?? "";
  expect(tone("failing")).toContain("fail");
  expect(tone("running")).toContain("link");
  expect(tone("passed")).not.toMatch(/\b(fail|link|ok|amber)\b/);
  screen.done();

  const green = render([build("1186", "SUCCESS", "ok")]);
  expect(text(green.line())).toBe("Builds · 1 passed");
  expect(green.line()?.querySelector(".fail, .link")).toBeNull();
  green.done();
});

test("the list is closed until the line is clicked, and then lists the builds in order", () => {
  const screen = render([build("1187", "FAILURE", "err"), build("1188", "running", "info"), build("1186", "SUCCESS", "ok")]);
  expect(screen.rows()).toHaveLength(0);
  expect(screen.line()?.getAttribute("aria-expanded")).toBe("false");

  screen.line()?.click();
  flushSync();
  expect(screen.line()?.getAttribute("aria-expanded")).toBe("true");
  const rows = screen.rows();
  expect(rows).toHaveLength(3);
  expect(rows[0]).toContain("Payout / Build #1187");
  expect(rows[0]).toContain("FAILURE");
  expect(rows[1]).toContain("running");
  expect(rows[2]).toContain("SUCCESS");

  screen.line()?.click();
  flushSync();
  expect(screen.rows()).toHaveLength(0);
  screen.done();
});

test("a line of the list opens its build", () => {
  const screen = render([build("1187", "FAILURE", "err")]);
  screen.line()?.click();
  flushSync();
  screen.target.querySelector<HTMLButtonElement>("button.row")?.click();
  expect(screen.onopen).toHaveBeenCalledOnce();
  const route = parseHash(screen.onopen.mock.calls[0]?.[0] ?? "");
  expect(route).toMatchObject({ detail: { kind: "build", entityId: "teamcity:1187" } });
  screen.done();
});

test("no build draws nothing at all", () => {
  const screen = render([]);
  expect(screen.target.textContent?.trim()).toBe("");
  expect(screen.line()).toBeNull();
  screen.done();
});
