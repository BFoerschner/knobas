/** Settings › Commands and a command's page (ADR-0060, section 27). */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, expect, test, vi } from "vitest";

import type { OneOff, OneOffDraft, OneOffRepoCommands } from "../../ipc/oneoff";
import { OneOffs, type OneOffPorts } from "../../render/oneoff.svelte";
import TfCommandsSection from "./TfCommandsSection.svelte";
import TfCommandPage from "./TfCommandPage.svelte";
import { commandPageHash, commandPagePlace, offeredOn, commandDraftFrom, toDraft } from "./command-page";

const ESLINT: OneOff = { id: "eslint", name: "ESLint", command: "npx eslint {file}", stdin: "none", files: ["*.ts"], folders: [], has: [], repo_holds: [], repos: [], shows: "auto", start_in: "root", path: [], env: [], colours: true, timeout_secs: 120, wsl_paths: false };

function ports(saved: OneOff[]): OneOffPorts & { setCommands: ReturnType<typeof vi.fn> } {
  let list = saved;
  return {
    commands: () => Promise.resolve(list),
    run: () => new Promise(() => {}),
    stop: () => Promise.resolve(),
    runs: () => Promise.resolve([]),
    lines: () => Promise.resolve(null),
    watch: () => Promise.resolve({ known: false, stop: () => {} }),
    save: () => Promise.resolve(""),
    result: () => Promise.resolve(new Uint8Array()),
    copy: () => Promise.resolve(),
    setCommands: vi.fn((drafts: OneOffDraft[]) => {
      list = drafts.map((d, i) => ({ ...ESLINT, ...d, id: d.id ?? `new-${i}`, colours: d.colours ?? true, timeout_secs: d.timeout_secs ?? 120 }) as OneOff);
      return Promise.resolve(list);
    }),
    forget: () => Promise.resolve(),
    shell: () => Promise.resolve({ on: true, offered: true, read: null }),
    repoCommands: () => Promise.resolve({ file: null, offered: [], failed: null }),
    copyRepoCommands: () => Promise.resolve([]),
    moved: () => () => {},
  };
}

const words = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, " ").trim() ?? "";
async function settle() {
  for (let i = 0; i < 4; i += 1) {
    await new Promise((resolve) => setTimeout(resolve, 0));
    flushSync();
  }
}
let view: ReturnType<typeof mount> | null = null;
afterEach(() => {
  if (view) unmount(view);
  view = null;
  document.body.innerHTML = "";
});

const reads = { repos: () => Promise.resolve([]), checkouts: () => Promise.resolve([]), files: () => Promise.resolve([]) };

test("the page's address names a command, a new one, an example, and the thing to try it on", () => {
  expect(commandPagePlace(["commands", "new"])).toEqual({ command: null, repo: null, on: null });
  expect(commandPagePlace(["commands", "eslint", "gitea:a%2Fb", "src", "a.ts"].map(decodeURIComponent))).toEqual({ command: "eslint", repo: "gitea:a/b", on: "src/a.ts" });
  expect(commandPagePlace(["commands", "example", "2"])).toEqual({ command: null, repo: null, on: null, example: 2 });
  expect(commandPagePlace(["commands", "example", "99"])).toBeNull();
  expect(commandPagePlace(["checkouts", "x"])).toBeNull();
  expect(commandPageHash({ command: "eslint", repo: "gitea:a/b", on: "src/a b.ts" })).toBe("#/settings/commands/eslint/gitea%3Aa%2Fb/src/a%20b.ts");
});

test("It is offered on is read from the placeholders and the globs as they are typed", () => {
  const d = (command: string, extra = {}) => ({ ...commandDraftFrom(null), command, ...extra });
  expect(offeredOn(d("eslint {file}", { files: "*.ts *.tsx", repoHolds: "package.json" }))).toBe("A file, matching *.ts *.tsx, where the repository holds package.json.");
  expect(offeredOn(d("ruff check {dir}", { has: "*.py" }))).toBe("A folder, holding *.py.");
  expect(offeredOn(d("cargo clippy"))).toBe("The repository.");
  expect(offeredOn(d("sh -c 'x {file}'"))).toBeNull();
  const draft = toDraft({ ...commandDraftFrom(null), name: " X ", command: "x {file}", env: "A=1\n\nB=two=2", path: "{root}/bin\n", timeout: "9000" }, null);
  expect(draft).toMatchObject({ name: "X", env: [{ name: "A", value: "1" }, { name: "B", value: "two=2" }], path: ["{root}/bin"], timeout_secs: 3600 });
});

