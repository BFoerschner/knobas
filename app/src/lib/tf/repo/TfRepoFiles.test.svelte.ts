/**
 * The Files tab mounted on its own over a stand-in source whose answers the
 * test releases by hand, so the order the tab asks in and the order it draws
 * in are both visible (#2244, spec #2234 step 2), and over long files for the
 * windowed viewer (#2251, spec #2234 step 5). The stand-in is shaped like
 * `knobas-source-gitea`'s tree (`files`, `files:<dir>`, `file:<path>`), as the
 * dev fake's is.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { SourceView, TreeEntry, TreeSections } from "../../ipc/entity";
import TfRepoFiles from "./TfRepoFiles.svelte";
import TfRepoFilesRefreshed from "./TfRepoFiles.refresh.fixture.svelte";
import type { HighlightHandlers, HighlightRequest, RenderPort } from "../../render/port";
import { installCommands, type CommandPort } from "../../render/command";
import { createRendererDefaults, type RendererDefaults } from "../../render/defaults.svelte";
import type { RendererDefault } from "../../render/defaults";
import { fileHash, lineHash } from "./repo";
import { toasts } from "../../shell/toasts.svelte";

const REPO = "gitea:tidewater/payout-service";

/** A tree of files by path; the stand-in lists its directories from it. */
type Files = Record<string, string>;

function listing(files: Files, dir: string): TreeEntry[] {
  const prefix = dir === "" ? "" : `${dir}/`;
  const dirs = new Set<string>();
  const entries: TreeEntry[] = [];
  for (const [path, text] of Object.entries(files)) {
    if (!path.startsWith(prefix)) continue;
    const rest = path.slice(prefix.length);
    if (rest.includes("/")) dirs.add(rest.slice(0, rest.indexOf("/")));
    else entries.push({ name: rest, kind: "file", size: text.length, section: `file:${path}` });
  }
  return [...[...dirs].sort().map((name): TreeEntry => ({ name, kind: "dir", size: null, section: `files:${prefix}${name}` })), ...entries];
}

function answer(files: Files, section: string): SourceView {
  const one = (blocks: unknown[]) => ({ sections: [{ id: section, title: "Files", count: null, deferred: false, blocks }], actions: [] }) as unknown as SourceView;
  if (section === "files" || section.startsWith("files:")) {
    const dir = section === "files" ? "" : section.slice("files:".length);
    return one([{ type: "tree", trail: [], entries: listing(files, dir), truncated: false, url: null }]);
  }
  const path = section.slice("file:".length);
  const text = files[path];
  if (text === undefined) throw { code: "not_found", message: `gitea: no file ${path} at main` };
  return one([{ type: "file", trail: [], path, text, language: null, size: text.length, truncated: false, binary: false, url: null, sha: "b10b", commit: "c0mmit" }]);
}

/** A read asked and not yet answered: `release` answers it. */
interface Asked {
  section: string;
  release: () => void;
}

/** A source that answers each read only when the test releases it. */
function heldSource(files: Files) {
  const asked: Asked[] = [];
  const answered: string[] = [];
  const entityView = (_at: string, section: string | null) =>
    new Promise<SourceView>((resolve, reject) => {
      const id = section ?? "";
      asked.push({
        section: id,
        release: () => {
          answered.push(id);
          try {
            resolve(answer(files, id));
          } catch (error) {
            reject(error);
          }
        },
      });
    });
  return { asked, answered, entityView };
}

/** A source that answers every read at once. */
function openSource(files: Files) {
  return { entityView: async (_at: string, section: string | null) => answer(files, section ?? "") };
}

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;
/** The scheme the repository's detail declares for its tree (#2298): the stand-in's, Gitea's. */
const GITEA_TREE: TreeSections = { dir: "files:", file: "file:" };
let declared: TreeSections | null = GITEA_TREE;

beforeEach(() => {
  declared = GITEA_TREE;
  target = document.createElement("div");
  document.body.appendChild(target);
});

afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
});

function draw(
  entityView: (at: string, section: string | null) => Promise<SourceView>,
  path: string | null,
  line: number | null = null,
  render?: RenderPort,
  commands?: CommandPort,
  defaults: RendererDefaults = createRendererDefaults(storedDefaults().port),
  fileBytes?: (entityId: string, path: string, commit: string | null) => Promise<Uint8Array>,
) {
  const ports = {
    entityView,
    ...(fileBytes ? { fileBytes } : {}),
    submitWrite: async () => {
      throw { code: "invalid", message: "no write in this suite" };
    },
  };
  view = mount(TfRepoFiles, {
    target,
    props: {
      ports,
      repoId: REPO,
      at: REPO,
      branch: "main",
      path,
      line,
      changed: new Set<string>(),
      treeSections: declared,
      defaults,
      ...(render ? { render } : {}),
      ...(commands ? { commands } : {}),
    },
  });
  flushSync();
}

async function settle() {
  for (let round = 0; round < 4; round += 1) {
    for (let i = 0; i < 10; i += 1) await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 0));
    flushSync();
  }
}

