/**
 * The settings block (#664, ADR-0020): one row per value -- name, value, and
 * a mark for an inherited one. A secret's value is never drawn (the adapter
 * sends none), and every string is source text, never markup (rule 5).
 */
import { flushSync, mount, unmount } from "svelte";
import { expect, test } from "vitest";

import type { Setting } from "../../ipc/entity";
import SettingsBlock from "./SettingsBlock.svelte";

function setting(over: Partial<Setting> & { name: string }): Setting {
  return { value: "", inherited: false, secret: false, editable: false, description: null, ...over };
}

function render(values: Setting[], title: string | null = null) {
  const target = document.createElement("div");
  document.body.append(target);
  const app = mount(SettingsBlock, { target, props: { title, values } });
  flushSync();
  return {
    target,
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
