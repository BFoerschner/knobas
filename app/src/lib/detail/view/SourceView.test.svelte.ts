/**
 * A source view drawn as tabs (#661, ADR-0020): the first section opens, a
 * count sits beside its title, a deferred tab reads its blocks once on first
 * open (rule 3), and every block draws its strings as text (rule 5).
 */
import { flushSync, mount, unmount } from "svelte";
import { beforeEach, expect, test, vi } from "vitest";

import type { SourceView as View, ViewSection } from "../../ipc/entity";

/** Every deferred read, as `[entity, section]`. */
const reads: [string, string | null][] = [];
let answer: (section: string | null) => Promise<View | null> = (section) =>
  Promise.resolve({ sections: [log(section ?? "log", false)] });

vi.mock("../../ipc/entity", () => ({
  entityView: (entityId: string, section: string | null) => {
    reads.push([entityId, section]);
    return answer(section);
  },
}));

const { default: SourceView } = await import("./SourceView.svelte");

function log(id: string, deferred: boolean): ViewSection {
  return {
    id,
    title: "Log",
    count: null,
    deferred,
    blocks: deferred ? [] : [{ type: "log", text: "[12:00] step 3 failed\n<i>exit 1</i>", truncated: true }],
  };
}

const VIEW: View = {
  sections: [
    {
      id: "overview",
      title: "Overview",
      count: null,
      deferred: false,
      blocks: [
        {
          type: "facts",
          facts: [
            { label: "Commit", value: "c90d11a3f5", tone: null },
            { label: "Outcome", value: "failed", tone: "err" },
          ],
        },
        {
          type: "entities",
          title: "Parent",
          rows: [
            {
              entity_id: "gitea:tidewater/payout-service@a41f2c",
              title: "a41f2c8b7d",
              meta: "PAY-231 backoff",
              tone: null,
              chip: "merged",
              url: null,
            },
            { entity_id: null, title: "not mirrored", meta: null, tone: null, chip: null, url: null },
          ],
        },
        { type: "text", text: "PAY-231: jitter\n\n<b>not markup</b>", markdown: false },
        {
          type: "settings",
          title: "Parameters",
          entity: "teamcity:buildType:X",
          write_op: "set_parameter",
          values: [
            { name: "env.RETRIES", value: "5", inherited: true, secret: false, editable: true, description: null },
            { name: "secure.token", value: "", inherited: false, secret: true, editable: false, description: null },
          ],
        },
      ],
    },
    { id: "diff", title: "Diff", count: 2, deferred: false, blocks: [{ type: "diff", files: [], truncated: false }] },
    log("log", true),
  ],
};

function render(view: View = VIEW) {
  const target = document.createElement("div");
  document.body.append(target);
  const navigated: string[] = [];
  const app = mount(SourceView, {
    target,
    props: {
      entityId: "gitea:tidewater/payout-service@c90d11",
      view,
      webUrl: null,
      sourceName: "Tidewater Git",
      onnavigate: (hash: string) => navigated.push(hash),
      onopenurl: () => {},
    },
  });
  flushSync();
  return {
    target,
    navigated,
    tabs: () => [...target.querySelectorAll<HTMLButtonElement>("[role=tab]")],
    panel: () => target.querySelector<HTMLElement>("[role=tabpanel]")!,
    text: () => (target.textContent ?? "").replace(/\s+/g, " "),
    done: () => {
      unmount(app);
      target.remove();
    },
  };
}

beforeEach(() => {
  reads.length = 0;
  answer = (section) => Promise.resolve({ sections: [log(section ?? "log", false)] });
});

test("sections are tabs, the first is open, and a count sits by its title", () => {
  const screen = render();
  expect(screen.tabs().map((tab) => tab.textContent!.replace(/\s+/g, " ").trim())).toEqual([
    "Overview",
    "Diff 2",
    "Log",
  ]);
  expect(screen.tabs().map((tab) => tab.getAttribute("aria-selected"))).toEqual(["true", "false", "false"]);
  expect(screen.panel().textContent).toContain("c90d11a3f5");
  expect(reads).toEqual([]);

  screen.tabs()[1]!.click();
  flushSync();
  expect(screen.tabs()[1]!.getAttribute("aria-selected")).toBe("true");
  expect(screen.panel().querySelector(".df")).not.toBeNull();
  screen.done();
});

test("a deferred tab is read once, on its first opening", async () => {
  const screen = render();
  screen.tabs()[2]!.click();
  flushSync();
  expect(reads).toEqual([["gitea:tidewater/payout-service@c90d11", "log"]]);
  await vi.waitFor(() => expect(screen.panel().textContent).toContain("step 3 failed"));

  screen.tabs()[0]!.click();
  flushSync();
  screen.tabs()[2]!.click();
  flushSync();
  expect(reads, "the blocks are kept, not read again").toHaveLength(1);
  expect(screen.panel().textContent).toContain("<i>exit 1</i>");
  expect(screen.panel().querySelector("i")).toBeNull();
  screen.done();
});

test("a deferred tab that fails says why and can be read again", async () => {
  let fail = true;
  answer = (section) =>
    fail
      ? Promise.reject({ code: "unreachable", message: "Gitea answered 502", source_id: null })
      : Promise.resolve({ sections: [log(section ?? "log", false)] });
  const screen = render();
  screen.tabs()[2]!.click();
  flushSync();
  await vi.waitFor(() => expect(screen.panel().textContent).toContain("Gitea answered 502"));

  fail = false;
  [...screen.panel().querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent!.includes("Try again"))!.click();
  await vi.waitFor(() => expect(screen.panel().textContent).toContain("step 3 failed"));
  expect(reads).toHaveLength(2);
  screen.done();
});

test("facts, rows, text and settings are drawn as text, with tones and addresses", () => {
  const screen = render();
  const panel = screen.panel();
  expect(panel.textContent).toContain("<b>not markup</b>");
  expect(panel.querySelector("b")).toBeNull();

  const failed = [...panel.querySelectorAll(".fb-v")].find((v) => v.textContent === "failed")!;
  expect(failed.classList.contains("fail"), "err is the theme's fail colour").toBe(true);

  // A mirrored row is an address; one the mirror lacks is plain text.
  const open = [...panel.querySelectorAll<HTMLButtonElement>(".eb-row button")];
  expect(open.map((b) => b.textContent!.trim())).toEqual(["a41f2c8b7d"]);
  open[0]!.click();
  expect(screen.navigated).toEqual(["#/entity/gitea:tidewater%2Fpayout-service%40a41f2c"]);
  expect(panel.textContent).toContain("not mirrored");
  expect(panel.textContent).toContain("merged");

  // A secret value is never drawn, and an inherited one says so.
  const settings = panel.querySelector(".sb")!.textContent!.replace(/\s+/g, " ");
  expect(settings).toContain("env.RETRIES");
  expect(settings).toContain("inherited");
  expect(settings).toContain("hidden");
  screen.done();
});

test("arrow keys move between tabs", () => {
  const screen = render();
  screen.tabs()[0]!.focus();
  screen.tabs()[0]!.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true }));
  flushSync();
  expect(screen.tabs()[1]!.getAttribute("aria-selected")).toBe("true");
  expect(document.activeElement).toBe(screen.tabs()[1]);
  screen.done();
});
