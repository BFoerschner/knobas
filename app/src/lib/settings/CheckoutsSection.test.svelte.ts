/**
 * The clones-root and open-command settings sections (issues #499 and #501).
 *
 * The seam every other settings section is tested at: a rendered section in,
 * user-visible text and bridge calls out. What the backend stores is
 * `crates/knobas-app/tests/it/checkout_ipc.rs`'s, and which templates are legal
 * is `knobas_core::checkout`'s; what is asserted here is that each field draws
 * what is **stored**, that Save and Clear and Reset send what was asked for,
 * and that a read or a write it could not make says so instead of drawing a
 * state it has not earned.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { CheckoutCommand } from "../ipc/entity";
import CheckoutsSection from "./CheckoutsSection.svelte";

const VSCODE = 'open -a "Visual Studio Code" {path}';
const JETBRAINS = 'open -a "IntelliJ IDEA" {path}';
const TERMINAL = "open -a Terminal {path}";

/** The three actions as a Mac answers them with nothing stored. */
function defaults(): CheckoutCommand[] {
  return [
    { action: "vscode", label: "Open in VS Code", template: VSCODE, is_default: true },
    { action: "jetbrains", label: "Open in JetBrains", template: JETBRAINS, is_default: true },
    { action: "terminal", label: "Open terminal here", template: TERMINAL, is_default: true },
  ];
}

let target: HTMLDivElement;
let app: Record<string, unknown> | undefined;

/**
 * Mount the section over a bridge that **remembers** what it was sent, so a
 * write followed by the section's own re-read draws what is now stored. A
 * fixture that answered the same thing after a write could not tell a section
 * that re-reads from one that keeps what was typed, which is the property the
 * Save test is about.
 */
function render(
  ports: {
    clonesRoot?: () => Promise<string | null>;
    setClonesRoot?: (path: string | null) => Promise<void>;
    checkoutCommands?: () => Promise<CheckoutCommand[]>;
    setCheckoutCommand?: (action: string, template: string | null) => Promise<CheckoutCommand[]>;
  } = {},
  initial: string | null = "/Users/mara/src",
  commands: CheckoutCommand[] = defaults(),
) {
  const sent: (string | null)[] = [];
  const sentCommands: { action: string; template: string | null }[] = [];
  let stored: string | null = initial;
  // The command fixture remembers too, for the reason the root's does: a
  // section that re-reads after a write and one that keeps what was typed draw
  // the same thing against a bridge that answers the same list either way.
  let storedCommands = commands;
  app = mount(CheckoutsSection, {
    target,
    props: {
      ports: {
        clonesRoot: () => Promise.resolve(stored),
        setClonesRoot: (path: string | null) => {
          sent.push(path);
          stored = path;
          return Promise.resolve();
        },
        checkoutCommands: () => Promise.resolve(storedCommands),
        setCheckoutCommand: (action: string, template: string | null) => {
          sentCommands.push({ action, template });
          storedCommands = storedCommands.map((command) =>
            command.action === action
              ? {
                  ...command,
                  template: template ?? defaults().find((d) => d.action === action)!.template,
                  is_default: template === null,
                }
              : command,
          );
          return Promise.resolve(storedCommands);
        },
        ...ports,
      },
    },
  });
  flushSync();
  return { sent, sentCommands };
}

async function settle() {
  for (let i = 0; i < 4; i += 1) await Promise.resolve();
  flushSync();
}

function text(): string {
  return (target.textContent ?? "").replace(/\s+/g, " ").trim();
}

function field(): HTMLInputElement {
  const input = target.querySelector<HTMLInputElement>("#clones-root");
  if (!input) throw new Error(`no clones-root field: ${text()}`);
  return input;
}

function button(label: string, within: ParentNode = target): HTMLButtonElement {
  const found = [...within.querySelectorAll("button")].find(
    (element) => (element.textContent ?? "").trim() === label,
  );
  if (!found) throw new Error(`no button reading ${label}: ${text()}`);
  return found as HTMLButtonElement;
}

/** One command's input, and the block it and its buttons live in (#501). */
function command(action: string): { input: HTMLInputElement; block: HTMLElement } {
  const input = target.querySelector<HTMLInputElement>(`#checkout-command-${action}`);
  if (!input) throw new Error(`no field for ${action}: ${text()}`);
  const block = input.closest(".field");
  if (!block) throw new Error(`the ${action} field is not in a block: ${text()}`);
  return { input, block: block as HTMLElement };
}

/** Type into a field the way a person does, so `bind:value` sees it. */
function type(input: HTMLInputElement, value: string) {
  input.value = value;
  input.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
}

beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
});

afterEach(() => {
  if (app) unmount(app);
  app = undefined;
  target.remove();
});

test("the field draws the stored root", async () => {
  render();
  await settle();
  expect(field().value).toBe("/Users/mara/src");
  // The section says what knobas will not do with the directory, which is the
  // question a person has about handing one over (ADR-0016).
  expect(text()).toContain("never clones, checks out or fetches");
});

