/**
 * A source view drawn as tabs (#661, ADR-0020): the first section opens, a
 * count sits beside its title, a deferred tab reads its blocks once on first
 * open (rule 3), and every block draws its strings as text (rule 5).
 */
import { flushSync, mount, unmount } from "svelte";
import { beforeEach, expect, test, vi } from "vitest";

import type { SourceView as View, ViewAction, ViewSection } from "../../ipc/entity";
import type { BuildParameter } from "../../ipc/sources";
import type { SettingMark } from "./settings";

/** Every deferred read, as `[entity, section]`. */
const reads: [string, string | null][] = [];
let answer: (section: string | null) => Promise<View | null> = (section) =>
  Promise.resolve({ sections: [log(section ?? "log", false)] });

vi.mock("../../ipc/entity", () => ({
  entityView: (entityId: string, section: string | null) => {
    reads.push([entityId, section]);
    return answer(section);
  },
}));

const { default: SourceView } = await import("./SourceView.svelte");

function log(id: string, deferred: boolean): ViewSection {
  return {
    id,
    title: "Log",
    count: null,
    deferred,
    blocks: deferred ? [] : [{ type: "log", text: "[12:00] step 3 failed\n<i>exit 1</i>", truncated: true }],
  };
}

const VIEW: View = {
  sections: [
    {
      id: "overview",
      title: "Overview",
      count: null,
      deferred: false,
      blocks: [
        {
          type: "facts",
          facts: [
            { label: "Commit", value: "c90d11a3f5", tone: null },
            { label: "Outcome", value: "failed", tone: "err" },
          ],
        },
        {
          type: "entities",
          title: "Parent",
          rows: [
            {
              entity_id: "gitea:tidewater/payout-service@a41f2c",
              title: "a41f2c8b7d",
              meta: "PAY-231 backoff",
              tone: null,
              chip: "merged",
              url: null,
            },
            { entity_id: null, title: "not mirrored", meta: null, tone: null, chip: null, url: null },
          ],
        },
        { type: "text", text: "PAY-231: jitter\n\n<b>not markup</b>", markdown: false },
        {
          type: "settings",
          title: "Parameters",
          entity: "teamcity:buildType:X",
          write_op: "set_parameter",
          values: [
            { name: "env.RETRIES", value: "5", inherited: true, secret: false, editable: true, description: null },
            { name: "secure.token", value: "", inherited: false, secret: true, editable: false, description: null },
          ],
        },
      ],
    },
    { id: "diff", title: "Diff", count: 2, deferred: false, blocks: [{ type: "diff", files: [], truncated: false }] },
    log("log", true),
  ],
};

/** Every action the view asked to queue, as `[write_op, option]`. */
const queued: [string, string | null][] = [];
/** The values each queued action carried (#669), in the order queued. */
const carried: BuildParameter[][] = [];
/** What the next `onaction` answers: whether the write was queued. */
let queues: () => Promise<boolean> = () => Promise.resolve(true);
/** Every setting the view asked to save, as `[entity, write_op, name, value]`. */
const saved: [string, string, string, string][] = [];

function render(
  view: View = VIEW,
  writeOps: string[] = [],
  marks: Record<string, SettingMark> = {},
  entityId = "gitea:tidewater/payout-service@c90d11",
) {
  const target = document.createElement("div");
  document.body.append(target);
  const navigated: string[] = [];
  const app = mount(SourceView, {
    target,
    props: {
      entityId,
      view,
      webUrl: null,
      sourceName: "Tidewater Git",
      writeOps,
      onnavigate: (hash: string) => navigated.push(hash),
      onopenurl: () => {},
      onaction: (action: ViewAction, option: string | null, parameters: BuildParameter[] = []) => {
        queued.push([action.write_op, option]);
        carried.push(parameters);
        return queues();
      },
      settingMarks: marks,
      onsetting: (block: { entity: string; write_op: string }, name: string, value: string) => {
        saved.push([block.entity, block.write_op, name, value]);
        return Promise.resolve(true);
      },
    },
  });
  flushSync();
  return {
    target,
    navigated,
    tabs: () => [...target.querySelectorAll<HTMLButtonElement>("[role=tab]")],
    panel: () => target.querySelector<HTMLElement>("[role=tabpanel]")!,
    text: () => (target.textContent ?? "").replace(/\s+/g, " "),
    done: () => {
      unmount(app);
      target.remove();
    },
  };
}

