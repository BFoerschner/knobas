/**
 * An example opened in the XML view (E1): the schema's place `example/ORDER`. The file header names the example after
 * the schema's path, says how it was made and that it is not saved, and offers only Copy and Save to Downloads.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

const saved = vi.hoisted(() => [] as { name: string; text: string }[]);
vi.mock("../../ipc/api", async (original) => ({
  ...(await original<typeof import("../../ipc/api")>()),
  xsdLocations: async () => ({ pointed: [] }),
  xsdSaveExample: async (name: string, text: string) => (saved.push({ name, text }), `/home/me/Downloads/${name}`),
}));

import type { SourceView } from "../../ipc/entity";
import TfRepoFiles from "./TfRepoFiles.svelte";
import { plainPort } from "../../render/port";
import { createRendererDefaults } from "../../render/defaults.svelte";
import { XS } from "../../render/xsd-xml";
import { forgetHeld } from "../render/xsd/example-held.svelte";

const REPO = "gitea:tidewater/schemas";
const PATH = "schemas/note.xsd";
const SCHEMA = `<xs:schema xmlns:xs="${XS}" targetNamespace="urn:n" xmlns="urn:n" elementFormDefault="qualified">
  <xs:element name="Note"><xs:complexType><xs:sequence><xs:element name="Must" type="xs:string"/></xs:sequence></xs:complexType></xs:element>
</xs:schema>`;

function source(): (at: string, section: string | null) => Promise<SourceView> {
  const name = PATH.slice(PATH.lastIndexOf("/") + 1);
  return async (_at, section) => {
    const blocks =
      section === "files"
        ? [{ type: "tree", trail: [], entries: [{ name, kind: "file", size: SCHEMA.length, section: `file:${PATH}` }], truncated: false, url: null }]
        : [{ type: "file", trail: [], path: PATH, text: SCHEMA, language: null, size: SCHEMA.length, truncated: false, binary: false, url: null, sha: "b10b", commit: "c0mmit" }];
    return { sections: [{ id: section, title: "Files", count: null, deferred: false, blocks }], actions: [] } as unknown as SourceView;
  };
}

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;

beforeEach(() => {
  forgetHeld();
  saved.length = 0;
  target = document.createElement("div");
  document.body.appendChild(target);
  Element.prototype.scrollIntoView = function () {};
});
afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
});

const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, " ").trim() ?? "";
const header = () => target.querySelector(".tf-fileview header");

test("at an example's place the header names the example, says it is not saved, and offers Copy and Save to Downloads only", async () => {
  view = mount(TfRepoFiles, {
    target,
    props: {
      ports: { entityView: source(), submitWrite: async () => Promise.reject(new Error("no write")) },
      repoId: REPO,
      at: REPO,
      branch: "main",
      path: PATH,
      line: null,
      place: "example/Note",
      changed: new Set<string>(),
      treeSections: { dir: "files:", file: "file:" },
      defaults: createRendererDefaults({ read: async () => [], save: async (list) => list }),
      render: plainPort,
    },
  });
  flushSync();
  // Before the explorer has made it, the header already names it by the seed it will be made with.
  await vi.waitFor(() => {
    flushSync();
    expect(text(header()?.querySelector(".tf-p"))).toBe("schemas/note.xsd › Note-example-4711.xml");
  }, { timeout: 10_000, interval: 20 });
  // Once made, the meta says how and how long, and the view is the XML view.
  await vi.waitFor(() => {
    flushSync();
    expect(text(header()?.querySelector(".tf-fmeta"))).toBe("Example made by knobas, seed 4711, required parts only. 5 lines. Not saved.");
    expect(target.querySelector(".tf-xsd-opened .tf-xv")).not.toBeNull();
  }, { timeout: 10_000, interval: 20 });
  const buttons = [...header()!.querySelectorAll(".tf-trail button")].map((b) => text(b));
  expect(buttons).not.toContain("Pipeline");
  expect(header()!.querySelector(".tf-rsw-face")).toBeNull();
  expect(buttons.filter((b) => b !== "Zen")).toEqual(["Copy", "Save to Downloads"]);
  [...header()!.querySelectorAll<HTMLButtonElement>(".tf-trail button")].find((b) => text(b) === "Save to Downloads")!.click();
  await vi.waitFor(() => expect(saved.length).toBe(1));
  expect(saved[0]!.name).toBe("Note-example-4711.xml");
  expect(saved[0]!.text.split("\n")[1]).toContain("<!-- Made by knobas from note.xsd (Note), seed 4711, required parts only.");
});