const text = (el: Element | null) => el?.textContent?.replace(/\s+/g, " ").trim() ?? "";
const tree = () => [...target.querySelectorAll(".tf-ftree a, .tf-ftree button")].map(text);
const drawnLines = () => target.querySelectorAll(".tf-fileview .tf-code .tf-l");
/** Drawn line `i`'s number and text. */
const lineText = (i: number) => [...(drawnLines()[i]?.children ?? [])].map((span) => span.textContent);

const DEEP = "src/psp/gateway/sepa/codes.rs";
const DEEP_FILES: Files = {
  "README.md": "# payout-service",
  [DEEP]: "pub const ACCEPTED: &str = \"ACCP\";\n",
  "src/psp/gateway/sepa/mod.rs": "pub mod codes;",
};

test("a file four directories deep asks for its directories and the file before any of them answers", async () => {
  const source = heldSource(DEEP_FILES);
  draw(source.entityView, DEEP);
  await settle();

  expect(source.answered).toEqual([]);
  expect(source.asked.map((read) => read.section).sort()).toEqual(
    ["files", "files:src", "files:src/psp", "files:src/psp/gateway", "files:src/psp/gateway/sepa", `file:${DEEP}`].sort(),
  );
});

test("the deep file draws as soon as its read answers, and the tree fills in as its directories arrive", async () => {
  const source = heldSource(DEEP_FILES);
  draw(source.entityView, DEEP);
  await settle();
  const release = (section: string) => source.asked.find((read) => read.section === section)!.release();

  release(`file:${DEEP}`);
  await settle();
  expect(text(target.querySelector(".tf-fileview header .tf-p"))).toBe(DEEP);
  expect(lineText(0)).toEqual(["1", 'pub const ACCEPTED: &str = "ACCP";']);
  expect(tree()).toEqual([]);

  release("files");
  release("files:src");
  await settle();
  expect(tree()).toEqual(["src", "psp", "README.md"]);

  for (const section of ["files:src/psp", "files:src/psp/gateway", "files:src/psp/gateway/sepa"]) release(section);
  await settle();
  expect(tree()).toEqual(["src", "psp", "gateway", "sepa", "codes.rs", "mod.rs", "README.md"]);
  // The listing that names the file takes the read already answered: one read each.
  expect(source.asked).toHaveLength(6);
  expect(lineText(0)).toEqual(["1", 'pub const ACCEPTED: &str = "ACCP";']);
});

test("Collapse all folds every directory open and leaves the file open (#2257)", async () => {
  const source = heldSource(DEEP_FILES);
  draw(source.entityView, DEEP);
  await settle();
  for (const read of [...source.asked]) read.release();
  await settle();
  expect(tree()).toEqual(["src", "psp", "gateway", "sepa", "codes.rs", "mod.rs", "README.md"]);

  target.querySelector<HTMLButtonElement>("button[title='Collapse all']")!.click();
  flushSync();
  expect(tree()).toEqual(["src", "README.md"]);
  expect(text(target.querySelector(".tf-fileview header .tf-p"))).toBe(DEEP);
  // A directory opened again opens one level: what was below it stays folded.
  target.querySelector<HTMLButtonElement>(".tf-ftree button.tf-fdir")!.click();
  flushSync();
  expect(tree()).toEqual(["src", "psp", "README.md"]);
});

test("a deep file opened again after its early read reads it afresh", async () => {
  const files: Files = { ...DEEP_FILES };
  const reads: string[] = [];
  const entityView = async (_at: string, section: string | null) => {
    reads.push(section ?? "");
    return answer(files, section ?? "");
  };
  const ports = {
    entityView,
    submitWrite: async () => {
      throw { code: "invalid", message: "no write in this suite" };
    },
  };
  const props = $state({ ports, repoId: REPO, at: REPO, branch: "main", path: DEEP as string | null, line: null, changed: new Set<string>() });
  view = mount(TfRepoFiles, { target, props });
  flushSync();
  await settle();
  expect(lineText(0)).toEqual(["1", 'pub const ACCEPTED: &str = "ACCP";']);

  files[DEEP] = "pub const ACCEPTED: &str = \"ACSC\";\n";
  props.path = "src/psp/gateway/sepa/mod.rs";
  await settle();
  props.path = DEEP;
  await settle();

  expect(lineText(0)).toEqual(["1", 'pub const ACCEPTED: &str = "ACSC";']);
  expect(reads.filter((read) => read === `file:${DEEP}`)).toHaveLength(2);
});