beforeEach(() => {
  reads.length = 0;
  queued.length = 0;
  carried.length = 0;
  saved.length = 0;
  queues = () => Promise.resolve(true);
  document.body.replaceChildren();
  answer = (section) => Promise.resolve({ sections: [log(section ?? "log", false)] });
});

test("sections are tabs, the first is open, and a count sits by its title", () => {
  const screen = render();
  expect(screen.tabs().map((tab) => tab.textContent!.replace(/\s+/g, " ").trim())).toEqual([
    "Overview",
    "Diff 2",
    "Log",
  ]);
  expect(screen.tabs().map((tab) => tab.getAttribute("aria-selected"))).toEqual(["true", "false", "false"]);
  expect(screen.panel().textContent).toContain("c90d11a3f5");
  expect(reads).toEqual([]);

  screen.tabs()[1]!.click();
  flushSync();
  expect(screen.tabs()[1]!.getAttribute("aria-selected")).toBe("true");
  expect(screen.panel().querySelector(".df")).not.toBeNull();
  screen.done();
});

test("a deferred tab is read once, on its first opening", async () => {
  const screen = render();
  screen.tabs()[2]!.click();
  flushSync();
  expect(reads).toEqual([["gitea:tidewater/payout-service@c90d11", "log"]]);
  await vi.waitFor(() => expect(screen.panel().textContent).toContain("step 3 failed"));

  screen.tabs()[0]!.click();
  flushSync();
  screen.tabs()[2]!.click();
  flushSync();
  expect(reads, "the blocks are kept, not read again").toHaveLength(1);
  expect(screen.panel().textContent).toContain("<i>exit 1</i>");
  expect(screen.panel().querySelector("i")).toBeNull();
  screen.done();
});

test("a view that opens on a deferred section reads it, once", async () => {
  const screen = render({ sections: [log("log", true), log("tail", false)] });
  expect(reads).toEqual([["gitea:tidewater/payout-service@c90d11", "log"]]);
  await vi.waitFor(() => expect(screen.panel().textContent).toContain("step 3 failed"));
  screen.tabs()[1]!.click();
  flushSync();
  screen.tabs()[0]!.click();
  flushSync();
  expect(reads, "the blocks are kept, not read again").toHaveLength(1);
  screen.done();
});

test("a deferred tab that fails says why and can be read again", async () => {
  let fail = true;
  answer = (section) =>
    fail
      ? Promise.reject({ code: "unreachable", message: "Gitea answered 502", source_id: null })
      : Promise.resolve({ sections: [log(section ?? "log", false)] });
  const screen = render();
  screen.tabs()[2]!.click();
  flushSync();
  await vi.waitFor(() => expect(screen.panel().textContent).toContain("Gitea answered 502"));

  fail = false;
  [...screen.panel().querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent!.includes("Try again"))!.click();
  await vi.waitFor(() => expect(screen.panel().textContent).toContain("step 3 failed"));
  expect(reads).toHaveLength(2);
  screen.done();
});

test("facts, rows, text and settings are drawn as text, with tones and addresses", () => {
  const screen = render();
  const panel = screen.panel();
  expect(panel.textContent).toContain("<b>not markup</b>");
  expect(panel.querySelector("b")).toBeNull();

  const failed = [...panel.querySelectorAll(".fb-v")].find((v) => v.textContent === "failed")!;
  expect(failed.classList.contains("fail"), "err is the theme's fail colour").toBe(true);

  // A mirrored row is an address; one the mirror lacks is plain text.
  const open = [...panel.querySelectorAll<HTMLButtonElement>(".eb-row button")];
  expect(open.map((b) => b.textContent!.trim())).toEqual(["a41f2c8b7d"]);
  open[0]!.click();
  expect(screen.navigated).toEqual(["#/entity/gitea:tidewater%2Fpayout-service%40a41f2c"]);
  expect(panel.textContent).toContain("not mirrored");
  expect(panel.textContent).toContain("merged");

  // A secret value is never drawn, and an inherited one says so.
  const settings = panel.querySelector(".sb")!.textContent!.replace(/\s+/g, " ");
  expect(settings).toContain("env.RETRIES");
  expect(settings).toContain("inherited");
  expect(settings).toContain("hidden");
  screen.done();
});

