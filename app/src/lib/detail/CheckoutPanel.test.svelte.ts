/**
 * The checkout panel on a repo and a branch detail (issue #499).
 *
 * The seam is the rendered panel: a `CheckoutView` in, user-visible text and
 * bridge calls out. What the backend does to produce that view is
 * `crates/knobas-app/tests/it/checkout_ipc.rs`'s, and whether a directory really
 * holds a clone is `knobas-core`'s; what is asserted here is that the three
 * states each say what they are, that *no checkout* offers the clone command,
 * and that setting and clearing a path go through `set_checkout_override`.
 *
 * The panel reads through the module import rather than a `ports` prop -- the
 * detail's own components do -- so the bridge is mocked here.
 *
 * The open buttons (#501) are asserted here too, and one thing about them is
 * asserted *only* here: the failure message names the template, because the
 * panel composes that sentence itself out of what `checkout_commands`
 * answered. A test that read the template out of the rejection would be
 * asserting its own mock.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { toasts } from "../shell/toasts.svelte";
import type { CheckoutCommand, CheckoutView } from "../ipc/entity";

const reads: string[] = [];
const writes: { entityId: string; path: string | null }[] = [];
const opens: { entityId: string; action: string }[] = [];
let answer: CheckoutView;
let afterWrite: CheckoutView | null = null;
/** Set by the one test about a refused read; cleared in `beforeEach`. */
let refuseRead = false;
/** The action whose spawn refuses, for the failure-message test. */
let refuseOpen: string | null = null;
/** Set by the one test about a refused commands read. */
let refuseCommands = false;
/** How many times the panel has asked for the open actions. */
let commandReads = 0;
let commands: CheckoutCommand[];

const VSCODE = 'open -a "Visual Studio Code" {path}';

/** The three actions, as `checkout_commands` answers them on a Mac. */
function configured(): CheckoutCommand[] {
  return [
    { action: "vscode", label: "Open in VS Code", template: VSCODE, is_default: true },
    {
      action: "jetbrains",
      label: "Open in JetBrains",
      template: 'open -a "IntelliJ IDEA" {path}',
      is_default: true,
    },
    {
      action: "terminal",
      label: "Open terminal here",
      template: "open -a Terminal {path}",
      is_default: true,
    },
  ];
}

vi.mock("../ipc/entity", () => ({
  entityCheckout: (entityId: string) => {
    reads.push(entityId);
    if (refuseRead) {
      return Promise.reject({
        code: "invalid",
        message: "a checkout belongs to a repo or a branch",
        source_id: null,
      });
    }
    return Promise.resolve(answer);
  },
  setCheckoutOverride: (entityId: string, path: string | null) => {
    writes.push({ entityId, path });
    return Promise.resolve(afterWrite ?? answer);
  },
  checkoutCommands: () => {
    commandReads += 1;
    return refuseCommands
      ? Promise.reject({ code: "internal", message: "no database", source_id: null })
      : Promise.resolve(commands);
  },
  openCheckout: (entityId: string, action: string) => {
    opens.push({ entityId, action });
    if (action === refuseOpen) {
      return Promise.reject({
        code: "invalid",
        // Deliberately *not* the template: what the panel prints has to come
        // from what it read, not from what the backend happened to echo.
        message: "No such file or directory (os error 2)",
        source_id: null,
      });
    }
    return Promise.resolve(undefined);
  },
}));

const CheckoutPanel = (await import("./CheckoutPanel.svelte")).default;

const REPO = "gitea:tidewater/payout-service";
const URL = "https://gitea.example.com/tidewater/payout-service";

function view(over: Partial<CheckoutView> = {}): CheckoutView {
  return {
    repo_entity_id: REPO,
    repo_url: URL,
    path: null,
    found_by: "nothing",
    clones_root: "/Users/mara/src",
    clone_command: `git clone ${URL}`,
    ...over,
  };
}

let target: HTMLDivElement;
let app: Record<string, unknown> | undefined;

function render(entityId = REPO) {
  app = mount(CheckoutPanel, { target, props: { entityId } });
  flushSync();
  return app;
}

/** Let the read's promise land, then re-render. */
async function settle() {
  await Promise.resolve();
  await Promise.resolve();
  flushSync();
}

function text(): string {
  return (target.textContent ?? "").replace(/\s+/g, " ").trim();
}

