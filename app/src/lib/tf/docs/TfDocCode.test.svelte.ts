/** A Docs page's block of code, coloured by the highlighter, its find marks kept (ADR-0051). */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { HighlightHandlers, HighlightRequest, RenderPort } from "../../render/port";
import { codeRuns } from "./code-runs";
import TfDocCode from "./TfDocCode.svelte";

/** A port that holds each job until the test answers it. */
function heldPort() {
  const jobs: { request: HighlightRequest; handlers: HighlightHandlers; cancelled: boolean }[] = [];
  const port: RenderPort = {
    highlight(request, handlers) {
      const job = { request, handlers, cancelled: false };
      jobs.push(job);
      return { focus() {}, cancel: () => (job.cancelled = true) };
    },
  };
  return { port, jobs };
}

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;
beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
});
afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
});

const CODE = 'export RETRY=3\necho "retry"';

test("a block with a language draws plain first, then role spans once the port answers", () => {
  const { port, jobs } = heldPort();
  view = mount(TfDocCode, { target, props: { code: CODE, language: "bash", stems: [], render: port } });
  flushSync();
  expect(jobs[0]!.request).toEqual({ text: CODE, lang: "bash" });
  expect(target.querySelector("code")!.textContent).toBe(CODE);
  expect(target.querySelectorAll("[class^=tf-hl-]")).toHaveLength(0);
  jobs[0]!.handlers.onLines(0, [
    [["export", "kw"], [" RETRY", ""], ["=", "op"], ["3", "num"]],
    [["echo", "fn"], [" ", ""], ['"retry"', "str"]],
  ]);
  flushSync();
  expect([...target.querySelectorAll("[class^=tf-hl-]")].map((s) => `${s.className}:${s.textContent}`)).toEqual([
    "tf-hl-kw:export",
    "tf-hl-op:=",
    "tf-hl-num:3",
    "tf-hl-fn:echo",
    'tf-hl-str:"retry"',
  ]);
  expect(target.querySelector("code")!.textContent).toBe(CODE);
});

test("a find mark survives the colours, and a second answer recolours in place", () => {
  const { port, jobs } = heldPort();
  view = mount(TfDocCode, { target, props: { code: CODE, language: "bash", stems: ["ret"], render: port } });
  flushSync();
  expect([...target.querySelectorAll("mark")].map((m) => m.textContent)).toEqual(["RETRY", "retry"]);
  jobs[0]!.handlers.onLines(1, [[["echo", "fn"], [" ", ""], ['"retry"', "str"]]]);
  flushSync();
  expect([...target.querySelectorAll("mark")].map((m) => m.textContent)).toEqual(["RETRY", "retry"]);
  expect(target.querySelector("mark .tf-hl-str")?.textContent).toBe("retry");
  jobs[0]!.handlers.onLines(1, [[['echo "retry"', "com"]]]);
  flushSync();
  expect(target.querySelector("mark .tf-hl-com")?.textContent).toBe("retry");
});

test("a block with no language starts no job; one that goes cancels its job", () => {
  const { port, jobs } = heldPort();
  view = mount(TfDocCode, { target, props: { code: CODE, language: null, stems: [], render: port } });
  flushSync();
  expect(jobs).toHaveLength(0);
  unmount(view);
  view = mount(TfDocCode, { target, props: { code: CODE, language: "bash", stems: [], render: port } });
  flushSync();
  expect(jobs).toHaveLength(1);
  unmount(view);
  flushSync();
  view = null;
  expect(jobs[0]!.cancelled).toBe(true);
});

test("tokens that do not add up to the line are left out", () => {
  expect(codeRuns("abc", [["xyz", "kw"]], [])).toEqual([{ text: "abc", kind: "", hit: false }]);
  expect(codeRuns("", undefined, [])).toEqual([]);
});