test("arrow keys move between tabs", () => {
  const screen = render();
  screen.tabs()[0]!.focus();
  screen.tabs()[0]!.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true }));
  flushSync();
  expect(screen.tabs()[1]!.getAttribute("aria-selected")).toBe("true");
  expect(document.activeElement).toBe(screen.tabs()[1]);
  screen.done();
});

const MERGE: ViewAction = {
  write_op: "merge_pull_request",
  label: "Merge…",
  options: ["merge", "rebase", "squash"],
  confirm: "Merge <b>feature/x</b> into main?",
};

function withActions(...actions: ViewAction[]): View {
  return {
    sections: [
      {
        id: "overview",
        title: "Overview",
        count: null,
        deferred: false,
        blocks: [
          { type: "facts", facts: [{ label: "State", value: "open", tone: "info" }] },
          { type: "actions", actions },
        ],
      },
    ],
  };
}

function buttons(root: ParentNode): HTMLButtonElement[] {
  return [...root.querySelectorAll<HTMLButtonElement>("button")];
}

function button(root: ParentNode, text: string): HTMLButtonElement | undefined {
  return buttons(root).find((b) => b.textContent!.replace(/\s+/g, " ").trim() === text);
}

test("an action is drawn only where the source declares its op", () => {
  const undeclared = render(withActions(MERGE), ["comment", "approve"]);
  expect(button(undeclared.panel(), "Merge…")).toBeUndefined();
  expect(undeclared.panel().querySelector(".ab"), "no empty strip either").toBeNull();
  undeclared.done();

  const declared = render(withActions(MERGE), ["comment", "merge_pull_request"]);
  expect(button(declared.panel(), "Merge…")).toBeDefined();
  declared.done();
});

test("an action opens its dialog, the first option picked, and queues the one chosen", async () => {
  const screen = render(withActions(MERGE), ["merge_pull_request"]);
  button(screen.panel(), "Merge…")!.click();
  flushSync();

  const dialog = document.querySelector<HTMLElement>("[role=dialog]")!;
  expect(dialog).not.toBeNull();
  // The sentence is source text: drawn, never rendered.
  expect(dialog.textContent).toContain("Merge <b>feature/x</b> into main?");
  expect(dialog.querySelector("b")).toBeNull();
  const radios = [...dialog.querySelectorAll<HTMLButtonElement>("[role=radio]")];
  expect(radios.map((r) => r.textContent!.trim())).toEqual(["merge", "rebase", "squash"]);
  expect(radios.map((r) => r.getAttribute("aria-checked"))).toEqual(["true", "false", "false"]);
  expect(queued, "nothing is queued by opening the dialog").toEqual([]);

  radios[2]!.click();
  flushSync();
  expect(radios.map((r) => r.getAttribute("aria-checked"))).toEqual(["false", "false", "true"]);
  button(dialog, "Merge")!.click();
  flushSync();
  expect(queued).toEqual([["merge_pull_request", "squash"]]);
  await vi.waitFor(() => expect(document.querySelector("[role=dialog]")).toBeNull());
  screen.done();
});

test("a write that could not be queued keeps the dialog open; cancelling queues nothing", async () => {
  queues = () => Promise.resolve(false);
  const screen = render(withActions(MERGE), ["merge_pull_request"]);
  button(screen.panel(), "Merge…")!.click();
  flushSync();
  button(document.querySelector("[role=dialog]")!, "Merge")!.click();
  await new Promise((settle) => setTimeout(settle, 0));
  flushSync();
  expect(queued).toEqual([["merge_pull_request", "merge"]]);
  expect(document.querySelector("[role=dialog]"), "still open to try again").not.toBeNull();

  button(document.querySelector("[role=dialog]")!, "Cancel")!.click();
  flushSync();
  expect(document.querySelector("[role=dialog]")).toBeNull();
  expect(queued).toHaveLength(1);
  screen.done();
});

