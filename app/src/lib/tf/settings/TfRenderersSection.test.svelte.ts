/** Settings › Renderers (#2262): add a command renderer, see it in the registry, and try it in the test pane. */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { RenderContent, UserRenderer, UserRendererDraft } from "../../ipc/app";
import { installCommands } from "../../render/command";
import { renderersFor, renderersForFence } from "../../render/registry";
import TfRenderersSection, { type RendererPorts } from "./TfRenderersSection.svelte";
import type { RendererDefault } from "../../render/defaults";
import { createRendererDefaults } from "../../render/defaults.svelte";
import { createDiffPipelines } from "../diff/diff-pipelines.svelte";

/** A `render.defaults` row that answers what it was last sent (#2263). */
function storedDefaults(start: RendererDefault[] = []) {
  let row = start;
  const saves: RendererDefault[][] = [];
  const store = createRendererDefaults({
    read: async () => row,
    save: async (list) => {
      if (list.some((d, i) => list.findIndex((o) => o.pattern.toLowerCase() === d.pattern.toLowerCase()) !== i))
        throw { code: "invalid", message: "a pattern has a default already" };
      saves.push(list);
      row = list;
      return list;
    },
  });
  return { store, saves };
}

let target: HTMLElement;
let app: ReturnType<typeof mount> | null = null;
beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
});
afterEach(() => {
  if (app) unmount(app);
  app = null;
  target.remove();
  installCommands([]);
});

/** A backend that stores what it is sent, as `set_user_renderers` does, and runs `cat`. */
function backend(start: UserRenderer[] = []) {
  let stored = start;
  const sent: UserRendererDraft[][] = [];
  const ran: [string, RenderContent][] = [];
  const ports: RendererPorts = {
    userRenderers: async () => stored,
    setUserRenderers: async (drafts) => {
      sent.push(drafts);
      stored = drafts.map((d) => ({ ...d, id: d.id ?? d.name.toLowerCase(), fences: d.fences.map((f) => f.toLowerCase()), timeout_secs: d.timeout_secs ?? 10 }));
      return stored;
    },
    commands: {
      run: async (id, content) => {
        ran.push([id, content]);
        return { outcome: "drawn", output: "html", body: content.kind === "text" ? content.text : "" };
      },
    },
  };
  return { ports, sent, ran };
}

async function settle() {
  for (let i = 0; i < 6; i += 1) {
    await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 0));
    flushSync();
  }
}

function type(label: string, value: string) {
  const field = [...target.querySelectorAll<HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement>("input, select, textarea")].find(
    (el) => target.querySelector(`label[for="${el.id}"]`)?.textContent?.startsWith(label),
  )!;
  field.value = value;
  field.dispatchEvent(new Event(field instanceof HTMLSelectElement ? "change" : "input", { bubbles: true }));
  flushSync();
}

const button = (name: string) => [...target.querySelectorAll("button")].find((b) => b.textContent?.trim() === name)!;

test("a renderer added here is saved whole, in the registry at once, and runs in the test pane", async () => {
  const { ports, sent, ran } = backend();
  app = mount(TfRenderersSection, { target, props: { ports, defaults: storedDefaults().store } });
  await settle();
  expect(target.textContent).toContain("No command renderers yet.");

  button("Add a renderer…").click();
  flushSync();
  type("Name", "Cat");
  type("Command", "cat");
  type("File patterns", "*.page, docs/*.htm");
  type("Markdown fence languages", "Cat");
  type("Writes", "html");
  type("Timeout", "3");
  button("Save").click();
  await settle();

  expect(sent).toEqual([
    [{ id: null, name: "Cat", command: "cat", patterns: ["*.page", "docs/*.htm"], fences: ["Cat"], input: "text", output: "html", timeout_secs: 3, diff: false }],
  ]);
  expect(renderersFor({ path: "a.page", binary: false }).map((r) => r.id)).toEqual(["command:cat", "source"]);
  expect(renderersForFence("cat").map((r) => r.id)).toEqual(["command:cat", "source"]);
  expect(target.querySelector("tbody")?.textContent).toContain("cat");

  type("Sample", "<b>bold</b><script>alert(1)</script>");
  button("Run").click();
  await settle();

  expect(ran).toEqual([["cat", { kind: "text", text: "<b>bold</b><script>alert(1)</script>" }]]);
  const frame = target.querySelector(".tf-stg-test iframe")!;
  expect(frame.getAttribute("sandbox")).toBe("");
  expect(frame.getAttribute("srcdoc")).toContain("<b>bold</b>");
  expect(frame.getAttribute("srcdoc")).not.toMatch(/<script/i);
});

test("a renderer that draws diffs says so in its form, and an edit keeps it (#2279)", async () => {
  const { ports, sent } = backend();
  app = mount(TfRenderersSection, { target, props: { ports, defaults: storedDefaults().store } });
  await settle();

  button("Add a renderer…").click();
  flushSync();
  type("Name", "Cat");
  type("Command", "cat");
  type("File patterns", "*.page");
  const box = () => [...target.querySelectorAll<HTMLInputElement>("input[type=checkbox]")].find((el) => target.querySelector(`label[for="${el.id}"]`)?.textContent?.startsWith("Diffs"))!;
  expect(box().checked).toBe(false);
  box().click();
  flushSync();
  button("Save").click();
  await settle();
  expect(sent.at(-1)?.[0]?.diff).toBe(true);
  expect(renderersFor({ path: "a.page", binary: false })[0]?.diff).toBe(true);

  button("Edit").click();
  flushSync();
  expect(box().checked).toBe(true);
  button("Save").click();
  await settle();
  expect(sent.at(-1)?.[0]?.diff).toBe(true);
});

