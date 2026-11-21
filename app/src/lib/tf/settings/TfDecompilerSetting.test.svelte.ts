/** Settings › Renderers › Decompiler (ux19): the Java found and the bundled decompiler, *Look again*, a Java of one's own. */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import TfDecompilerSetting, { type DecompilerPorts } from "./TfDecompilerSetting.svelte";
import type { JavaRuntime } from "../../ipc/app";

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;
beforeEach(() => {
  target = document.createElement("div");
  document.body.appendChild(target);
});
afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
});

const FOUND: JavaRuntime = { java: { path: "/usr/bin/java", version: "21.0.4", release: 21 }, decompiler: "Vineflower 1.12.0", needs: 17, can_decompile: true, reason: null, set_path: null };
const NONE: JavaRuntime = { ...FOUND, java: null, can_decompile: false };
const OLD: JavaRuntime = { ...FOUND, java: { path: "/opt/jdk11/bin/java", version: "11.0.21", release: 11 }, can_decompile: false, reason: "Java 11 found; Decompile needs Java 17", set_path: "/opt/jdk11" };

async function settle() {
  for (let i = 0; i < 5; i += 1) await new Promise((resolve) => setTimeout(resolve, 0));
  flushSync();
}
const text = (sel: string) => target.querySelector(sel)?.textContent?.replace(/\s+/g, " ").trim() ?? "";
const button = (label: string) => [...target.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent?.trim() === label)!;

function draw(answers: JavaRuntime[], calls: string[] = []) {
  let at = 0;
  const next = () => Promise.resolve(answers[Math.min(at++, answers.length - 1)]!);
  const ports: DecompilerPorts = {
    java: (again) => {
      calls.push(`java ${again}`);
      return next();
    },
    setPath: (path) => {
      calls.push(`set ${path}`);
      return next();
    },
  };
  view = mount(TfDecompilerSetting, { target, props: { ports } });
  flushSync();
  return calls;
}

test("it says the Java found and the decompiler it runs, holding the line's room while it looks", async () => {
  const calls = draw([FOUND]);
  expect(text(".tf-dec-line")).toBe("Looking for a Java…");
  await settle();
  expect(text(".tf-dec-line")).toBe("Java 21.0.4 at /usr/bin/java, with Vineflower 1.12.0 (bundled).");
  expect(calls).toEqual(["java false"]);
});

test("with no Java it says the JAR view shows outlines; Look again looks again", async () => {
  const calls = draw([NONE, FOUND]);
  await settle();
  expect(text(".tf-dec-line")).toBe("No Java found on this machine, so the JAR view shows outlines only; Vineflower 1.12.0 (bundled).");
  button("Look again").click();
  await settle();
  expect(calls).toEqual(["java false", "java true"]);
  expect(text(".tf-dec-line")).toContain("Java 21.0.4");
});

test("a Java of one's own is used, shown with why it cannot run; cleared, knobas looks by itself", async () => {
  const calls = draw([FOUND, OLD, FOUND]);
  await settle();
  const box = target.querySelector<HTMLInputElement>("#tf-rnd-java")!;
  box.value = "/opt/jdk11";
  box.dispatchEvent(new Event("input", { bubbles: true }));
  button("Use this Java").click();
  await settle();
  expect(text(".tf-dec-why")).toBe("Java 11 found; Decompile needs Java 17");
  expect(box.value).toBe("/opt/jdk11");
  box.value = "";
  box.dispatchEvent(new Event("input", { bubbles: true }));
  button("Use this Java").click();
  await settle();
  expect(calls).toEqual(["java false", "set /opt/jdk11", "set null"]);
  expect(target.querySelector(".tf-dec-why")).toBeNull();
});

test("a late answer brings nothing above the path row: the line and the reason share one room drawn from the first frame (ux19 gate)", async () => {
  // On the real backend the answer lands after the page draws; the reason came in above the path row and the line
  // stood beside it, moving *The Java to use*, *Use this Java* and *Look again* (2026-11-19).
  let answer!: (rt: JavaRuntime) => void;
  const ports: DecompilerPorts = { java: () => new Promise((resolve) => (answer = resolve)), setPath: () => Promise.resolve(OLD) };
  view = mount(TfDecompilerSetting, { target, props: { ports } });
  flushSync();
  const cell = target.querySelector(".tf-stg-c")!;
  const above = () => [...cell.children].slice(0, [...cell.children].indexOf(target.querySelector(".tf-dec-path")!)).map((e) => e.className);
  expect(cell.classList.contains("tf-dec-c")).toBe(true);
  const before = above();
  expect(before).toEqual(["tf-dec-st"]);
  answer(OLD);
  await settle();
  expect(above()).toEqual(before);
  const room = target.querySelector(".tf-dec-st")!;
  expect(room.querySelector(".tf-dec-line")?.getAttribute("title")).toBe("Java 11.0.21 at /opt/jdk11/bin/java, with Vineflower 1.12.0 (bundled).");
  expect(room.querySelector(".tf-dec-why")?.getAttribute("title")).toBe("Java 11 found; Decompile needs Java 17");
  // A Java found is one line, its path cut in the middle, so the reason has the second (the renderers-columns pass).
  expect(room.querySelector(".tf-dec-line")?.classList.contains("tf-dec-at")).toBe(true);
});

test("a Java's deep path is a path cut in its middle, so java.exe stays in view (the renderers-columns pass)", async () => {
  const path = String.raw`C:\Users\some one\AppData\Local\Programs\Eclipse Adoptium\jdk-21.0.4.7-hotspot\bin\java.exe`;
  draw([{ ...FOUND, java: { ...FOUND.java!, path } }]);
  await settle();
  const line = target.querySelector(".tf-dec-line")!;
  expect(line.classList.contains("tf-dec-at")).toBe(true);
  expect(line.querySelector(".tf-path")?.getAttribute("title")).toBe(`Java 21.0.4 at ${path}, with Vineflower 1.12.0 (bundled).`);
  expect(line.querySelector(".tf-path-tail")?.textContent).toBe("java.exe");
  expect(text(".tf-dec-line")).toBe(`Java 21.0.4 at ${path}, with Vineflower 1.12.0 (bundled).`);
});
