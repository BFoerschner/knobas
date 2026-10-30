import { expect, test } from "vitest";

import type { DiffFile, PipeRun } from "../../ipc/entity";
import { pipedDiff, pipelineLabel } from "./piped";

const RAW: DiffFile = {
  path: "fixtures/webhooks.json.b64",
  old_path: null,
  change: "modified",
  additions: 1,
  deletions: 1,
  hunks: [{ header: "@@ -1 +1 @@", lines: [{ kind: "del", old: 1, new: null, text: "eyJhIjogMX0=" }, { kind: "add", old: null, new: 1, text: "eyJhIjogMn0=" }] }],
  truncated: false,
  threads: [],
};

const done = (text: string): PipeRun => ({ outcome: "done", name: "webhooks.json", output: { kind: "text", text } });

test("both sides' outputs diff line by line, under the file's own path", () => {
  const got = pipedDiff(RAW, done('{\n  "a": 1,\n  "b": 2\n}\n'), done('{\n  "a": 1,\n  "b": 3\n}\n'));
  expect(got.kind).toBe("diff");
  if (got.kind !== "diff") return;
  expect(got.file.path).toBe("fixtures/webhooks.json.b64");
  expect(got.name).toBe("webhooks.json");
  expect(got.file.change).toBe("modified");
  expect(got.file.hunks.flatMap((h) => h.lines.map((l) => `${l.kind} ${l.text}`))).toEqual([
    'context {',
    'context   "a": 1,',
    'del   "b": 2',
    'add   "b": 3',
    "context }",
  ]);
});

test("an added file has no old side: every output line is added", () => {
  const got = pipedDiff({ ...RAW, change: "added" }, null, done("x\ny"));
  expect(got.kind === "diff" && got.file.hunks[0]!.lines.map((l) => l.kind)).toEqual(["add", "add"]);
  expect(got.kind === "diff" && got.file.change).toBe("added");
});

test("a failed step on one side names that side, the step and its error", () => {
  const failed: PipeRun = {
    outcome: "failed",
    failure: { index: 0, name: "base64 decode", why: "error", error: "invalid symbol 33", input: { size: 4, head: "!!!!", binary: false } },
  };
  expect(pipedDiff(RAW, failed, done("{}"))).toEqual({ kind: "failed", side: "old", why: "at step 1 (base64 decode): invalid symbol 33" });
});

test("a side whose output is bytes is not diffed as text", () => {
  const bytes: PipeRun = { outcome: "done", name: "x.bin", output: { kind: "bytes", base64: "AA==", size: 1 } };
  expect(pipedDiff(RAW, done("{}"), bytes)).toEqual({ kind: "failed", side: "new", why: "where its output is 1 B of bytes, not text" });
});

test("a pipeline goes by its name, else by its steps", () => {
  expect(pipelineLabel({ name: "webhook payload", pattern: "*.json.b64", steps: [{ step: "base64_decode" }] })).toBe("webhook payload");
  expect(pipelineLabel({ name: null, pattern: null, steps: [{ step: "base64_decode" }, { step: "pretty", format: "json" }] })).toBe(
    "base64 decode → pretty-print JSON",
  );
});
