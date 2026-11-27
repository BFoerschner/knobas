/** Settings › Renderers, the pipelines half (#2265): command steps added, named pipelines listed and edited by reference. */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

/** The schemas kept, the files downloaded, and what was asked: as the backend keeps them. */
const schemas = vi.hoisted(() => ({ pointed: [] as unknown[], files: new Set<string>(), fetched: [] as string[], next: "" }));
vi.mock("../../ipc/api", async (original) => ({
  ...(await original<typeof import("../../ipc/api")>()),
  xmlCheck: async () => ({ on: true }),
  xsdLocations: async () => ({ pointed: structuredClone(schemas.pointed) }),
  // As IPC carries it: JSON, which a page's state proxies pass through.
  xsdSetLocations: async (list: { pointed: unknown[] }) => ((schemas.pointed = JSON.parse(JSON.stringify(list.pointed))), { pointed: structuredClone(schemas.pointed) }),
  xsdFetch: async (url: string) => (schemas.fetched.push(url), schemas.files.add(schemas.next), { url, sha256: schemas.next, size: 1, at: "2026-11-24" }),
  xsdForget: async (sha: string) => void schemas.files.delete(sha),
}));

import type { CommandStep, CommandStepDraft, NamedPipeline, NamedPipelineDraft, RendererDefault } from "../../ipc/app";
import { createRendererDefaults } from "../../render/defaults.svelte";
import { createCommandSteps, createNamedPipelines } from "../../render/pipelines.svelte";
import TfPipelinesSettings from "./TfPipelinesSettings.svelte";

let target: HTMLElement;
let app: ReturnType<typeof mount> | null = null;
beforeEach(() => {
  schemas.pointed = [];
  schemas.files.clear();
  schemas.fetched = [];
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
const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, " ").trim() ?? "";
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
  button("Add step").click();
  await settle();
  expect(s.stepSaves).toEqual([[{ id: null, name: "Upper", command: "tr a-z A-Z", timeout_secs: 4 }]]);
  expect(cells(".tf-rnd-steps")).toEqual([["Upper", "tr a-z A-Z", "4s"]]);
});

test("a pipeline's Add a command step opens the step's dialog over it, and the new step joins the pipeline", async () => {
  const held = stores([], [], []);
  app = mount(TfPipelinesSettings, { target, props: { pipelines: held.pipelines, stepCommands: held.stepCommands, defaults: held.defaults } });
  await new Promise((r) => setTimeout(r));
  flushSync();
  const named = (text: string, from: ParentNode = document) => [...from.querySelectorAll<HTMLElement>("button, a")].find((b) => b.textContent?.trim().startsWith(text))!;
  const titles = () => [...document.querySelectorAll(".tf-dialog h2")].map((h) => h.textContent);
  named("Add a pipeline…").click();
  flushSync();
  named("Add step…").click();
  flushSync();
  const item = named("Add a command step…", document.querySelector(".tf-menu")!);
  expect(item.tagName, "not a link to the page it is on").toBe("BUTTON");
  item.click();
  flushSync();
  expect(titles()).toEqual(["Add a pipeline", "Add a command step"]);
  const field = (id: string, value: string) => {
    const input = document.querySelector<HTMLInputElement>(id)!;
    input.value = value;
    input.dispatchEvent(new Event("input", { bubbles: true }));
  };
  field("#tf-pst-name", "Decode");
  field("#tf-pst-cmd", "protoc --decode_raw");
  flushSync();
  document.querySelector<HTMLFormElement>("#tf-pst-form")!.requestSubmit();
  await new Promise((r) => setTimeout(r));
  flushSync();
  expect(titles()).toEqual(["Add a pipeline"]);
  expect([...document.querySelectorAll(".tf-dialog .tf-pstep")].map((s) => s.textContent?.trim())).toContain("Decode");
});

test("a named pipeline is listed with the patterns that use it, and an edit keeps its id so they follow", async () => {
  const s = stores([GZ], [], [{ pattern: "*.json.gz", renderer: "", pipeline: "gz-json" }, { pattern: "*.md", renderer: "source", pipeline: null }]);
  app = mount(TfPipelinesSettings, { target, props: s });
  await settle();
  expect(cells(".tf-rnd-pipes")).toEqual([["Gzipped JSON", "gunzip → jq", "*.json.gz"]]);

  button("Edit Gzipped JSON…").click();
  flushSync();
  button("Remove jq").click();
  flushSync();
  button("Save pipeline").click();
  await settle();
  expect(s.pipeSaves).toEqual([[{ id: "gz-json", name: "Gzipped JSON", steps: [{ step: "gunzip" }] }]]);
  expect(s.defaultSaves).toEqual([]);
  expect(s.pipelines.list[0]?.steps).toEqual([{ step: "gunzip" }]);
});