function button(label: string): HTMLButtonElement {
  const found = [...target.querySelectorAll("button")].find((element) =>
    (element.textContent ?? "").includes(label),
  );
  if (!found) throw new Error(`no button reading ${label}: ${text()}`);
  return found as HTMLButtonElement;
}

beforeEach(() => {
  reads.length = 0;
  writes.length = 0;
  answer = view();
  afterWrite = null;
  refuseRead = false;
  opens.length = 0;
  refuseOpen = null;
  refuseCommands = false;
  commandReads = 0;
  commands = configured();
  toasts.items = [];
  target = document.createElement("div");
  document.body.append(target);
});

afterEach(() => {
  if (app) unmount(app);
  app = undefined;
  target.remove();
});

test("a checkout the scan found is drawn as a path, and says where it came from", async () => {
  answer = view({ path: "/Users/mara/src/payout-service", found_by: "scan" });
  render();
  await settle();

  expect(reads).toEqual([REPO]);
  expect(text()).toContain("/Users/mara/src/payout-service");
  expect(text()).toContain("found under the clones root");
  // Nothing to copy: there is a checkout, so the clone command is not offered.
  expect(text()).not.toContain("git clone");
});

test("a path set by hand says so, and offers to clear it back to the scan", async () => {
  answer = view({ path: "/Users/mara/work/payout-service", found_by: "override" });
  render();
  await settle();

  expect(text()).toContain("set by hand");
  expect(text()).toContain("/Users/mara/work/payout-service");

  afterWrite = view({ path: "/Users/mara/src/payout-service", found_by: "scan" });
  button("Clear").click();
  await settle();

  expect(writes).toEqual([{ entityId: REPO, path: null }]);
  // The panel redraws from the backend's answer, so clearing *shows* the scan
  // taking over rather than blanking the line.
  expect(text()).toContain("found under the clones root");
  expect(text()).toContain("/Users/mara/src/payout-service");
});

test("no checkout says so, names the root it looked in, and offers the clone command", async () => {
  render();
  await settle();

  expect(text()).toContain("No checkout on this machine.");
  expect(text()).toContain("/Users/mara/src");
  expect(text()).toContain(`git clone ${URL}`);
  expect(button("Copy clone command")).toBeTruthy();
});

test("no clones root is a different sentence from nothing found under one", async () => {
  answer = view({ clones_root: null });
  render();
  await settle();

  expect(text()).toContain("No clones root is set.");
  expect(text()).not.toContain("Nothing under");
});

test("a repo with no URL offers no clone command, because there is none to build", async () => {
  answer = view({ repo_url: null, clone_command: null });
  render();
  await settle();

  expect(text()).toContain("No checkout on this machine.");
  expect(text()).not.toContain("git clone");
});

test("setting a path sends it, and the panel draws what came back", async () => {
  render();
  await settle();

  button("Set path…").click();
  flushSync();
  const input = target.querySelector<HTMLInputElement>("input");
  if (!input) throw new Error("no path input");
  input.value = "/Users/mara/work/payout-service";
  input.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();

  afterWrite = view({ path: "/Users/mara/work/payout-service", found_by: "override" });
  button("Save").click();
  await settle();

  expect(writes).toEqual([{ entityId: REPO, path: "/Users/mara/work/payout-service" }]);
  expect(text()).toContain("set by hand");
  expect(text()).toContain("/Users/mara/work/payout-service");
});

test("a branch draws its repository's checkout, and its override is keyed on the repo", async () => {
  const branch = `${REPO}@refs/heads/feature/PAY-231-sepa-retry`;
  answer = view({ path: "/Users/mara/src/payout-service", found_by: "scan" });
  render(branch);
  await settle();

  expect(reads).toEqual([branch]);
  expect(text()).toContain("/Users/mara/src/payout-service");

  // Sent with the *branch's* address; the backend is what keys it on the
  // repository, and `checkout_ipc.rs` is where that is asserted.
  afterWrite = view({ path: "/Users/mara/work/wt", found_by: "override" });
  button("Set path…").click();
  flushSync();
  const input = target.querySelector<HTMLInputElement>("input");
  if (!input) throw new Error("no path input");
  input.value = "/Users/mara/work/wt";
  input.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  button("Save").click();
  await settle();

  expect(writes).toEqual([{ entityId: branch, path: "/Users/mara/work/wt" }]);
});