test("stepping back to the file drawn before the next one answers shows it undimmed", async () => {
  const source = heldSource(DEEP_FILES);
  const ports = {
    entityView: source.entityView,
    submitWrite: async () => {
      throw { code: "invalid", message: "no write in this suite" };
    },
  };
  const props = $state({ ports, repoId: REPO, at: REPO, branch: "main", path: DEEP as string | null, line: null, changed: new Set<string>() });
  view = mount(TfRepoFiles, { target, props });
  flushSync();
  const releaseAll = async () => {
    for (let round = 0; round < 6; round += 1) {
      for (const read of [...source.asked]) if (!source.answered.includes(read.section)) read.release();
      await settle();
    }
  };
  await releaseAll();
  expect(lineText(0)).toEqual(["1", 'pub const ACCEPTED: &str = "ACCP";']);

  // mod.rs's read is held; the reader steps back to codes.rs before it answers.
  props.path = "src/psp/gateway/sepa/mod.rs";
  await settle();
  expect(target.querySelector(".tf-fileview .tf-wait-dim")).not.toBeNull();
  props.path = DEEP;
  await settle();

  expect(target.querySelector(".tf-fileview .tf-wait-dim")).toBeNull();
  expect(lineText(0)).toEqual(["1", 'pub const ACCEPTED: &str = "ACCP";']);
});

test("a link to a file that does not exist says so once its directory is read", async () => {
  const missing = "src/psp/gateway/sepa/gone.rs";
  const source = heldSource(DEEP_FILES);
  draw(source.entityView, missing);
  await settle();

  source.asked.find((read) => read.section === `file:${missing}`)!.release();
  await settle();
  expect(target.querySelector(".tf-cfail")).toBeNull();

  for (const read of [...source.asked]) if (!source.answered.includes(read.section)) read.release();
  await settle();
  expect(text(target.querySelector(".tf-fileview .tf-cfail"))).toBe(`There is no file ${missing} on main.`);
});

test("a source that declares no tree sections asks for no section its listings have not named (#2298)", async () => {
  declared = null;
  const source = heldSource(DEEP_FILES);
  draw(source.entityView, DEEP);
  await settle();

  expect(source.asked.map((read) => read.section)).toEqual(["files"]);
});

test("a deep link asks for the sections its source declares, in the source's own spelling (#2298)", async () => {
  declared = { dir: "dir:", file: "blob:" };
  const source = heldSource(DEEP_FILES);
  draw(source.entityView, DEEP);
  await settle();

  expect(source.answered).toEqual([]);
  expect(source.asked.map((read) => read.section).sort()).toEqual(
    ["files", "dir:src", "dir:src/psp", "dir:src/psp/gateway", "dir:src/psp/gateway/sepa", `blob:${DEEP}`].sort(),
  );
});

test("a source that names its sections otherwise still opens the deep file, through the sections its listings name", async () => {
  // `dir:`/`blob:` in place of Gitea's `files:`/`file:`: every early guess is refused.
  const theirs = (id: string) => id.replace(/^files:/, "dir:").replace(/^file:/, "blob:");
  const ours = (id: string) => id.replace(/^dir:/, "files:").replace(/^blob:/, "file:");
  const reads: string[] = [];
  const entityView = async (_at: string, section: string | null) => {
    const id = section ?? "";
    reads.push(id);
    if (id !== "files" && ours(id) === id) throw { code: "invalid", message: `no section ${id}` };
    const view = answer(DEEP_FILES, ours(id));
    const block = view.sections[0]!.blocks[0]!;
    if (block.type === "tree") for (const entry of block.entries) entry.section = entry.section && theirs(entry.section);
    return view;
  };
  draw(entityView, DEEP);
  await settle();

  expect(lineText(0)).toEqual(["1", 'pub const ACCEPTED: &str = "ACCP";']);
  expect(target.querySelector(".tf-cfail")).toBeNull();
  expect(tree()).toEqual(["src", "psp", "gateway", "sepa", "codes.rs", "mod.rs", "README.md"]);
  expect(reads).toContain(`blob:${DEEP}`);
});

/** A file of `n` numbered lines. */
const long = (n: number) => Array.from({ length: n }, (_, i) => `let value_${i + 1} = ${i + 1};`).join("\n");

test("a 50,000-line file draws fewer than 500 of its lines", async () => {
  draw(openSource({ "BULK.rs": long(50_000) }).entityView, "BULK.rs");
  await settle();

  expect(text(target.querySelector(".tf-fileview header .tf-dim"))).toBe("1.2 MB · 50000 lines · at c0mmit");
  expect(drawnLines().length).toBeGreaterThan(0);
  expect(drawnLines().length).toBeLessThan(500);
  expect(lineText(0)).toEqual(["1", "let value_1 = 1;"]);
});

test("a 1,999-line file draws whole", async () => {
  draw(openSource({ "BULK.rs": long(1_999) }).entityView, "BULK.rs");
  await settle();

  expect(drawnLines()).toHaveLength(1_999);
  expect(lineText(1_998)).toEqual(["1999", "let value_1999 = 1999;"]);
});

test("a link to line 40,000 of a long file draws and marks that line", async () => {
  draw(openSource({ "BULK.rs": long(50_000) }).entityView, "BULK.rs", 40_000);
  await settle();

  const marked = target.querySelectorAll(".tf-fileview .tf-code .tf-l.tf-at");
  expect(marked).toHaveLength(1);
  expect([...marked[0]!.children].map((span) => span.textContent)).toEqual(["40000", "let value_40000 = 40000;"]);
  expect(drawnLines().length).toBeLessThan(500);
});

