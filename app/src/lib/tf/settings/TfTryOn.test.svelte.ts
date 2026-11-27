/** *Try it on* (#2562): the panel every renderer's, command's and application's settings page shares. */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { TryOn } from "../../ipc/app";
import TfTryOn from "./TfTryOn.svelte";
import type { TrySource } from "./try-on";

let target: HTMLElement;
let app: ReturnType<typeof mount> | null = null;
beforeEach(() => {
  target = document.createElement("div");
  document.body.appendChild(target);
});
afterEach(() => {
  if (app) unmount(app);
  app = null;
  target.remove();
});

async function settle() {
  for (let i = 0; i < 4; i += 1) {
    await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 0));
    flushSync();
  }
}

const button = (name: string) => [...target.querySelectorAll("button")].find((b) => b.textContent?.trim() === name)!;
const source = (words: string) => [...target.querySelectorAll<HTMLButtonElement>(".tf-seg button")].find((b) => b.textContent === words)!;
const why = () => target.querySelector(".tf-dwhy")?.textContent ?? null;

function open(props: { sources?: TrySource[]; why?: string | null; pick?: (kind: "file" | "folder") => Promise<string | null>; ontry?: (on: TryOn) => unknown }) {
  const tried: TryOn[] = [];
  const picks: string[] = [];
  app = mount(TfTryOn, {
    target,
    props: {
      id: "t",
      sources: props.sources ?? ["text", "file", "folder"],
      why: props.why ?? null,
      pick: props.pick ?? (async (kind) => (picks.push(kind), kind === "file" ? "/Users/mara/examples/flow.mmd" : "/Users/mara/examples/flows")),
      ontry: props.ontry ?? ((on) => void tried.push(on)),
    },
  });
  flushSync();
  return { tried, picks };
}

test("pasted text is sent under its name once it has both, and says what it waits for", async () => {
  const { tried } = open({});
  expect(button("Try it").disabled).toBe(true);
  expect(why()).toBe("Paste some text first.");
  const text = target.querySelector<HTMLTextAreaElement>("#t-text")!;
  text.value = "graph LR\n  a --> b";
  text.dispatchEvent(new Event("input", { bubbles: true }));
  const name = target.querySelector<HTMLInputElement>("#t-name")!;
  name.value = " flow.mmd ";
  name.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  expect(why()).toBeNull();
  button("Try it").click();
  await settle();
  expect(tried).toEqual([{ kind: "text", name: "flow.mmd", text: "graph LR\n  a --> b" }]);
});

test("a file and a folder are chosen with the picker, and only the path is sent on the press", async () => {
  const { tried, picks } = open({});
  source("A file").click();
  flushSync();
  expect(target.querySelector(".tf-try-path")?.textContent).toBe("Nothing chosen");
  expect(why()).toBe("Choose a file first.");
  button("Choose a file…").click();
  await settle();
  expect(picks).toEqual(["file"]);
  expect(target.querySelector(".tf-try-path")?.textContent).toBe("/Users/mara/examples/flow.mmd");
  expect(tried, "choosing tries nothing").toEqual([]);
  button("Try it").click();
  await settle();
  expect(tried).toEqual([{ kind: "path", path: "/Users/mara/examples/flow.mmd" }]);
  unmount(app!);
  const folder = open({});
  source("A folder").click();
  flushSync();
  button("Choose a folder…").click();
  await settle();
  button("Try it").click();
  await settle();
  expect(folder.picks).toEqual(["folder"]);
  expect(folder.tried).toEqual([{ kind: "path", path: "/Users/mara/examples/flows" }]);
});

test("a picker closed with nothing chosen leaves nothing chosen", async () => {
  open({ pick: async () => null });
  source("A folder").click();
  flushSync();
  button("Choose a folder…").click();
  await settle();
  expect(target.querySelector(".tf-try-path")?.textContent).toBe("Nothing chosen");
  expect(button("Try it").disabled).toBe(true);
});

test("the page's reason comes first, and a refusal is said under the panel", async () => {
  open({ why: "Name it first." });
  expect(why()).toBe("Name it first.");
  unmount(app!);
  open({
    sources: ["folder"],
    pick: async () => "/Users/mara/flows",
    ontry: () => Promise.reject({ code: "invalid", message: "lint runs on a file: choose a file or paste text" }),
  });
  expect(target.querySelector(".tf-seg"), "one source needs no choice").toBeNull();
  button("Choose a folder…").click();
  await settle();
  button("Try it").click();
  await settle();
  expect(target.querySelector("[role=alert]")?.textContent).toBe("lint runs on a file: choose a file or paste text");
});

test("a repository's rows are the page's own, with their own Try it", () => {
  open({ sources: ["repo", "text"] });
  expect(source("A repository").getAttribute("aria-pressed")).toBe("true");
  expect([...target.querySelectorAll("button")].some((b) => b.textContent?.trim() === "Try it")).toBe(false);
});