test("an action with no options and no sentence queues on the button", () => {
  const run: ViewAction = { write_op: "merge_pull_request", label: "Merge", options: [], confirm: null };
  const screen = render(withActions(run), ["merge_pull_request"]);
  button(screen.panel(), "Merge")!.click();
  flushSync();
  expect(document.querySelector("[role=dialog]")).toBeNull();
  expect(queued).toEqual([["merge_pull_request", null]]);
  screen.done();
});

test("a view read again drops the deferred tabs it had read, and reads the open one afresh", async () => {
  const target = document.createElement("div");
  document.body.append(target);
  const props = $state({
    entityId: "gitea:tidewater/payout-service#142",
    view: { sections: [log("log", true)] } as View,
    webUrl: null,
    sourceName: "Tidewater Git",
    writeOps: [] as string[],
    onnavigate: () => {},
    onopenurl: () => {},
    onaction: () => Promise.resolve(true),
  });
  const app = mount(SourceView, { target, props });
  flushSync();
  await vi.waitFor(() => expect(target.textContent).toContain("step 3 failed"));
  expect(reads).toHaveLength(1);

  props.view = { sections: [log("log", true)] };
  flushSync();
  await vi.waitFor(() => expect(reads).toHaveLength(2));
  unmount(app);
  target.remove();
});

// --- Settings (#668) ------------------------------------------------------

function settingsCell(target: HTMLElement, name: string): HTMLElement {
  const row = [...target.querySelectorAll<HTMLElement>(".sb-row")].find(
    (r) => r.querySelector(".sb-name")!.textContent!.trim() === name,
  )!;
  return row.querySelector<HTMLElement>(".sb-val")!;
}

test("a settings value is editable only where the source declares the block's op", () => {
  const undeclared = render(VIEW, ["trigger_build"]);
  expect(settingsCell(undeclared.target, "env.RETRIES").tagName).toBe("SPAN");
  undeclared.done();

  const declared = render(VIEW, ["trigger_build", "set_parameter"]);
  expect(settingsCell(declared.target, "env.RETRIES").tagName).toBe("BUTTON");
  // The adapter said no for this one, and the declaration does not override it.
  expect(settingsCell(declared.target, "secure.token").tagName).toBe("SPAN");
  declared.done();
});

test("an edit is handed up with the block's entity and op, and a mark comes back down", async () => {
  const marks = { "env.RETRIES": { state: "pending" as const, detail: null, value: "9" } };
  // A mark is this entity's: a settings block of another entity does not draw it.
  const other = render(VIEW, ["set_parameter"], marks);
  expect(other.text()).not.toContain("pending");
  other.done();
  const screen = render(VIEW, ["set_parameter"], marks, "teamcity:buildType:X");
  expect(screen.text()).toContain("pending");
  settingsCell(screen.target, "env.RETRIES").click();
  flushSync();
  const input = screen.target.querySelector<HTMLInputElement>("input.sb-input")!;
  input.value = "7";
  input.dispatchEvent(new Event("input", { bubbles: true }));
  input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  await Promise.resolve();
  expect(saved).toEqual([["teamcity:buildType:X", "set_parameter", "env.RETRIES", "7"]]);
  screen.done();
});

// --- Run… (#669) ------------------------------------------------------------

const CONFIG = "teamcity:buildType:Payout_IntegrationTests";

/** A configuration's view: *Run…* on the overview, its parameters on their own tab. */
const RUN_VIEW: View = {
  sections: [
    {
      id: "overview",
      title: "Overview",
      count: null,
      deferred: false,
      blocks: [
        { type: "facts", facts: [{ label: "Project", value: "Payout", tone: null }] },
        {
          type: "actions",
          actions: [
            {
              write_op: "run_build",
              label: "Run…",
              options: ["feature/PAY-231-sepa-retry", "fix/<b>PAY-228</b>"],
              confirm: "Queue one build of this configuration.",
            },
          ],
        },
      ],
    },
    {
      id: "parameters",
      title: "Parameters",
      count: 4,
      deferred: false,
      blocks: [
        {
          type: "settings",
          title: null,
          entity: CONFIG,
          write_op: "set_parameter",
          values: [
            { name: "env.SEPA_RETRIES", value: "5", inherited: false, secret: false, editable: true, description: null },
            { name: "env.SEPA_SANDBOX_KEY", value: "", inherited: false, secret: true, editable: true, description: null },
            { name: "tidewater.region", value: "eu-central-1", inherited: true, secret: false, editable: true, description: null },
            { name: "vcs.mirror", value: "https://git.example/p.git", inherited: false, secret: false, editable: false, description: null },
          ],
        },
      ],
    },
  ],
};