test("copying a selection across lines gives the selected text, without the line numbers", async () => {
  draw(openSource({ "src/main.rs": "fn main() {\n    run();\n}\n" }).entityView, "src/main.rs");
  await settle();

  // From after `fn ` on line 1 to after `}` on line 3, as a drag leaves it.
  const body = (i: number) => drawnLines()[i]!.lastElementChild!.firstChild!;
  const range = document.createRange();
  range.setStart(body(0), 3);
  range.setEnd(body(2), 1);
  const selection = document.getSelection()!;
  selection.removeAllRanges();
  selection.addRange(range);
  const copied: Record<string, string> = {};
  const event = Object.assign(new Event("copy", { bubbles: true, cancelable: true }), {
    clipboardData: { setData: (type: string, value: string) => (copied[type] = value) },
  });
  body(1).parentElement!.dispatchEvent(event);

  expect(copied["text/plain"]).toBe("main() {\n    run();\n}");
  expect(event.defaultPrevented).toBe(true);
});

/** A renderer port whose highlighting answers only when the test says, with what each job was asked and focused on. */
function heldRender() {
  const jobs: { request: HighlightRequest; handlers: HighlightHandlers; focus: [number, number][]; cancelled: boolean }[] = [];
  const port: RenderPort = {
    highlight(request, handlers) {
      const job = { request, handlers, focus: [] as [number, number][], cancelled: false };
      jobs.push(job);
      return { focus: (from, to) => job.focus.push([from, to]), cancel: () => (job.cancelled = true) };
    },
  };
  return { jobs, port };
}

const colored = () => [...target.querySelectorAll(".tf-fileview .tf-code [class^='tf-hl-']")].map((el) => `${el.className}:${el.textContent}`);

test("a file draws as plain text first and gains colour when the highlighter answers", async () => {
  const render = heldRender();
  draw(openSource({ "src/main.rs": "fn main() {\n    run(42);\n}" }).entityView, "src/main.rs", null, render.port);
  await settle();

  expect(lineText(0)).toEqual(["1", "fn main() {"]);
  expect(colored()).toEqual([]);
  expect(render.jobs.map((job) => job.request.path)).toEqual(["src/main.rs"]);

  render.jobs[0]!.handlers.onLines(0, [[["fn", "kw"], [" main() {", ""]], [["    run(", ""], ["42", "num"], [");", ""]], [["}", ""]]]);
  flushSync();

  expect(colored()).toEqual(["tf-hl-kw:fn", "tf-hl-num:42"]);
  expect(lineText(1)).toEqual(["2", "    run(42);"]);
});

test("a file in a language the highlighter does not know stays plain text, with no error", async () => {
  const render = heldRender();
  draw(openSource({ "notes/plan.zzq": "fn main() {}" }).entityView, "notes/plan.zzq", null, render.port);
  await settle();
  render.jobs[0]!.handlers.onPlain?.();
  flushSync();

  expect(lineText(0)).toEqual(["1", "fn main() {}"]);
  expect(colored()).toEqual([]);
  expect(target.querySelector(".tf-cfail")).toBeNull();
});

test("the highlighter is asked for the lines in view of a long file first, and told when the view moves", async () => {
  const render = heldRender();
  draw(openSource({ "BULK.rs": long(50_000) }).entityView, "BULK.rs", 40_000, render.port);
  await settle();

  const job = render.jobs.at(-1)!;
  const asked = [job.request.focus!, ...job.focus];
  const last = asked.at(-1)!;
  const drawn = [...drawnLines()].map((row) => Number((row as HTMLElement).dataset["n"]) - 1);
  expect(last[1] - last[0]).toBeLessThan(500);
  expect(last[0]).toBeLessThanOrEqual(Math.min(...drawn));
  expect(last[1]).toBeGreaterThan(Math.max(...drawn));
  expect(drawn).toContain(39_999);
});

test("closing the file viewer cancels its highlighting, and a late answer draws nothing", async () => {
  const render = heldRender();
  draw(openSource({ "src/main.rs": "fn main() {}" }).entityView, "src/main.rs", null, render.port);
  await settle();
  const job = render.jobs[0]!;
  expect(job.cancelled).toBe(false);

  unmount(view!);
  view = null;
  expect(job.cancelled).toBe(true);
  job.handlers.onLines(0, [[["fn", "kw"], [" main() {}", ""]]]);
  flushSync();
  expect(colored()).toEqual([]);
});

test("an SVG file is drawn as an image, sanitized first (Standards 1)", async () => {
  const svg = '<svg width="4" height="4" onload="parent.x=1"><script>parent.x=2</script></svg>';
  const made: Blob[] = [];
  const create = URL.createObjectURL;
  URL.createObjectURL = (blob: Blob) => (made.push(blob), `blob:fake/${made.length}`);
  try {
    draw(openSource({ "docs/logo.svg": svg }).entityView, "docs/logo.svg");
    await settle();

    const img = target.querySelector<HTMLImageElement>(".tf-fileview img");
    expect(img?.getAttribute("src")).toBe("blob:fake/1");
    expect(made[0]!.type).toBe("image/svg+xml");
    const drawn = await made[0]!.text();
    expect(drawn).toContain('width="4"');
    expect(drawn).not.toMatch(/script|onload/i);
    expect(drawnLines()).toHaveLength(0);
  } finally {
    URL.createObjectURL = create;
  }
});

