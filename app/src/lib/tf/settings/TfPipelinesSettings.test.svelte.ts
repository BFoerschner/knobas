/** Settings › Renderers, the pipelines half (#2265): command steps added, named pipelines listed and edited by reference. */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { CommandStep, CommandStepDraft, NamedPipeline, NamedPipelineDraft, RendererDefault } from "../../ipc/app";
import { createRendererDefaults } from "../../render/defaults.svelte";
import { createCommandSteps, createNamedPipelines } from "../../render/pipelines.svelte";
import TfPipelinesSettings from "./TfPipelinesSettings.svelte";

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
});

const GZ: NamedPipeline = { id: "gz-json", name: "Gzipped JSON", steps: [{ step: "gunzip" }, { step: "jq", filter: "." }] };

function stores(pipes: NamedPipeline[], steps: CommandStep[], rows: RendererDefault[]) {
  const pipeSaves: NamedPipelineDraft[][] = [];
  const stepSaves: CommandStepDraft[][] = [];
  const defaultSaves: RendererDefault[][] = [];
  const pipelines = createNamedPipelines({
    read: async () => pipes,
    save: async (list) => {
      pipeSaves.push(list);
      return list.map((d) => ({ id: d.id ?? d.name.toLowerCase(), name: d.name, steps: d.steps }));
    },
  });
  const stepCommands = createCommandSteps({
    read: async () => steps,
    save: async (list) => {
      stepSaves.push(list);
      return list.map((d) => ({ id: d.id ?? d.name.toLowerCase(), name: d.name, command: d.command, timeout_secs: d.timeout_secs ?? 10 }));
    },
  });
  const defaults = createRendererDefaults({
    read: async () => rows,
    save: async (list) => {
      defaultSaves.push(list);
      return list;
    },
  });
  defaults.hold(rows);
  return { pipelines, stepCommands, defaults, pipeSaves, stepSaves, defaultSaves };
}

async function settle() {
  for (let i = 0; i < 6; i += 1) {
    await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 0));
    flushSync();
  }
}

function type(label: string, value: string) {
  const field = [...target.querySelectorAll<HTMLInputElement>("input")].find((el) => target.querySelector(`label[for="${el.id}"]`)?.textContent?.startsWith(label))!;
  field.value = value;
  field.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
}

const cells = (selector: string) => [...target.querySelectorAll(`${selector} tbody tr`)].map((tr) => [...tr.querySelectorAll("td")].slice(0, -1).map((td) => text(td)));
const text = (el: Element | null) => el?.textContent?.replace(/\s+/g, " ").trim() ?? "";
const button = (name: string) => [...target.querySelectorAll("button")].find((b) => text(b) === name || b.getAttribute("aria-label") === name)!;

test("a command step added here is saved whole with its name, command and timeout", async () => {
  const s = stores([], [], []);
  app = mount(TfPipelinesSettings, { target, props: s });
  await settle();
  expect(target.textContent).toContain("No command steps yet.");
  button("Add a command step…").click();
  flushSync();
  type("Step name", "Upper");
  type("Step command", "tr a-z A-Z");
  type("Step timeout", "4");
  button("Save step").click();
  await settle();
  expect(s.stepSaves).toEqual([[{ id: null, name: "Upper", command: "tr a-z A-Z", timeout_secs: 4 }]]);
  expect(cells(".tf-rnd-steps")).toEqual([["Upper", "tr a-z A-Z", "4 s"]]);
});

test("a named pipeline is listed with the patterns that use it, and an edit keeps its id so they follow", async () => {
  const s = stores([GZ], [], [{ pattern: "*.json.gz", renderer: "", pipeline: "gz-json" }, { pattern: "*.md", renderer: "source", pipeline: null }]);
  app = mount(TfPipelinesSettings, { target, props: s });
  await settle();
  expect(cells(".tf-rnd-pipes")).toEqual([["Gzipped JSON", "gunzip → jq", "*.json.gz"]]);

  button("Edit Gzipped JSON").click();
  flushSync();
  button("Remove jq").click();
  flushSync();
  button("Save pipeline").click();
  await settle();
  expect(s.pipeSaves).toEqual([[{ id: "gz-json", name: "Gzipped JSON", steps: [{ step: "gunzip" }] }]]);
  expect(s.defaultSaves).toEqual([]);
  expect(s.pipelines.list[0]?.steps).toEqual([{ step: "gunzip" }]);
});
