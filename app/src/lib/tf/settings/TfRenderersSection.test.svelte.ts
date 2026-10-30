/** Settings › Renderers (#2262): add a command renderer, see it in the registry, and try it in the test pane. */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { RenderContent, UserRenderer, UserRendererDraft } from "../../ipc/app";
import { installCommands } from "../../render/command";
import { renderersFor, renderersForFence } from "../../render/registry";
import TfRenderersSection, { type RendererPorts } from "./TfRenderersSection.svelte";

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
  app = mount(TfRenderersSection, { target, props: { ports } });
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
    [{ id: null, name: "Cat", command: "cat", patterns: ["*.page", "docs/*.htm"], fences: ["Cat"], input: "text", output: "html", timeout_secs: 3 }],
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

test("Remove saves the list without it, and the registry drops it", async () => {
  const cat: UserRenderer = { id: "cat", name: "Cat", command: "cat", patterns: ["*.page"], fences: [], input: "text", output: "html", timeout_secs: 10 };
  const { ports, sent } = backend([cat]);
  app = mount(TfRenderersSection, { target, props: { ports } });
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
  app = mount(TfRenderersSection, { target, props: { ports } });
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
