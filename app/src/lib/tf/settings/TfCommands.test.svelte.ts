/** Settings › Commands and a command's page (ADR-0060, section 27). */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, expect, test, vi } from "vitest";

import type { OneOff, OneOffDraft } from "../../ipc/oneoff";
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
