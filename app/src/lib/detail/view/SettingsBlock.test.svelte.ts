/**
 * The settings block (#664, ADR-0020): one row per value -- name, value, and
 * a mark for an inherited one. A secret's value is never drawn (the adapter
 * sends none), and every string is source text, never markup (rule 5).
 */
import { flushSync, mount, tick, unmount } from "svelte";
import { expect, test } from "vitest";

import type { Setting } from "../../ipc/entity";
import SettingsBlock from "./SettingsBlock.svelte";
import type { SettingMark } from "./settings";

function row(target: HTMLElement, name: string): HTMLElement {
  const found = [...target.querySelectorAll<HTMLElement>(".sb-row")].find(
    (r) => r.querySelector(".sb-name")!.textContent!.trim() === name,
  );
  if (!found) throw new Error(`no row ${name}`);
  return found;
}

function key(element: HTMLElement, key: string) {
  element.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true }));
  flushSync();
}

function type(input: HTMLInputElement, value: string) {
  input.value = value;
  input.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
}

function setting(over: Partial<Setting> & { name: string }): Setting {
  return { value: "", inherited: false, secret: false, editable: false, description: null, ...over };
}

interface Options {
  title?: string | null;
  writable?: boolean;
  marks?: Record<string, SettingMark>;
  onsave?: (name: string, value: string, base: string | null) => Promise<boolean>;
}

function render(values: Setting[], titleOrOptions: string | null | Options = null) {
  const options: Options =
    typeof titleOrOptions === "object" && titleOrOptions !== null ? titleOrOptions : { title: titleOrOptions };
  const target = document.createElement("div");
  document.body.append(target);
  const props = $state({
    title: options.title ?? null,
    values,
    writable: options.writable ?? false,
    marks: options.marks ?? {},
    onsave: options.onsave ?? (() => Promise.resolve(true)),
  });
  const app = mount(SettingsBlock, { target, props });
  flushSync();
  return {
    target,
    props,
    /** The value cell of the row called `name`: a button when editable. */
    cell: (name: string) => row(target, name).querySelector<HTMLElement>(".sb-val")!,
    input: () => target.querySelector<HTMLInputElement>("input.sb-input"),
    mark: (name: string) => row(target, name).querySelector(".sb-mark")!.textContent!.trim(),
    rows: () =>
      [...target.querySelectorAll<HTMLElement>(".sb-row")].map((row) => ({
        name: row.querySelector(".sb-name")!.textContent!.trim(),
        value: row.querySelector(".sb-val")!.textContent!.trim(),
        mark: row.querySelector(".sb-mark")!.textContent!.trim(),
      })),
    done: () => {
      unmount(app);
      target.remove();
    },
  };
}

test("each value is a row of name, value and mark; an inherited one says so", () => {
  const view = render([
    setting({ name: "env.SEPA_RETRIES", value: "5", description: "how often" }),
    setting({ name: "tidewater.region", value: "eu-central-1", inherited: true }),
  ]);
  expect(view.rows()).toEqual([
    { name: "env.SEPA_RETRIES", value: "5", mark: "" },
    { name: "tidewater.region", value: "eu-central-1", mark: "inherited" },
  ]);
  expect(view.target.querySelector(".sb-name")!.getAttribute("title")).toBe("how often");
  expect(view.target.querySelector(".sb-row.inh .sb-name")!.textContent).toBe("tidewater.region");
  view.done();
});

test("a secret draws `hidden`, never a value, even one that arrived", () => {
  const view = render([setting({ name: "env.SEPA_SANDBOX_KEY", value: "leaked-by-mistake", secret: true })]);
  expect(view.rows()).toEqual([{ name: "env.SEPA_SANDBOX_KEY", value: "hidden", mark: "" }]);
  expect(view.target.textContent).not.toContain("leaked-by-mistake");
  view.done();
});

test("values are text, not markup", () => {
  const view = render([setting({ name: "<b>n</b>", value: "<img src=x onerror=alert(1)>" })]);
  expect(view.target.querySelector("b, img")).toBeNull();
  expect(view.rows()[0]).toMatchObject({ name: "<b>n</b>", value: "<img src=x onerror=alert(1)>" });
  view.done();
});

test("a title heads the block with the count; none draws no header", () => {
  const titled = render([setting({ name: "a" })], "Parameters");
  expect(titled.target.querySelector(".sec-h")!.textContent).toContain("Parameters");
  titled.done();
  const bare = render([setting({ name: "a" })]);
  expect(bare.target.querySelector(".sec-h")).toBeNull();
  bare.done();
});

// --- Editing (#668) -------------------------------------------------------

test("a non-editable row has no control, and neither has a block the shell cannot write", () => {
  const readOnly = render([setting({ name: "build.counter", value: "12" })], { writable: true });
  expect(readOnly.cell("build.counter").tagName).toBe("SPAN");
  expect(readOnly.target.querySelector("button, input")).toBeNull();
  readOnly.done();

  // Editable by the adapter's word, but the source does not declare the op.
  const undeclared = render([setting({ name: "env.SEPA_RETRIES", value: "5", editable: true })], { writable: false });
  expect(undeclared.target.querySelector("button, input")).toBeNull();
  undeclared.done();
});