test("the section lists the commands in the menus' order and the six examples; none is installed by itself", async () => {
  const store = new OneOffs(ports([]));
  view = mount(TfCommandsSection, { target: document.body, props: { store } });
  await settle();
  expect(words(document.querySelector(".tf-stg-sec .tf-stg-p"))).toBe("No command yet: the menus offer none until you add one.");
  expect(document.querySelectorAll(".tf-oo-ex").length).toBe(6);
  unmount(view);
  const two = new OneOffs(ports([ESLINT, { ...ESLINT, id: "ruff", name: "Ruff", command: "ruff check {dir}", files: [] }]));
  view = mount(TfCommandsSection, { target: document.body, props: { store: two } });
  await settle();
  const rows = [...document.querySelectorAll(".tf-oo-tbl tbody tr")].map((tr) => words(tr.querySelector("td")));
  expect(rows).toEqual(["ESLint", "Ruff"]);
  expect(words(document.querySelectorAll(".tf-oo-tbl tbody tr")[1]!.querySelectorAll("td")[2])).toBe("A folder");
});

test("a new command's page refuses a path in a script, then saves the command", async () => {
  const p = ports([]);
  const store = new OneOffs(p);
  await store.load();
  view = mount(TfCommandPage, { target: document.body, props: { at: { command: null, repo: null, on: null }, store, reads } });
  await settle();
  const name = document.querySelector<HTMLInputElement>("#tf-ocp-name")!;
  const cmd = document.querySelector<HTMLInputElement>("#tf-ocp-cmd")!;
  name.value = "Lint";
  name.dispatchEvent(new Event("input"));
  cmd.value = "sh -c 'eslint {file}'";
  cmd.dispatchEvent(new Event("input"));
  flushSync();
  expect(words(document.querySelector(".tf-stg-errt"))).toContain("quoted script");
  expect(document.querySelector<HTMLButtonElement>("button[type=submit]")!.disabled).toBe(true);
  cmd.value = "eslint {file}";
  cmd.dispatchEvent(new Event("input"));
  flushSync();
  expect(words(document.querySelector(".tf-oo-offered"))).toBe("It is offered on A file.");
  document.querySelector<HTMLFormElement>("#tf-ocp-form")!.dispatchEvent(new Event("submit", { cancelable: true }));
  await settle();
  expect(p.setCommands).toHaveBeenCalledTimes(1);
  expect(p.setCommands.mock.calls[0]![0][0]).toMatchObject({ id: null, name: "Lint", command: "eslint {file}" });
});

test("an example's page comes filled in and saves nothing until saved", async () => {
  const p = ports([]);
  const store = new OneOffs(p);
  view = mount(TfCommandPage, { target: document.body, props: { at: { command: null, repo: null, on: null, example: 0 }, store, reads } });
  await settle();
  expect(document.querySelector<HTMLInputElement>("#tf-ocp-name")!.value).toBe("Ruff");
  expect(document.querySelector<HTMLInputElement>("#tf-ocp-files")!.value).toBe("*.py");
  expect(p.setCommands).not.toHaveBeenCalled();
});

test("a repository's own commands are copied after a look; one Save refuses cannot be ticked", async () => {
  const p = ports([]);
  const copy = vi.fn(() => Promise.resolve([ESLINT]));
  p.copyRepoCommands = copy;
  const store = new OneOffs(p);
  const { default: TfRepoCommands } = await import("../oneoff/TfRepoCommands.svelte");
  const base = { files: [], folders: [], has: [], repo_holds: [], stdin: "none" as const, shows: "auto" as const, start_in: "root" as const, timeout_secs: null, copied: null, theirs: null };
  view = mount(TfRepoCommands, {
    target: document.body,
    props: {
      entityId: "gitea:a/b",
      repo: "b",
      read: {
        file: "/c/b/.knobas/commands.toml",
        offered: [
          { ...base, name: "ESLint", command: "npx eslint {file}", kind: "file" as const, refused: null },
          { ...base, name: "Unsafe", command: "sh -c 'x {file}'", kind: "file" as const, refused: "{file} stands inside a quoted script" },
        ],
        failed: null,
      },
      store,
      onclose: () => {},
    },
  });
  flushSync();
  const ticks = [...document.querySelectorAll<HTMLInputElement>(".tf-oo-offer input[type=checkbox]")];
  expect(ticks.map((t) => t.disabled)).toEqual([false, true]);
  expect(words(document.querySelector(".tf-oo-offer"))).toContain("npx eslint {file}");
  ticks[0]!.click();
  flushSync();
  [...document.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === "Add to my commands")!.click();
  await settle();
  expect(copy).toHaveBeenCalledWith("gitea:a/b", ["ESLint"]);
});