// -- Find in file (#2299) ------------------------------------------------------

const findField = () => target.querySelector<HTMLInputElement>("input[aria-label='Find in file']")!;
const findSays = () => text(target.querySelector(".tf-findbar .tf-lqn"));
const current = () => target.querySelector(".tf-code mark.tf-curm");
const rowOf = (el: Element | null) => (el?.closest(".tf-l") as HTMLElement | null)?.dataset["n"];

function find(typed: string) {
  findField().value = typed;
  findField().dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
}

function press(shift = false) {
  findField().dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", shiftKey: shift, bubbles: true, cancelable: true }));
  flushSync();
}

test("find in a file drawn whole marks every hit and steps through them with Enter and Shift+Enter", async () => {
  draw(openSource({ "src/sepa/retry.rs": "retry once\nnothing\nretry twice, Retry thrice" }).entityView, "src/sepa/retry.rs");
  await settle();

  find("retry");
  expect(findSays()).toBe("1 of 3");
  expect(target.querySelectorAll(".tf-code mark")).toHaveLength(3);
  expect(rowOf(current())).toBe("1");

  press();
  expect(findSays()).toBe("2 of 3");
  expect(rowOf(current())).toBe("3");
  press();
  expect(text(current())).toBe("Retry");
  press(true);
  press(true);
  expect(findSays()).toBe("1 of 3");
  expect(rowOf(current())).toBe("1");

  find("absent");
  expect(findSays()).toBe("0 found");
  expect(target.querySelectorAll(".tf-code mark")).toHaveLength(0);
});

test("find in a long file draws a hit's line the window left out before marking it", async () => {
  const lines = long(50_000).split("\n");
  lines[44_443] = "let needle = 1;";
  lines[9] = "let needle = 0;";
  draw(openSource({ "BULK.rs": lines.join("\n") }).entityView, "BULK.rs");
  await settle();

  find("needle");
  expect(findSays()).toBe("1 of 2");
  expect(target.querySelector(".tf-l[data-n='44444']")).toBeNull();

  press();
  expect(findSays()).toBe("2 of 2");
  expect(rowOf(current())).toBe("44444");
  expect(drawnLines().length).toBeLessThan(500);
  press();
  expect(rowOf(current())).toBe("10");
});

test("a file a saved command renderer accepts is run by its id and drawn in the sandboxed frame (#2262)", async () => {
  installCommands([{ id: "cat", name: "Cat", command: "cat", patterns: ["*.page"], fences: [], input: "text", output: "html", timeout_secs: 10, diff: false }]);
  const asked: string[] = [];
  const commands: CommandPort = {
    run: async (id, content) => {
      asked.push(id);
      return { outcome: "drawn", output: "html", body: content.kind === "text" ? content.text : "" };
    },
  };
  try {
    draw(openSource({ "docs/index.page": "<p>Hi</p><script>alert(1)</script>" }).entityView, "docs/index.page", null, undefined, commands);
    await settle();

    expect(asked).toEqual(["cat"]);
    const frame = target.querySelector(".tf-fileview iframe")!;
    expect(frame.getAttribute("sandbox")).toBe("");
    expect(frame.getAttribute("srcdoc")).toContain("<p>Hi</p>");
    expect(frame.getAttribute("srcdoc")).not.toMatch(/<script/i);
  } finally {
    installCommands([]);
  }
});

// -- Markdown (#2255) -----------------------------------------------------------

const MD_FILES: Files = {
  "README.md": [
    "# payout-service",
    "",
    "| Area | State |",
    "|---|---|",
    "| Card | live |",
    "",
    "- [x] Retry with backoff",
    "- [ ] Dead-letter queue",
    "",
    "```rust",
    "fn main() {}",
    "```",
    "",
    "See [the runbook](docs/runbook.md).",
  ].join("\n"),
  "docs/runbook.md": "# Runbook\n\n[Back](../README.md), [the retry](../src/retry.rs#L3).",
  "docs/logo.svg": '<svg width="4" height="4"/>',
  "src/retry.rs": "fn retry() {}\n\n\n",
};

const rendered = () => target.querySelector(".tf-fileview .tf-md");
/** The switcher's face, "Render as <name>". */
const face = () => text(target.querySelector(".tf-fileview .tf-rsw-face"));
/** Opens the switcher and reads its items: a tick `*` on the one shown, then its name and tag. */
function menuItems(): string[] {
  if (target.querySelector(".tf-rsw .tf-menu") === null) {
    target.querySelector<HTMLButtonElement>(".tf-rsw-face")!.click();
    flushSync();
  }
  return [...target.querySelectorAll<HTMLElement>(".tf-rsw .tf-menu [role^='menuitem']")].map(
    (item) => `${item.getAttribute("aria-checked") === "true" ? "*" : ""}${text(item).replace(/^✓ ?/, "")}`,
  );
}
function choose(start: string) {
  menuItems();
  [...target.querySelectorAll<HTMLElement>(".tf-rsw .tf-menu [role^='menuitem']")].find((item) => text(item).replace(/^✓ ?/, "").startsWith(start))!.click();
  flushSync();
}