test("an editable value opens in place, pre-filled; Enter saves the new value", async () => {
  const saved: [string, string, string | null][] = [];
  const view = render([setting({ name: "env.SEPA_RETRIES", value: "5", editable: true })], {
    writable: true,
    onsave: (name, value, base) => {
      saved.push([name, value, base]);
      return Promise.resolve(true);
    },
  });
  const cell = view.cell("env.SEPA_RETRIES");
  expect(cell.tagName).toBe("BUTTON");
  expect(cell.getAttribute("aria-label")).toBe("Edit env.SEPA_RETRIES");
  cell.click();
  flushSync();
  await tick();
  const input = view.input()!;
  expect(input.value).toBe("5");
  expect(input.type).toBe("text");
  expect(document.activeElement).toBe(input);
  type(input, "7");
  key(input, "Enter");
  await Promise.resolve();
  flushSync();
  // With the value the edit started from, which the source checks.
  expect(saved).toEqual([["env.SEPA_RETRIES", "7", "5"]]);
  expect(view.input()).toBeNull();
  view.done();
});

test("Esc cancels without a write, and an unchanged value saves nothing", () => {
  const saved: string[] = [];
  const view = render([setting({ name: "env.SEPA_RETRIES", value: "5", editable: true })], {
    writable: true,
    onsave: (_name, value) => {
      saved.push(value);
      return Promise.resolve(true);
    },
  });
  view.cell("env.SEPA_RETRIES").click();
  flushSync();
  type(view.input()!, "9");
  key(view.input()!, "Escape");
  expect(view.input()).toBeNull();
  expect(view.cell("env.SEPA_RETRIES").textContent!.trim()).toBe("5");

  view.cell("env.SEPA_RETRIES").click();
  flushSync();
  key(view.input()!, "Enter");
  expect(view.input()).toBeNull();
  expect(saved).toEqual([]);
  view.done();
});

test("a secret is never pre-filled, and is typed into a password field", () => {
  const view = render(
    [setting({ name: "env.SEPA_SANDBOX_KEY", value: "leaked-by-mistake", secret: true, editable: true })],
    { writable: true },
  );
  view.cell("env.SEPA_SANDBOX_KEY").click();
  flushSync();
  const input = view.input()!;
  expect(input.value).toBe("");
  expect(input.type).toBe("password");
  expect(view.target.textContent).not.toContain("leaked-by-mistake");
  expect(view.target.querySelector(".sb-hint")!.textContent).toContain("current value is not shown");
  view.done();
});

test("an inherited value says, before saving, that the edit overrides it", () => {
  const view = render([setting({ name: "tidewater.region", value: "eu-central-1", inherited: true, editable: true })], {
    writable: true,
  });
  view.cell("tidewater.region").click();
  flushSync();
  expect(view.target.querySelector(".sb-hint")!.textContent).toContain("Overrides the inherited value");
  view.done();
});

test("a save the shell could not queue keeps the editor open with what was typed", async () => {
  const view = render([setting({ name: "env.SEPA_RETRIES", value: "5", editable: true })], {
    writable: true,
    onsave: () => Promise.resolve(false),
  });
  view.cell("env.SEPA_RETRIES").click();
  flushSync();
  type(view.input()!, "7");
  key(view.input()!, "Enter");
  await Promise.resolve();
  flushSync();
  expect(view.input()!.value).toBe("7");
  view.done();
});

test("a queued write marks its row until it settles, and shows the value it carries", () => {
  const view = render(
    [
      setting({ name: "env.SEPA_RETRIES", value: "5", editable: true }),
      setting({ name: "env.SEPA_SANDBOX_KEY", secret: true, editable: true }),
      setting({ name: "tidewater.region", value: "eu-central-1", inherited: true, editable: true }),
    ],
    {
      writable: true,
      marks: {
        "env.SEPA_RETRIES": { state: "pending", detail: "TeamCity is unreachable", value: "7" },
        "env.SEPA_SANDBOX_KEY": { state: "held", detail: null, value: "new-secret" },
      },
    },
  );
  expect(view.mark("env.SEPA_RETRIES")).toBe("pending");
  expect(view.cell("env.SEPA_RETRIES").textContent!.trim()).toBe("7");
  expect(row(view.target, "env.SEPA_RETRIES").querySelector(".sb-mark")!.getAttribute("title")).toBe(
    "TeamCity is unreachable",
  );
  expect(view.mark("env.SEPA_SANDBOX_KEY")).toBe("held");
  expect(view.target.textContent).not.toContain("new-secret");
  expect(view.mark("tidewater.region")).toBe("inherited");

  view.props.marks = {};
  flushSync();
  expect(view.mark("env.SEPA_RETRIES")).toBe("");
  expect(view.cell("env.SEPA_RETRIES").textContent!.trim()).toBe("5");
  view.done();
});