function dialog(): HTMLElement {
  return document.querySelector<HTMLElement>("[role=dialog]")!;
}

function field(name: string): HTMLInputElement | null {
  return dialog().querySelector<HTMLInputElement>(`input[aria-label="${name}"]`);
}

function type(input: HTMLInputElement, value: string) {
  input.value = value;
  input.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
}

test("Run… offers a branch with its suggestions and the parameters as they stand; untouched, it sends neither", async () => {
  const screen = render(RUN_VIEW, ["run_build"], {}, CONFIG);
  button(screen.panel(), "Run…")!.click();
  flushSync();

  const branch = field("Branch")!;
  expect(branch.value, "the default branch is the empty field").toBe("");
  expect(branch.placeholder).toBe("default branch");
  const list = document.getElementById(branch.getAttribute("list")!)!;
  // Suggestions are source text: values, never markup.
  expect([...list.querySelectorAll("option")].map((o) => o.value)).toEqual([
    "feature/PAY-231-sepa-retry",
    "fix/<b>PAY-228</b>",
  ]);
  expect(dialog().querySelector("[role=radio]"), "a typed branch, not a pick").toBeNull();

  expect(field("env.SEPA_RETRIES")!.value).toBe("5");
  expect(field("tidewater.region")!.value).toBe("eu-central-1");
  const secret = field("env.SEPA_SANDBOX_KEY")!;
  expect(secret.type).toBe("password");
  expect(secret.value, "a secret is never prefilled").toBe("");
  expect(field("vcs.mirror"), "a value the source does not let change is shown, not offered").toBeNull();
  expect(dialog().textContent).toContain("https://git.example/p.git");

  button(dialog(), "Run")!.click();
  flushSync();
  expect(queued).toEqual([["run_build", ""]]);
  expect(carried).toEqual([[]]);
  await vi.waitFor(() => expect(document.querySelector("[role=dialog]")).toBeNull());
  screen.done();
});

test("Run… sends the branch typed and only the values changed, a secret only when typed", () => {
  const screen = render(RUN_VIEW, ["run_build"], {}, CONFIG);
  button(screen.panel(), "Run…")!.click();
  flushSync();

  type(field("Branch")!, "fix/PAY-228-partial-refund-drift");
  type(field("tidewater.region")!, "eu-west-1");
  type(field("env.SEPA_RETRIES")!, "5");
  expect(dialog().textContent).toContain("1 changed");
  type(field("env.SEPA_SANDBOX_KEY")!, "typed-key");
  expect(dialog().textContent).toContain("2 changed");

  button(dialog(), "Run")!.click();
  flushSync();
  expect(queued).toEqual([["run_build", "fix/PAY-228-partial-refund-drift"]]);
  expect(carried).toEqual([
    [
      { name: "env.SEPA_SANDBOX_KEY", value: "typed-key" },
      { name: "tidewater.region", value: "eu-west-1" },
    ],
  ]);
  screen.done();
});

test("a run offers only this entity's own settings, and a dialog opened again starts afresh", () => {
  const other = render(RUN_VIEW, ["run_build"], {}, "teamcity:buildType:Ledger_Deploy_Staging");
  button(other.panel(), "Run…")!.click();
  flushSync();
  expect(field("env.SEPA_RETRIES")).toBeNull();
  other.done();

  queues = () => Promise.resolve(true);
  const screen = render(RUN_VIEW, ["run_build"], {}, CONFIG);
  button(screen.panel(), "Run…")!.click();
  flushSync();
  type(field("env.SEPA_RETRIES")!, "9");
  type(field("Branch")!, "feature/x");
  button(dialog(), "Cancel")!.click();
  flushSync();
  button(screen.panel(), "Run…")!.click();
  flushSync();
  expect(field("env.SEPA_RETRIES")!.value).toBe("5");
  expect(field("Branch")!.value).toBe("");
  screen.done();
});