/** A backend's `render.defaults` row: what was saved is what the next read answers, as after a restart. */
function storedDefaults(start: RendererDefault[] = []) {
  let row = start;
  const saves: RendererDefault[][] = [];
  const port = {
    read: async () => row,
    save: async (list: RendererDefault[]) => {
      saves.push(list);
      row = list;
      return list;
    },
  };
  return { port, saves };
}

test("the landing renders the README as Markdown: its table, task list, Rust fence and relative link", async () => {
  draw(openSource(MD_FILES).entityView, null);
  await settle();

  expect(drawnLines()).toHaveLength(0);
  expect(text(rendered()!.querySelector("h1"))).toBe("payout-service");
  expect([...rendered()!.querySelectorAll("td")].map(text)).toEqual(["Card", "live"]);
  expect(rendered()!.querySelectorAll("li.tf-task")).toHaveLength(2);
  expect(text(rendered()!.querySelector(".tf-fence pre"))).toBe("fn main() {}");
  expect(rendered()!.querySelector("a")?.getAttribute("href")).toBe(fileHash(REPO, "docs/runbook.md"));
  expect(face()).toBe("Render as Markdown");
});

test("a README's relative image is read through the byte read at the README's commit and drawn from a blob (#2256)", async () => {
  const asked: [string, string, string | null][] = [];
  const create = URL.createObjectURL;
  URL.createObjectURL = () => "blob:readme/0";
  try {
    const files = { ...MD_FILES, "docs/runbook.md": "# Runbook\n\n![flow](img/flow.png)" };
    draw(openSource(files).entityView, "docs/runbook.md", null, undefined, undefined, undefined, async (entityId, path, commit) => {
      asked.push([entityId, path, commit]);
      return new Uint8Array([0x89, 0x50]);
    });
    await settle();
    expect(asked).toEqual([[REPO, "docs/img/flow.png", "c0mmit"]]);
    expect(rendered()!.querySelector("img")?.getAttribute("src")).toBe("blob:readme/0");
  } finally {
    URL.createObjectURL = create;
  }
});

test("a Markdown file's relative links open files in the Files tab, resolved against its own directory", async () => {
  draw(openSource(MD_FILES).entityView, "docs/runbook.md");
  await settle();

  expect([...rendered()!.querySelectorAll("a")].map((a) => a.getAttribute("href"))).toEqual([
    fileHash(REPO, "README.md"),
    lineHash(REPO, "src/retry.rs", 3),
  ]);
});

test("a .md file mapped to Source opens as source, and the switcher offers Markdown (#2263)", async () => {
  const defaults = createRendererDefaults(storedDefaults([{ pattern: "*.md", renderer: "source", pipeline: null }]).port);
  await defaults.load();
  draw(openSource(MD_FILES).entityView, "README.md", null, undefined, undefined, defaults);
  await settle();

  expect(rendered()).toBeNull();
  expect(lineText(0)).toEqual(["1", "# payout-service"]);
  expect(face()).toBe("Render as Source");
  expect(menuItems()).toEqual([
    "Markdown built in",
    "*Source built in · highlighted DEFAULT FOR *.md",
    "Use Markdown for *.md by default",
    "Renderers and defaults… Settings › Renderers",
  ]);

  choose("Markdown built in");
  expect(text(rendered()!.querySelector("h1"))).toBe("payout-service");
  expect(face()).toBe("Render as Markdown");
});

test("a pick in the switcher holds for that file; Make default writes the pattern, and a restart reads it back (#2263)", async () => {
  const backend = storedDefaults();
  const props = $state({
    ports: { ...openSource(MD_FILES), submitWrite: async () => ({}) as never },
    repoId: REPO,
    at: REPO,
    branch: "main",
    path: "README.md" as string | null,
    changed: new Set<string>(),
    defaults: createRendererDefaults(backend.port),
  });
  view = mount(TfRepoFiles, { target, props });
  await settle();
  expect(menuItems()).toEqual([
    "*Markdown built in DEFAULT FOR *.md",
    "Source built in · highlighted",
    "Use Source for *.md by default",
    "Renderers and defaults… Settings › Renderers",
  ]);

  choose("Source built in");
  expect(lineText(0)).toEqual(["1", "# payout-service"]);
  props.path = "docs/runbook.md";
  await settle();
  expect(face()).toBe("Render as Markdown");
  props.path = "README.md";
  await settle();
  expect(face()).toBe("Render as Source");
  expect(backend.saves).toEqual([]);

  choose("Use Source for *.md by default");
  await settle();
  expect(backend.saves).toEqual([[{ pattern: "*.md", renderer: "source", pipeline: null }]]);
  props.path = "docs/runbook.md";
  await settle();
  expect(face()).toBe("Render as Source");

  // The app again: a new copy reads what the backend holds.
  unmount(view);
  const again = createRendererDefaults(backend.port);
  await again.load();
  draw(openSource(MD_FILES).entityView, "README.md", null, undefined, undefined, again);
  await settle();
  expect(face()).toBe("Render as Source");
  expect(lineText(0)).toEqual(["1", "# payout-service"]);
});