test("Remove saves the list without it, and the registry drops it", async () => {
  const cat: UserRenderer = { id: "cat", name: "Cat", command: "cat", patterns: ["*.page"], fences: [], input: "text", output: "html", timeout_secs: 10, diff: false };
  const { ports, sent } = backend([cat]);
  app = mount(TfRenderersSection, { target, props: { ports, defaults: storedDefaults().store } });
  await settle();
  expect(renderersFor({ path: "a.page", binary: false }).map((r) => r.id)).toEqual(["command:cat", "source"]);

  button("Remove").click();
  await settle();

  expect(sent).toEqual([[]]);
  expect(renderersFor({ path: "a.page", binary: false }).map((r) => r.id)).toEqual(["source"]);
});

test("a command the backend refuses is said beside the form, which stays open", async () => {
  const { ports } = backend();
  ports.setUserRenderers = () => Promise.reject({ code: "invalid", message: "Bad's command cannot be run: the command has a quote that is never closed" });
  app = mount(TfRenderersSection, { target, props: { ports, defaults: storedDefaults().store } });
  await settle();

  button("Add a renderer…").click();
  flushSync();
  type("Name", "Bad");
  type("Command", "cat 'x");
  button("Save").click();
  await settle();

  expect(target.querySelector("[role=alert]")?.textContent).toContain("never closed");
  expect(target.querySelector("#tf-rnd-name")).not.toBeNull();
});

// -- Default renderers (#2263) ---------------------------------------------------

const defaultRows = () =>
  [...target.querySelectorAll(".tf-rnd-defaults tbody tr")].map((row) =>
    [row.querySelector(".tf-mono")?.textContent, row.querySelector<HTMLSelectElement>("select[aria-label$=' with']")?.value, row.querySelector(".tf-dim")?.textContent ?? ""].join(" | "),
  );

test("the default renderers are listed, changed and removed here, each change saved whole", async () => {
  const { ports } = backend();
  const { store, saves } = storedDefaults([
    { pattern: "*.md", renderer: "source", pipeline: null },
    { pattern: "docs/*.md", renderer: "markdown", pipeline: null },
  ]);
  app = mount(TfRenderersSection, { target, props: { ports, defaults: store } });
  await settle();
  expect(defaultRows()).toEqual(["*.md | source | ", "docs/*.md | markdown | "]);

  const pick = target.querySelector<HTMLSelectElement>("select[aria-label='Open *.md with']")!;
  pick.value = "markdown";
  pick.dispatchEvent(new Event("change", { bubbles: true }));
  await settle();
  expect(saves.at(-1)).toEqual([
    { pattern: "*.md", renderer: "markdown", pipeline: null },
    { pattern: "docs/*.md", renderer: "markdown", pipeline: null },
  ]);

  target.querySelectorAll<HTMLButtonElement>(".tf-rnd-defaults button")[1]!.click();
  await settle();
  expect(saves.at(-1)).toEqual([{ pattern: "*.md", renderer: "markdown", pipeline: null }]);
  expect(defaultRows()).toEqual(["*.md | markdown | "]);
});

test("a default is added from a pattern and a renderer; one the backend refuses is said", async () => {
  const { ports } = backend();
  const { store, saves } = storedDefaults([{ pattern: "*.md", renderer: "source", pipeline: null }]);
  app = mount(TfRenderersSection, { target, props: { ports, defaults: store } });
  await settle();

  type("Add a default", "*.mmd");
  const open = target.querySelector<HTMLSelectElement>("select[aria-label='Open with']")!;
  open.value = "source";
  open.dispatchEvent(new Event("change", { bubbles: true }));
  button("Add").click();
  await settle();
  expect(saves.at(-1)).toEqual([
    { pattern: "*.md", renderer: "source", pipeline: null },
    { pattern: "*.mmd", renderer: "source", pipeline: null },
  ]);
  expect(defaultRows()).toEqual(["*.md | source | ", "*.mmd | source | "]);
});

test("a default naming a renderer that was deleted says the built-in default applies", async () => {
  const { ports } = backend();
  const { store } = storedDefaults([{ pattern: "*.puml", renderer: "command:plantuml", pipeline: null }]);
  app = mount(TfRenderersSection, { target, props: { ports, defaults: store } });
  await settle();
  expect(defaultRows()).toEqual(["*.puml | command:plantuml | Not installed: the built-in default applies."]);
});

test("Run pipelines on diffs is on until turned off here, and stored (#2270, Standards 6)", async () => {
  const { ports } = backend();
  const saved: boolean[] = [];
  const piping = createDiffPipelines({ read: async () => true, save: async (on) => (saved.push(on), on) });
  app = mount(TfRenderersSection, { target, props: { ports, defaults: storedDefaults().store, piping } });
  await settle();
  const box = target.querySelector<HTMLInputElement>("#tf-rnd-pipes")!;
  expect(target.querySelector('label[for="tf-rnd-pipes"]')?.textContent).toMatch(/^Run pipelines on diffs/);
  expect(box.checked).toBe(true);
  box.click();
  await settle();
  expect(saved).toEqual([false]);
  expect(piping.on).toBe(false);
});

test("a refused save of Run pipelines on diffs puts the box back (Standards 6)", async () => {
  const { ports } = backend();
  const piping = createDiffPipelines({ read: async () => true, save: () => Promise.reject(new Error("the database is gone")) });
  app = mount(TfRenderersSection, { target, props: { ports, defaults: storedDefaults().store, piping } });
  await settle();
  const box = target.querySelector<HTMLInputElement>("#tf-rnd-pipes")!;
  box.click();
  await settle();
  expect(piping.on).toBe(true);
  expect(box.checked).toBe(true);
});