test("a repository's Run… menu says what its file offers to add and what changed, never one Save refuses", async () => {
  const base = { files: [], folders: [], has: [], repo_holds: [], stdin: "none" as const, shows: "auto" as const, start_in: "root" as const, timeout_secs: null, kind: "repository" as const };
  const row = async (offered: OneOffRepoCommands["offered"]) => {
    const p = ports([]);
    p.repoCommands = () => Promise.resolve({ file: "/c/b/.knobas/commands.toml", offered, failed: null });
    const { default: TfRunMenu } = await import("../oneoff/TfRunMenu.svelte");
    const target = document.createElement("div");
    document.body.append(target);
    const menu = mount(TfRunMenu, { target, props: { entityId: "gitea:a/b", repo: "b", target: { kind: "repository" as const }, checkout: { path: "/c/b" } as never, store: new OneOffs(p) } });
    await settle();
    target.querySelector<HTMLButtonElement>(".tf-oo-runin button")!.click();
    await settle();
    const said = [...document.querySelectorAll("[role=menuitem]")].map(words).filter((w) => w.includes("this repository"));
    unmount(menu);
    target.remove();
    return said;
  };
  const unsafe = { ...base, name: "Unsafe", command: "sh -c 'x {dir}'", refused: "{dir} stands inside a quoted script", copied: null, theirs: null };
  expect(await row([unsafe])).toEqual([]);
  expect(await row([{ ...base, name: "Tests", command: "npm test", refused: null, copied: null, theirs: null }, unsafe])).toEqual(["1 command from this repository…"]);
  expect(await row([{ ...base, name: "Tests", command: "npm test", refused: null, copied: "tests", theirs: "npm run test" }, unsafe])).toEqual(["1 changed in this repository…"]);
  expect(
    await row([
      { ...base, name: "Tests", command: "npm test", refused: null, copied: "tests", theirs: "npm run test" },
      { ...base, name: "Build", command: "npm run build", refused: null, copied: null, theirs: null },
      { ...base, name: "Lint", command: "npm run lint", refused: null, copied: null, theirs: null },
    ]),
  ).toEqual(["2 commands from this repository, 1 changed in this repository…"]);
});

test("Try it on pasted text saves the command, then opens the run dialog on it, outside any repository (#2562)", async () => {
  const p = ports([]);
  const tried: [string, unknown][] = [];
  p.tryRun = (command, on) => (
    tried.push([command, on]),
    Promise.resolve({ started: { run: "r1", argv: ["eslint", "/tmp/knobas-try/command-1/a.ts"], cwd: "/tmp/knobas-try/command-1", branch: null, head: null, started_at: 0, already: false }, stop: () => {} })
  );
  const store = new OneOffs(p);
  await store.load();
  view = mount(TfCommandPage, { target: document.body, props: { at: { command: null, repo: null, on: null }, store, reads } });
  await settle();
  const set = (id: string, value: string) => {
    const field = document.querySelector<HTMLInputElement | HTMLTextAreaElement>(`#${id}`)!;
    field.value = value;
    field.dispatchEvent(new Event("input", { bubbles: true }));
    flushSync();
  };
  set("tf-ocp-name", "Lint");
  set("tf-ocp-cmd", "eslint {file}");
  // A file's command is tried on pasted text, a chosen file, or a repository's file; never a folder.
  const sources = [...document.querySelectorAll(".tf-try .tf-seg button")].map((b) => b.textContent);
  expect(sources).toEqual(["Pasted text", "A file", "A repository"]);
  set("tf-ocp-try-name", "a.ts");
  set("tf-ocp-try-text", "let x = 1\n");
  [...document.querySelectorAll<HTMLButtonElement>(".tf-try button")].find((b) => b.textContent?.trim() === "Try it")!.click();
  await settle();
  expect(p.setCommands, "saved first: only a saved command runs").toHaveBeenCalledTimes(1);
  expect(tried).toEqual([["new-0", { kind: "text", name: "a.ts", text: "let x = 1\n" }]]);
  expect(store.open?.entityId).toBe("~try");
  expect(store.open?.target).toEqual({ kind: "file", path: "a.ts", lines: null, at: null });
  expect(store.open?.cwd).toBe("/tmp/knobas-try/command-1");
  // A folder's command takes a folder or text instead.
  set("tf-ocp-cmd", "ruff check {dir}");
  expect([...document.querySelectorAll(".tf-try .tf-seg button")].map((b) => b.textContent)).toEqual(["A folder", "Pasted text", "A repository"]);
});