test("Make default that the backend refuses says so and changes nothing (#2263)", async () => {
  const defaults = createRendererDefaults({
    read: async () => [],
    save: async () => {
      throw { code: "invalid", message: "*.md has a default already" };
    },
  });
  draw(openSource(MD_FILES).entityView, "README.md", null, undefined, undefined, defaults);
  await settle();
  choose("Use Source for *.md by default");
  await settle();
  expect(defaults.list).toEqual([]);
  expect(face()).toBe("Render as Markdown");
  expect(toasts.items.map((t) => t.text)).toEqual(["Could not make Source the default: *.md has a default already"]);
});

test("a file only Source takes still shows the switcher, with nothing to make default", async () => {
  draw(openSource(MD_FILES).entityView, "src/retry.rs");
  await settle();
  expect(face()).toBe("Render as Source");
  expect(menuItems()).toEqual(["*Source built in · highlighted DEFAULT FOR *.rs", "Renderers and defaults… Settings › Renderers"]);
});

// -- Mermaid files (#2258) --------------------------------------------------------

function diagramRender() {
  const asked: string[] = [];
  const port: RenderPort = {
    ...heldRender().port,
    async diagram(request) {
      asked.push(request.code);
      if (request.code.includes("-->|")) return { error: "Parse error on line 2:\nExpecting 'SEMI', got 'PIPE'" };
      return { svg: '<svg viewBox="0 0 300 120"><text>drawn</text></svg>' };
    },
  };
  return { asked, port };
}

test("a .mmd file is drawn as a diagram in a sandboxed frame, and Source shows its lines", async () => {
  const render = diagramRender();
  draw(openSource({ "docs/flow.mmd": "flowchart LR\n  a --> b" }).entityView, "docs/flow.mmd", null, render.port);
  await settle();

  const frame = target.querySelector<HTMLIFrameElement>(".tf-fileview .tf-fdiagram iframe")!;
  expect(frame.getAttribute("sandbox")).toBe("");
  expect([frame.width, frame.height]).toEqual(["300", "120"]);
  expect(render.asked).toEqual(["flowchart LR\n  a --> b"]);
  expect(menuItems()).toEqual([
    "*Mermaid built in · strict security DEFAULT FOR *.mmd",
    "Source built in · highlighted",
    "Use Source for *.mmd by default",
    "Renderers and defaults… Settings › Renderers",
  ]);

  choose("Source built in");
  await settle();
  expect(target.querySelector(".tf-fileview iframe")).toBeNull();
  expect(lineText(1)).toEqual(["2", "  a --> b"]);
});

test("a .mermaid file that does not parse shows the parser's error above its numbered source", async () => {
  draw(openSource({ "flow.mermaid": "flowchart TB\n  api -->|outbox worker" }).entityView, "flow.mermaid", null, diagramRender().port);
  await settle();

  const drawn = target.querySelector(".tf-fileview .tf-fdiagram")!;
  expect(text(drawn.querySelector(".tf-stepfail b"))).toBe("Mermaid could not draw this.");
  expect(drawn.querySelector(".tf-stepfail pre")?.textContent).toContain("Expecting 'SEMI', got 'PIPE'");
  expect([...drawn.querySelectorAll(".tf-code .tf-l")].map((row) => [...row.children].map((span) => span.textContent))).toEqual([
    ["1", "flowchart TB"],
    ["2", "  api -->|outbox worker"],
  ]);
  expect(drawn.querySelector("iframe")).toBeNull();
});

test("a .d2, a .dot and a .gv file are drawn by their engines in a sandboxed frame, with Source in the switcher (#2259)", async () => {
  for (const [path, engine, blurb, pattern] of [
    ["docs/flow.d2", "D2", "WASM", "*.d2"],
    ["deps.dot", "Graphviz", "viz.js WASM", "*.dot"],
    ["graphs/deps.gv", "Graphviz", "viz.js WASM", "*.gv"],
  ] as const) {
    if (view) unmount(view);
    view = null;
    const render = diagramRender();
    draw(openSource({ [path]: "a -> b" }).entityView, path, null, render.port);
    await settle();
    const frame = target.querySelector<HTMLIFrameElement>(".tf-fileview .tf-fdiagram iframe")!;
    expect([path, frame.getAttribute("sandbox"), frame.title]).toEqual([path, "", `${engine} diagram`]);
    expect(render.asked).toEqual(["a -> b"]);
    expect(menuItems()).toEqual([
      `*${engine} built in · ${blurb} DEFAULT FOR ${pattern}`,
      "Source built in · highlighted",
      `Use Source for ${pattern} by default`,
      "Renderers and defaults… Settings › Renderers",
    ]);
  }
});

// ---- the tree at a commit, moved behind (#2241, #2242) ----

/**
 * A source whose listings name the commit they were read at, as Gitea's do
 * (`file@<n>:<path>`), and whose branch `push()` moves to a new one.
 */