test("an unset root says so, and offers no Clear", async () => {
  render({}, null);
  await settle();
  expect(field().value).toBe("");
  expect(text()).toContain("Not set");
  expect(() => button("Clear")).toThrow();
});

test("Save sends the typed path and the field then draws what is stored", async () => {
  const { sent } = render({}, null);
  await settle();

  const input = field();
  input.value = "/Users/mara/code";
  input.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  button("Save").click();
  await settle();

  expect(sent).toEqual(["/Users/mara/code"]);
  expect(field().value).toBe("/Users/mara/code");
  expect(text()).not.toContain("Not set");
});

test("Clear unsets it, and the section goes back to saying nothing is looked for", async () => {
  const { sent } = render();
  await settle();

  button("Clear").click();
  await settle();

  expect(sent).toEqual([null]);
  expect(field().value).toBe("");
  expect(text()).toContain("Not set");
});

test("a read that failed says so rather than drawing an empty field", async () => {
  render({ clonesRoot: () => Promise.reject(new Error("the database is still starting")) });
  await settle();

  expect(text()).toContain("the database is still starting");
  // Not *Not set*: "no root" is a claim about what is stored, and a section
  // that could not ask has not earned it.
  expect(text()).not.toContain("Not set");
});

// -- the open commands (#501) ------------------------------------------------

test("each action draws the command it would run, and says it is the default", async () => {
  render();
  await settle();

  expect(command("vscode").input.value).toBe(VSCODE);
  expect(command("jetbrains").input.value).toBe(JETBRAINS);
  expect(command("terminal").input.value).toBe(TERMINAL);
  // Three of them, one per action: a default nobody has changed is still a
  // default, and the section says so rather than looking like a stored value.
  expect(text().match(/The default on this platform\./g)).toHaveLength(3);
  // The rule the placeholder exists for, in the words the ADR uses: this is
  // the one place a person is told what a command knobas runs can and cannot
  // be handed, and it is the sentence ADR-0016 is about.
  expect(text()).toContain(
    "{path} stands for the checkout, and it is the only thing knobas fills in",
  );
  expect(text()).toContain("nothing a source mirrored");
  expect(text()).toContain("No shell is involved");
});

test("an action with no template says it is not configured", async () => {
  render({}, "/Users/mara/src", [
    { action: "vscode", label: "Open in VS Code", template: null, is_default: true },
    { action: "jetbrains", label: "Open in JetBrains", template: null, is_default: true },
    { action: "terminal", label: "Open terminal here", template: null, is_default: true },
  ]);
  await settle();

  expect(command("vscode").input.value).toBe("");
  expect(text()).toContain("Not configured");
  // Nothing to go back to, so nothing offers to.
  expect(() => button("Reset", command("vscode").block)).toThrow();
});

test("Save sends the typed template, and the field then draws what is stored", async () => {
  const { sentCommands } = render();
  await settle();

  const { input, block } = command("terminal");
  type(input, "open -a Ghostty {path}");
  button("Save", block).click();
  await settle();

  expect(sentCommands).toEqual([{ action: "terminal", template: "open -a Ghostty {path}" }]);
  expect(command("terminal").input.value).toBe("open -a Ghostty {path}");
  // No longer the platform's, so the section stops calling it that and offers
  // the way back.
  expect(text().match(/The default on this platform\./g)).toHaveLength(2);
  expect(button("Reset", command("terminal").block)).toBeTruthy();
});

test("Reset forgets the template and the default comes back", async () => {
  const { sentCommands } = render({}, "/Users/mara/src", [
    { action: "vscode", label: "Open in VS Code", template: "code {path}", is_default: false },
    { action: "jetbrains", label: "Open in JetBrains", template: JETBRAINS, is_default: true },
    { action: "terminal", label: "Open terminal here", template: TERMINAL, is_default: true },
  ]);
  await settle();

  button("Reset", command("vscode").block).click();
  await settle();

  expect(sentCommands).toEqual([{ action: "vscode", template: null }]);
  expect(command("vscode").input.value).toBe(VSCODE);
});

test("a refused template says why, and the field keeps what was typed", async () => {
  const refused: string[] = [];
  render({
    setCheckoutCommand: (action: string, template: string | null) => {
      refused.push(`${action}=${template}`);
      return Promise.reject({
        code: "invalid",
        message: `'${template}' cannot be run: {repo_url} is not something knobas fills in`,
        source_id: null,
      });
    },
  });
  await settle();

  type(command("vscode").input, "code {repo_url}");
  button("Save", command("vscode").block).click();
  await settle();

  expect(refused).toEqual(["vscode=code {repo_url}"]);
  expect(text()).toContain("{repo_url} is not something knobas fills in");
  // Nothing was stored, so nothing is re-read: the field keeps the string the
  // reader has to correct rather than snapping back to what it was.
  expect(command("vscode").input.value).toBe("code {repo_url}");
});

test("a read that failed says so rather than drawing three empty fields", async () => {
  render({ checkoutCommands: () => Promise.reject(new Error("the database is still starting")) });
  await settle();

  expect(text()).toContain("the database is still starting");
  expect(target.querySelector("#checkout-command-vscode")).toBeNull();
});
