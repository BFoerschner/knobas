/**
 * The re-enter strip, and the one rule the whole optional account rests on:
 * **absent means keep** (#452).
 *
 * Mounted directly rather than through `SourcesView`, unlike the rest of this
 * strip's coverage, because what is being asserted is a *prop* -- whether the
 * source's adapter declares it can carry a second credential. Driving that
 * through the view would mean loading the module-level kind registry to get
 * one boolean into one component.
 *
 * The direction that matters is the one a form cannot show: knobas may never
 * read a credential back, so a reader adding an account has no way to retype
 * an API key Uptime Kuma showed them once. If an empty key field meant *clear*
 * rather than *keep*, adding an account would silently break the source's
 * reads -- and the roster would go on drawing the last sync's monitors while
 * nothing polled them.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { CredentialHealth, SecretInput } from "../ipc/sources";

const NOW = new Date("2026-09-07T12:00:00Z");

const calls = {
  setSecret: [] as { id: string; secret: SecretInput }[],
  syncNow: [] as string[],
};

vi.mock("../ipc/sources", () => ({
  setSourceSecret: (id: string, secret: SecretInput) => {
    calls.setSecret.push({ id, secret });
    return Promise.resolve({
      source_id: id,
      state: "ok",
      checked_at: NOW.toISOString(),
      detail: null,
      secret_expires_at: null,
    } satisfies CredentialHealth);
  },
  syncNow: (id: string) => {
    calls.syncNow.push(id);
    return Promise.resolve(null);
  },
}));

const { default: ReenterSecret } = await import("./ReenterSecret.svelte");

let target: HTMLDivElement;
let app: Record<string, unknown> | undefined;

function render(acceptsAccount: boolean) {
  app = mount(ReenterSecret, {
    target,
    props: {
      sourceId: "kuma",
      displayName: "Uptime Kuma",
      acceptsAccount,
      onhealth: () => {},
      oncancel: () => {},
    },
  });
  flushSync();
}

function type(selector: string, value: string) {
  const el = target.querySelector<HTMLInputElement>(selector)!;
  el.value = value;
  el.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
}

function save(): HTMLButtonElement {
  return [...target.querySelectorAll<HTMLButtonElement>("button")].find((button) =>
    button.textContent?.includes("Save and retry sync"),
  )!;
}

/** Let the save's two awaited calls settle. */
const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

beforeEach(() => {
  calls.setSecret = [];
  calls.syncNow = [];
  target = document.createElement("div");
  document.body.append(target);
});

afterEach(() => {
  if (app) unmount(app);
  app = undefined;
  target.remove();
});

test("a source whose adapter declares no account gets no account fields", () => {
  render(false);
  expect(target.querySelector("#reenter-kuma-user")).toBeNull();
  expect(target.querySelector("#reenter-kuma-password")).toBeNull();
  // And nothing to save with nothing typed, exactly as before.
  expect(save().disabled).toBe(true);
});

/**
 * Criterion 1's "without re-entering the key": the key field is left empty,
 * the account is typed, and what crosses is `value: null` -- which the backend
 * reads as *keep what is stored*.
 */
test("an account can be added with the key field left empty", async () => {
  render(true);
  expect(save().disabled).toBe(true);

  type("#reenter-kuma-user", "knobas");
  type("#reenter-kuma-password", "knobas-dev");
  expect(save().disabled).toBe(false);

  save().click();
  await settle();

  expect(calls.setSecret).toEqual([
    {
      id: "kuma",
      secret: { value: null, account: { username: "knobas", password: "knobas-dev" } },
    },
  ]);
  expect(calls.syncNow).toEqual(["kuma"]);
});

/**
 * The other direction of the same rule: replacing an expired key without
 * touching the account sends `account: null`, and the backend keeps the stored
 * one. A form that sent an empty account here would silently take the write
 * half off a source whose reads a reader was fixing.
 */
test("the key can be replaced with the account fields left empty", async () => {
  render(true);
  type("#reenter-kuma", "uk1_fresh");
  save().click();
  await settle();

  expect(calls.setSecret).toEqual([
    { id: "kuma", secret: { value: "uk1_fresh", account: null } },
  ]);
});

/** Half an account is refused rather than stored. */
test("a half-filled account cannot be saved", () => {
  render(true);
  type("#reenter-kuma-user", "knobas");
  expect(save().disabled).toBe(true);
  expect(target.textContent).toContain("An account needs both a username and a password");

  type("#reenter-kuma-password", "knobas-dev");
  expect(save().disabled).toBe(false);
});

/** Neither half survives the round trip on screen. */
test("nothing typed here reaches the DOM or outlives the save", async () => {
  render(true);
  type("#reenter-kuma", "uk1_fresh");
  type("#reenter-kuma-user", "knobas");
  type("#reenter-kuma-password", "knobas-dev");
  expect(target.innerHTML).not.toContain("uk1_fresh");
  expect(target.innerHTML).not.toContain("knobas-dev");

  save().click();
  await settle();
  expect(target.querySelector<HTMLInputElement>("#reenter-kuma")!.value).toBe("");
  expect(target.querySelector<HTMLInputElement>("#reenter-kuma-user")!.value).toBe("");
  expect(target.querySelector<HTMLInputElement>("#reenter-kuma-password")!.value).toBe("");
});
