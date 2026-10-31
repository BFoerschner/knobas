/**
 * Pipelines on diffs (#2270), mounted as a caller mounts the diff: a stand-in
 * `piped` reader that runs a real base64 decode and JSON pretty-print on the
 * side it is asked for and records each ask, a stand-in default pipeline for
 * `*.json.b64` (the default-pipeline store is #2265's), and the setting over
 * a storage of its own.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import { createPendingComments } from "../../detail/view/pending-comments.svelte";
import type { DiffFile, PipeRun, PipeStep } from "../../ipc/entity";
import { createDiffLayout } from "./diff-layout.svelte";
import { createDiffPipelines } from "./diff-pipelines.svelte";
import type { DiffPipelines } from "./piped";
import TfDiffView from "./TfDiffView.svelte";

const OLD_JSON = '{"url":"ledger.internal/hook","timeout_ms":5000}';
const NEW_JSON = '{"url":"ledger.internal/hook","timeout_ms":2000}';

const B64: DiffFile = {
  path: "fixtures/webhooks.json.b64",
  old_path: null,
  change: "modified",
  additions: 1,
  deletions: 1,
  hunks: [{ header: "@@ -1 +1 @@", lines: [{ kind: "del", old: 1, new: null, text: btoa(OLD_JSON) }, { kind: "add", old: null, new: 1, text: btoa(NEW_JSON) }] }],
  truncated: false,
  threads: [],
};

const STEPS: PipeStep[] = [{ step: "base64_decode" }, { step: "pretty", format: "json" }];
const defaults: DiffPipelines = { of: (path) => (path.endsWith(".json.b64") ? { name: null, pattern: "*.json.b64", steps: STEPS } : null) };

/** A `piped` reader: base64 decode and pretty JSON over each side's text, or `fail` on the side it names. */
function reader(fail: "old" | "new" | null = null) {
  const asked: string[] = [];
  const context = {
    text: async () => null,
    async piped(file: DiffFile, side: "old" | "new", steps: PipeStep[]): Promise<PipeRun> {
      asked.push(`${side}:${file.path}:${steps.map((s) => s.step).join(",")}`);
      if (side === fail)
        return {
          outcome: "failed",
          failure: { index: 0, name: "base64 decode", why: "error", error: "invalid symbol 33, offset 0", input: { size: 4, head: "!!!!", binary: false } },
        };
      const json = side === "old" ? OLD_JSON : NEW_JSON;
      return { outcome: "done", name: "webhooks.json", output: { kind: "text", text: `${JSON.stringify(JSON.parse(json), null, 2)}\n` } };
    },
  };
  return { asked, context };
}

/** The setting over a backend row that answers what it stored. */
function storedSetting() {
  return createDiffPipelines({ read: async () => true, save: async (on) => on });
}

let target: HTMLElement;
let views: ReturnType<typeof mount>[] = [];
beforeEach(() => {
  target = document.createElement("div");
  document.body.appendChild(target);
});
afterEach(() => {
  views.forEach((view) => unmount(view));
  views = [];
  target.remove();
});

function draw(props: Record<string, unknown>) {
  views.push(mount(TfDiffView, { target, props: { layout: createDiffLayout(() => null), pipelines: defaults, ...props } as never }));
  flushSync();
}
const settle = async () => {
  for (let n = 0; n < 8; n += 1) await Promise.resolve();
  flushSync();
};
const squash = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, " ").trim() ?? "";
const lines = () => [...target.querySelectorAll(".tf-dbody > .tf-ln")].map(squash);
const header = () => target.querySelector(".tf-fh")!;
const button = (name: string) => [...target.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent?.trim() === name);

test("a changed base64 JSON file with a default pipeline diffs as decoded JSON, and the header names the pipeline", async () => {
  const { asked, context } = reader();
  draw({ files: [B64], context, piping: storedSetting() });
  await settle();
  expect(asked).toEqual(["old:fixtures/webhooks.json.b64:base64_decode,pretty", "new:fixtures/webhooks.json.b64:base64_decode,pretty"]);
  expect(squash(header().querySelector(".tf-ran"))).toBe("base64 decode → pretty-print JSON");
  expect(squash(header().querySelector(".tf-ran-why"))).toBe("default for *.json.b64");
  expect(lines()).toEqual(["11 {", '22 "url": "ledger.internal/hook",', '3− "timeout_ms": 5000', '3+ "timeout_ms": 2000', "44 }"]);
  // The header's counts are the decoded diff's.
  expect(squash(header().querySelector(".tf-add-n"))).toBe("+1");
});

test("the per-diff toggle shows the raw diff, and turns the pipeline back on", async () => {
  const { context } = reader();
  draw({ files: [B64], context, piping: storedSetting() });
  await settle();
  button("Show raw")!.click();
  flushSync();
  expect(lines()).toEqual([`1− ${btoa(OLD_JSON)}`, `1+ ${btoa(NEW_JSON)}`]);
  expect(header().querySelector(".tf-ran")).toBeNull();
  button("Run pipeline")!.click();
  await settle();
  expect(lines()[2]).toBe('3− "timeout_ms": 5000');
});