function commitSource() {
  let tip = 1;
  const texts: Record<number, string> = { 1: "// one", 2: "// two" };
  const reads: string[] = [];
  const entityView = async (_at: string, section: string | null): Promise<SourceView> => {
    const id = section ?? "";
    reads.push(id);
    const one = (blocks: unknown[]) => ({ sections: [{ id, title: "Files", count: null, deferred: false, blocks }], actions: [] }) as unknown as SourceView;
    if (id === "files")
      return one([{ type: "tree", trail: [], entries: [{ name: "main.rs", kind: "file", size: 5, section: `file@${tip}:main.rs` }], truncated: false, url: null }]);
    const at = Number(/^file@(\d+):/.exec(id)?.[1] ?? tip);
    return one([{ type: "file", trail: [], path: "main.rs", text: texts[at], language: null, size: 5, truncated: false, binary: false, url: null, sha: `b${at}`, commit: null }]);
  };
  return { reads, entityView, push: () => (tip += 1) };
}

test("the app's view:fresh for the top directory reads the tree at the new commit and the file there; one that did not move reads nothing more", async () => {
  const source = commitSource();
  const heard: ((event: { payload: unknown }) => void)[] = [];
  const ports = {
    entityView: source.entityView,
    submitWrite: async () => {
      throw { code: "invalid", message: "no write in this suite" };
    },
    listen: async (event: string, handler: (event: { payload: unknown }) => void) => {
      if (event === "view:fresh") heard.push(handler);
      return () => {};
    },
  };
  view = mount(TfRepoFiles, { target, props: { ports, repoId: REPO, at: REPO, branch: "main", path: "main.rs", changed: new Set<string>() } });
  await settle();
  expect(lineText(0)).toEqual(["1", "// one"]);

  const tell = (entity_id: string, section: string) => {
    for (const hear of heard) hear({ payload: { entity_id, section } });
  };
  const before = source.reads.length;
  tell(REPO, "files");
  await settle();
  expect(source.reads.slice(before)).toEqual(["files"]);
  expect(lineText(0)).toEqual(["1", "// one"]);

  source.push();
  tell(`${REPO}@refs/heads/other`, "files");
  await settle();
  expect(lineText(0)).toEqual(["1", "// one"]);

  tell(REPO, "files");
  await settle();
  expect(lineText(0)).toEqual(["1", "// two"]);
  expect(source.reads).toContain("file@2:main.rs");
});

test("a deep link's guess that drew the file stands for the section its listing names, so the file is not read twice", async () => {
  const source = commitSource();
  draw(source.entityView, "main.rs");
  await settle();
  expect(lineText(0)).toEqual(["1", "// one"]);
  expect(source.reads.filter((id) => id.endsWith("main.rs"))).toEqual(["file:main.rs"]);
});

// -- The scroll a file opens at (#2260's merger) ---------------------------------

test("another file opens at its top, not at the depth the last one was read to; a link to a line keeps its own scroll", async () => {
  // The shell's one scroller, with the file view 300 px down its content.
  const main = document.createElement("div");
  main.className = "tf-main";
  target.appendChild(main);
  const rect = (top: number) => ({ top, bottom: top, left: 0, right: 0, width: 0, height: 0, x: 0, y: top, toJSON: () => ({}) });
  const real = Element.prototype.getBoundingClientRect;
  Element.prototype.getBoundingClientRect = function (this: Element) {
    if (this === main) return rect(100);
    if (this.classList.contains("tf-fileview")) return rect(100 + 300 - main.scrollTop);
    return real.call(this);
  };
  try {
    const props = $state({
      ports: { ...openSource(DEEP_FILES), submitWrite: async () => ({}) as never },
      repoId: REPO,
      at: REPO,
      branch: "main",
      path: "README.md" as string | null,
      line: null as number | null,
      refreshed: 0,
      changed: new Set<string>(),
      defaults: createRendererDefaults(storedDefaults().port),
    });
    view = mount(TfRepoFilesRefreshed, { target: main, props: { props } });
    await settle();

    // Read far down README.md, then open another file: its top comes to rest 12 px under the scroller's.
    main.scrollTop = 1500;
    props.path = "src/psp/gateway/sepa/mod.rs";
    await settle();
    expect(text(main.querySelector(".tf-code"))).toContain("pub mod codes;");
    expect(main.scrollTop).toBe(300 - 12);

    // The file's top already in view: nothing moves.
    main.scrollTop = 200;
    props.path = DEEP;
    await settle();
    expect(main.scrollTop).toBe(200);

    // A refresh reads the same file again: the reader stays where they are.
    main.scrollTop = 1500;
    props.refreshed += 1;
    await settle();
    expect(main.scrollTop).toBe(1500);

    // A link to a line scrolls to that line instead (jsdom has no `scrollIntoView`, so it stays put here).
    main.scrollTop = 1500;
    props.path = "README.md";
    props.line = 1;
    await settle();
    expect(main.scrollTop).toBe(1500);
  } finally {
    Element.prototype.getBoundingClientRect = real;
  }
});