test("a command step's command and a pipeline's steps and patterns keep to one line, each whole in its title (the renderers-columns pass)", async () => {
  const command = String.raw`"C:\Program Files\Some Vendor\bin\orchestra-decode.exe" --profile "C:\Users\some one\default.json"`;
  const step: CommandStep = { id: "decode", name: "Orchestra decode", command, timeout_secs: 10 };
  const pipe: NamedPipeline = { id: "orc", name: "Decode and prettify the orchestra scores", steps: [{ step: "base64_decode" }, { step: "gunzip" }, { step: "command", id: "decode" }, { step: "pretty", format: "json" }] };
  const patterns = ["*.orc.b64", "*.orc.gz", "*.score.b64", "scores/**/*.b64", "docs/*.gz"];
  const s = stores([pipe], [step], patterns.map((pattern) => ({ pattern, renderer: "", pipeline: "orc" })));
  app = mount(TfPipelinesSettings, { target, props: s });
  await settle();

  const steps = target.querySelector(".tf-rnd-steps")!;
  expect(steps.classList).toContain("tf-rnd-tbl");
  expect(steps.querySelector(".tf-wrapall")).toBeNull();
  expect(steps.querySelector(".tf-cmd")?.getAttribute("title")).toBe(command);
  expect(steps.querySelector(".tf-cmd-file")?.textContent).toBe('orchestra-decode.exe"');

  const pipes = target.querySelector(".tf-rnd-pipes")!;
  expect(pipes.classList).toContain("tf-rnd-tbl");
  const [name, chain, used] = [...pipes.querySelectorAll("tbody td")];
  expect(name!.querySelector(".tf-cut")?.getAttribute("title")).toBe(pipe.name);
  expect(chain!.querySelector(".tf-cut")?.getAttribute("title")).toBe("base64 decode → gunzip → Orchestra decode → pretty-print JSON");
  expect(used!.querySelector(".tf-words")?.getAttribute("title")).toBe(patterns.join(" "));
  expect(used!.querySelector(".tf-words-more")?.textContent).toBe("+2");
});

test("Schemas: a namespace, a location and the file that answers it each keep to one line, cut in the middle, whole on hover (#2518)", async () => {
  const location = String.raw`C:\Users\some one\source\repos\folders with spaces in them\schemas\opentrans_2_1.xsd`;
  const namespace = "http://www.opentrans.example/XMLSchema/2.1/with/a/namespace/that/is/long";
  const path = "schemas/vendors/opentrans/a folder with a long name/opentrans_2_1.xsd";
  schemas.pointed = [{ namespace, location, to: { repo: "tidewater/edi-schemas", path } }];
  app = mount(TfPipelinesSettings, { target, props: stores([], [], []) });
  await settle();
  const row = [...target.querySelectorAll(".tf-rnd-schemas tbody tr")][0]!;
  const [ns, at, by] = [...row.querySelectorAll("td")].map((td) => td.querySelector(".tf-path") ?? null);
  expect(ns?.getAttribute("title")).toBe(namespace);
  expect(text(ns?.querySelector(".tf-path-tail"))).toBe("long");
  expect(at?.getAttribute("title")).toBe(location);
  expect(text(at?.querySelector(".tf-path-tail"))).toBe("opentrans_2_1.xsd");
  expect(by?.getAttribute("title")).toBe(`tidewater/edi-schemas: ${path}`);
  expect(text(by?.querySelector(".tf-path-tail"))).toBe("opentrans_2_1.xsd");
});

test("Schemas: Download again keeps the row in its place and drops the old file once nothing uses it; Forget drops a file with its last row", async () => {
  const [one, two] = ["1".repeat(64), "2".repeat(64)];
  const url = "https://schemas.example/b.xsd";
  const by = (sha: string) => ({ url, sha256: sha, at: "2026-11-01" });
  schemas.pointed = [
    { namespace: "urn:a", location: "a.xsd", to: { repo: "r", path: "a.xsd" } },
    { namespace: "urn:b", location: url, to: by(one) },
    { namespace: "urn:c", location: "c.xsd", to: by(one) },
  ];
  schemas.files = new Set([one]);
  schemas.next = two;
  app = mount(TfPipelinesSettings, { target, props: stores([], [], []) });
  await settle();
  const rows = () => [...target.querySelectorAll(".tf-stg-sec tbody tr")].filter((tr) => tr.textContent?.includes("urn:")).map((tr) => text(tr.querySelector("td")));
  expect(rows()).toEqual(["urn:a", "urn:b", "urn:c"]);
  // Downloaded again: the same row, the new file; the old one stays, urn:c still uses it.
  [...target.querySelectorAll<HTMLButtonElement>("button")].filter((b) => text(b) === "Download again")[0]!.click();
  await settle();
  expect(schemas.fetched).toEqual([url]);
  expect(rows()).toEqual(["urn:a", "urn:b", "urn:c"]);
  expect((schemas.pointed[1] as { to: { sha256: string } }).to.sha256).toBe(two);
  expect([...schemas.files].sort()).toEqual([one, two]);
  // urn:c forgotten: the last row that uses the old file, which goes with it.
  button("Forget the file for urn:c").click();
  await settle();
  expect(rows()).toEqual(["urn:a", "urn:b"]);
  expect([...schemas.files]).toEqual([two]);
});

