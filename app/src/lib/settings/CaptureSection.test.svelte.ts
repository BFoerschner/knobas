/**
 * The capture-shortcut settings section (issue #503, criterion 1).
 *
 * The seam every other settings section is tested at: a rendered section in,
 * user-visible text and bridge calls out. What the backend stores, and what a
 * refusal is, are `crates/knobas-app/tests/capture_ipc.rs`'s; what is asserted
 * here is that the field draws what is **stored**, that Save and Clear send what
 * was asked for, and — the one this section exists for — that a shortcut which
 * is stored and **not registered** says so rather than reading as set.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import CaptureSection from "./CaptureSection.svelte";
import type { CaptureShortcut } from "../ipc/entity";

const SHORTCUT = "CmdOrCtrl+Shift+N";

let target: HTMLDivElement;
let app: Record<string, unknown> | undefined;

/**
 * Mount the section over a bridge that **remembers** what it was sent.
 *
 * A fixture that answered the same thing after a write could not tell a section
 * that re-reads from one that keeps what was typed, which is the property the
 * Save test is about — `CheckoutsSection`'s reason, and its shape.
 *
 * `refuse` is what the stand-in operating system will not hand over: the write
 * stores it and the answer carries a sentence, which is the backend's own
 * behaviour and not this section's.
 */
function render(
  initial: CaptureShortcut = { accelerator: null, refusal: null },
  refuse: string | null = null,
) {
  const sent: (string | null)[] = [];
  let stored = initial;
  app = mount(CaptureSection, {
    target,
    props: {
      ports: {
        captureShortcut: () => Promise.resolve(stored),
        setCaptureShortcut: (accelerator: string | null) => {
          sent.push(accelerator);
          const trimmed = accelerator?.trim() ?? "";
          stored =
            trimmed === ""
              ? { accelerator: null, refusal: null }
              : {
                  accelerator: trimmed,
                  refusal:
                    trimmed === refuse ? `${trimmed} is registered by another application` : null,
                };
          return Promise.resolve(stored);
        },
      },
    },
  });
  flushSync();
  return { sent };
}

async function settle() {
  for (let i = 0; i < 4; i += 1) await Promise.resolve();
  flushSync();
}

function text(): string {
  return (target.textContent ?? "").replace(/\s+/g, " ").trim();
}

function field(): HTMLInputElement {
  const input = target.querySelector<HTMLInputElement>("#capture-shortcut");
  if (!input) throw new Error(`no shortcut field: ${text()}`);
  return input;
}

function button(label: string): HTMLButtonElement {
  const found = target.querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`);
  if (!found) throw new Error(`no button named ${label}: ${text()}`);
  return found;
}

function type(value: string) {
  const input = field();
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

test("a knobas nobody has configured says nothing is registered", async () => {
  render();
  await settle();

  expect(field().value).toBe("");
  expect(text()).toContain("Not set — no shortcut is registered");
  // Nothing to clear, so nothing offers to.
  expect(target.querySelector('button[aria-label="Clear capture shortcut"]')).toBeNull();
});

test("the field draws the stored shortcut, and says it is registered", async () => {
  render({ accelerator: SHORTCUT, refusal: null });
  await settle();

  expect(field().value).toBe(SHORTCUT);
  expect(text()).toContain("Registered");
});

test("Save sends what was typed, and the field then draws what is stored", async () => {
  const { sent } = render();
  await settle();

  type(SHORTCUT);
  button("Save capture shortcut").click();
  await settle();

  expect(sent).toEqual([SHORTCUT]);
  expect(field().value).toBe(SHORTCUT);
  expect(text()).toContain("Registered");
});

/**
 * The one this section exists for: a shortcut that was stored and that the
 * operating system would not hand over. It reads as **not registered**, with
 * the refusal beside it — not as *Registered*, and not as an error that
 * replaced the field.
 */
test("a shortcut the operating system refused is shown, with the refusal", async () => {
  render({ accelerator: null, refusal: null }, SHORTCUT);
  await settle();

  type(SHORTCUT);
  button("Save capture shortcut").click();
  await settle();

  expect(field().value).toBe(SHORTCUT);
  expect(text()).toContain(`Not registered: ${SHORTCUT} is registered by another application`);
  expect(text()).not.toContain("Not set");
});

test("a refusal that was there on the first read is drawn without a write", async () => {
  render({ accelerator: SHORTCUT, refusal: "another application already registered it" });
  await settle();

  expect(text()).toContain("Not registered: another application already registered it");
});

test("Clear sends nothing and the section goes back to unset", async () => {
  const { sent } = render({ accelerator: SHORTCUT, refusal: null });
  await settle();

  button("Clear capture shortcut").click();
  await settle();

  expect(sent).toEqual([null]);
  expect(field().value).toBe("");
  expect(text()).toContain("Not set — no shortcut is registered");
});

test("Save is not offered while the field matches what is stored", async () => {
  render({ accelerator: SHORTCUT, refusal: null });
  await settle();

  expect(button("Save capture shortcut").disabled).toBe(true);
  type("Alt+Space");
  expect(button("Save capture shortcut").disabled).toBe(false);
});

test("a read that failed says so and offers to try again, rather than drawing an empty field", async () => {
  app = mount(CaptureSection, {
    target,
    props: {
      ports: {
        captureShortcut: () =>
          Promise.reject({ code: "not_ready", message: "the database is still starting" }),
      },
    },
  });
  flushSync();
  await settle();

  expect(text()).toContain("the database is still starting");
  expect(target.querySelector("#capture-shortcut")).toBeNull();
});