test("with the setting off every diff is raw and nothing runs, and the per-diff toggle still turns a pipeline on", async () => {
  const { asked, context } = reader();
  const piping = storedSetting();
  piping.hold(false);
  draw({ files: [B64], context, piping });
  await settle();
  expect(asked).toEqual([]);
  expect(lines()).toEqual([`1− ${btoa(OLD_JSON)}`, `1+ ${btoa(NEW_JSON)}`]);
  expect(target.querySelector(".tf-dtool button.tf-pill")?.getAttribute("aria-pressed")).toBe("false");
  button("Run pipeline")!.click();
  await settle();
  expect(asked).toHaveLength(2);
  expect(lines()[3]).toBe('3+ "timeout_ms": 2000');
});

test("the toolbar's Run pipelines turns every file's pipeline on or off for this diff", async () => {
  const { context } = reader();
  draw({ files: [B64], context, piping: storedSetting() });
  await settle();
  const pill = target.querySelector<HTMLButtonElement>(".tf-dtool button.tf-pill")!;
  expect(squash(pill)).toBe("Run pipelines");
  expect(pill.getAttribute("aria-pressed")).toBe("true");
  pill.click();
  flushSync();
  expect(pill.getAttribute("aria-pressed")).toBe("false");
  expect(lines()).toEqual([`1− ${btoa(OLD_JSON)}`, `1+ ${btoa(NEW_JSON)}`]);
});

test("a pipeline that fails on one side shows the raw diff and the step's error", async () => {
  const { context } = reader("old");
  draw({ files: [B64], context, piping: storedSetting() });
  await settle();
  expect(lines()).toEqual([`1− ${btoa(OLD_JSON)}`, `1+ ${btoa(NEW_JSON)}`]);
  expect(squash(target.querySelector(".tf-diff .tf-banner.tf-err"))).toBe(
    "The pipeline failed on the file before the change, at step 1 (base64 decode): invalid symbol 33, offset 0. This is the raw diff.",
  );
});

test("a file with no default pipeline, or a diff whose caller runs none, is drawn raw with no toggle", async () => {
  const { asked, context } = reader();
  draw({ files: [{ ...B64, path: "src/plain.txt" }], context, piping: storedSetting() });
  await settle();
  expect(asked).toEqual([]);
  expect(target.querySelector(".tf-dtool button.tf-pill")).toBeNull();
  expect(button("Show raw")).toBeUndefined();
});

test("a piped diff takes no comment: no +, no box from c, and the raw file's thread and draft are counted, not drawn on the piped lines", async () => {
  const { context } = reader();
  const pending = createPendingComments(() => null);
  pending.add("pr", { path: B64.path, side: "new", line: 1, body: "why 2000?" });
  const thread = { side: "new" as const, line: 1, outdated: false, comments: [{ author: "ana", body: "too short", at: null }], id: "t1", replies: true, resolved_by: null };
  draw({ files: [{ ...B64, threads: [thread] } as DiffFile], context, piping: storedSetting(), review: { entity: "pr", pending } });
  // While the runs are out the raw lines are drawn, comments and all: nothing is hidden yet.
  expect(target.querySelector(".tf-ran-hidden")).toBeNull();
  expect(target.querySelector(".tf-thread")).not.toBeNull();
  await settle();
  expect(lines()[3]).toBe('3+ "timeout_ms": 2000');
  expect(target.querySelector("button.tf-addc")).toBeNull();
  expect(target.querySelector(".tf-thread")).toBeNull();
  expect(target.querySelector(".tf-pending")).toBeNull();
  expect(squash(target.querySelector(".tf-ran-hidden"))).toBe("2 comments are on the raw file's lines, not the pipeline's output. Show raw to read them or to comment.");
  document.dispatchEvent(new KeyboardEvent("keydown", { key: "c", bubbles: true }));
  flushSync();
  expect(target.querySelector(".tf-newc")).toBeNull();
  // Raw again: the thread, the draft and the + are back on the file's own lines.
  button("Show raw")!.click();
  flushSync();
  expect(target.querySelector(".tf-ran-hidden")).toBeNull();
  expect(target.querySelectorAll("button.tf-addc").length).toBeGreaterThan(0);
  expect(target.querySelector(".tf-thread")).not.toBeNull();
  expect(target.querySelector(".tf-pending")).not.toBeNull();
});

test("two outputs too long to match line by line still diff, and the card says how they are drawn", async () => {
  const many = (tag: string) => Array.from({ length: 2_100 }, (_, n) => `"${tag}${n}"`).join(",\n");
  const context = {
    text: async () => null,
    piped: async (_file: DiffFile, side: "old" | "new"): Promise<PipeRun> => ({
      outcome: "done",
      name: "webhooks.json",
      output: { kind: "text", text: `[\n${many(side === "old" ? "a" : "b")}\n]\n` },
    }),
  };
  draw({ files: [B64], context, piping: storedSetting() });
  await settle();
  expect(squash(target.querySelector(".tf-coarse"))).toBe("Too many changed lines to match one by one: the changed part is drawn removed whole, then added whole.");
  expect(squash(header().querySelector(".tf-add-n"))).toBe("+2100");
  expect(squash(header().querySelector(".tf-del-n"))).toBe("−2100");
});
