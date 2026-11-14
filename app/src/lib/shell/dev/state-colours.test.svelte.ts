/**
 * Settings › Appearance › State colours (#2466) through the app's own mount
 * over the whole fake IPC: a section and its line for each kind the fake
 * declares a state for, and a move that reaches the fake's `set_tone_override`
 * and draws what it answered. Under `dev/` because only the harness may
 * import the harness.
 */
import { mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import App from "../../../App.svelte";
import { until } from "../test-wait";
import { demoHandlers, installFakeTauri, type Handler } from "./fake-tauri";

let sent: { cmd: string; args: Record<string, unknown> }[] = [];
let target: HTMLDivElement;
let app: Record<string, unknown> | undefined;

function recorded(handlers: Record<string, Handler>): Record<string, Handler> {
  return Object.fromEntries(
    Object.entries(handlers).map(([cmd, handler]) => [
      cmd,
      (args: Record<string, unknown>) => {
        sent.push({ cmd, args });
        return handler(args);
      },
    ]),
  );
}

beforeEach(() => {
  sent = [];
  installFakeTauri(recorded(demoHandlers()));
  target = document.createElement("div");
  document.body.append(target);
});

afterEach(() => {
  if (app) unmount(app);
  app = undefined;
  target.remove();
  history.replaceState(null, "", location.pathname + location.search);
});

const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, " ").trim() ?? "";
const section = (name: string) => target.querySelector<HTMLElement>(`.tf-kept:not([hidden]) section[aria-label="${name}"]`);
const column = (name: string, tone: string) =>
  [...(section(name)?.querySelectorAll(`[role="group"][aria-label="${tone}"] .tf-sc-word`) ?? [])].map(text);

test("the dev fake draws a section and its line for builds, monitors, pull requests and tickets", async () => {
  location.hash = "#/settings/appearance";
  app = mount(App, { target, props: {} });
  await until(() => section("Tickets") !== null, "Appearance never drew State colours");

  const sections = [...target.querySelectorAll(".tf-kept:not([hidden]) .tf-sc section")];
  expect(sections.map((s) => s.getAttribute("aria-label"))).toEqual(["Builds", "Monitors", "Pull requests", "Tickets"]);
  expect(text(section("Builds")!.querySelector(".tf-sc-line"))).toBe(
    "Error is what the inbox and signals count as a failed build. A job counts Error builds as failing, Active as running and Good as passing.",
  );
  expect(text(section("Pull requests")!.querySelector(".tf-sc-line"))).toContain("Good and Quiet pull requests are left out of the open ones on Code");
  for (const s of sections) expect(text(s.querySelector(".tf-sc-line"))).not.toBe("");
  // The fake's two seen Jira statuses: Blocked at its category's info, To Do (category to do) uncoloured.
  expect(column("Tickets", "Active")).toEqual(["In progress", "In review", "Blocked"]);
  expect(section("Tickets")!.querySelector<HTMLElement>('button[aria-label="Move Blocked"]')!.title).toContain("From its Jira category");
  expect(column("Tickets", "No colour")).toEqual(["To do"]);

  section("Builds")!
    .querySelector('button[aria-label="Move Failed"]')!
    .dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true }));
  await until(() => column("Builds", "Active").includes("Failed"), "Failed never moved to Active");
  expect(sent.filter((c) => c.cmd === "set_tone_override").map((c) => c.args)).toEqual([{ kind: "build", word: "FAILURE", tone: "info" }]);
  expect(section("Builds")!.querySelector('button[aria-label="Reset Failed"]')).not.toBeNull();
});