test("a read that fails leaves the panel quiet rather than shouting over the item", async () => {
  refuseRead = true;
  render();
  await settle();

  // Still the reading line, and no error banner: the detail is worth drawing
  // without a checkout, and a toast about one nobody asked to see is noise.
  expect(text()).toContain("Reading…");
  expect(text()).not.toContain("No checkout");
});

// -- the open buttons (#501) -------------------------------------------------

const FOUND = { path: "/Users/mara/src/payout-service", found_by: "scan" as const };

test("a checkout offers the three buttons, and pressing one runs its action", async () => {
  answer = view(FOUND);
  render();
  await settle();

  for (const label of ["Open in VS Code", "Open in JetBrains", "Open terminal here"]) {
    expect(button(label).disabled).toBe(false);
  }

  button("Open in VS Code").click();
  await settle();

  // The entity's own address and the action id -- and nothing else. What is
  // opened is the backend's answer to that address, never a value the panel
  // passed down from the mirrored row it is drawn beside (ADR-0016).
  expect(opens).toEqual([{ entityId: REPO, action: "vscode" }]);
});

test("a branch's buttons run against the branch's own address", async () => {
  const branch = `${REPO}@refs/heads/feature/PAY-231-sepa-retry`;
  answer = view(FOUND);
  render(branch);
  await settle();

  button("Open terminal here").click();
  await settle();

  expect(opens).toEqual([{ entityId: branch, action: "terminal" }]);
});

test("no checkout means no buttons: there is nothing to open", async () => {
  answer = view();
  render();
  await settle();

  expect(text()).toContain("No checkout on this machine.");
  expect(() => button("Open in VS Code")).toThrow();
});

test("an action with no template is drawn as not configured and cannot be pressed", async () => {
  answer = view(FOUND);
  commands = configured().map((command) =>
    command.action === "jetbrains" ? { ...command, template: null } : command,
  );
  render();
  await settle();

  // The button is still there -- spec #491: off macOS the buttons exist and
  // read *not configured* until a template is set.
  const jetbrains = button("Open in JetBrains");
  expect(jetbrains.disabled).toBe(true);
  expect(text()).toContain("not configured");
  expect(jetbrains.title).toContain("Settings");
  // And the other two are untouched by it.
  expect(button("Open terminal here").disabled).toBe(false);

  jetbrains.click();
  await settle();
  expect(opens).toEqual([]);
});

test("a spawn that fails says so and names the template that would not run", async () => {
  answer = view(FOUND);
  refuseOpen = "vscode";
  render();
  await settle();

  button("Open in VS Code").click();
  await settle();

  const toast = toasts.items.at(-1);
  expect(toast?.tone).toBe("err");
  // The template, verbatim, because that is the string somebody has to go and
  // change -- and the reason, which came from the backend.
  expect(toast?.text).toContain(VSCODE);
  expect(toast?.text).toContain("No such file or directory");
});

test("a refused commands read leaves the panel drawing its checkout and no buttons", async () => {
  answer = view(FOUND);
  refuseCommands = true;
  render();
  await settle();

  // The same choice the checkout read makes: the detail is worth drawing, and
  // a toast about a read nobody asked for would be noise over the item.
  expect(text()).toContain("/Users/mara/src/payout-service");
  expect(() => button("Open in VS Code")).toThrow();
  expect(toasts.items).toEqual([]);
});

test("a second detail re-reads the commands, so a changed template is not stale", async () => {
  answer = view(FOUND);
  // A reactive props object, because what is under test is the panel staying
  // mounted while the address under it changes -- which is what happens when a
  // reader opens a second repo from the same room, and the case a fresh mount
  // per test cannot reach.
  const props = $state({ entityId: REPO });
  app = mount(CheckoutPanel, { target, props });
  flushSync();
  await settle();
  expect(commandReads).toBe(1);

  // The template somebody just changed in Settings.
  commands = configured().map((command) =>
    command.action === "vscode" ? { ...command, template: "code {path}" } : command,
  );
  props.entityId = `${REPO}-two`;
  flushSync();
  await settle();

  expect(commandReads).toBe(2);
  refuseOpen = "vscode";
  button("Open in VS Code").click();
  await settle();
  expect(toasts.items.at(-1)?.text).toContain("code {path}");
});
