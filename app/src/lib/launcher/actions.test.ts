import { expect, test } from "vitest";
import { parseTfHash, sectionOf } from "../tf/route";
import { settingsPlace } from "../tf/settings/settings";
import { filterActions, navigationActions } from "./actions";

/**
 * #1417: `>` with an empty query is the app's navigation menu (spec §4): the
 * top bar's sections left to right, Job left out (its address needs a key),
 * then Settings and its sections top to bottom. Written out by hand, so a
 * dropped or reordered section is a failure here and not a copy of `NAV`.
 */
test("the palette lists every section the shell has an address for, in the top bar's order, then Settings", () => {
  expect(navigationActions().map((action) => action.label)).toEqual([
    "Tickets",
    "Inbox",
    "Code",
    "Builds",
    "Docs",
    "Dependencies",
    "Monitors",
    "Link sets",
    "Proposed links",
    "Time",
    "Settings",
    "Settings › Sources",
    "Settings › Checkouts and Open in…",
    "Settings › Time",
    "Settings › Desktop notifications",
    "Settings › Standup",
    "Settings › Monitoring",
    "Settings › Notes and capture",
    "Settings › Backup and export",
    "Settings › Appearance",
    "Settings › Performance",
    "Settings › Renderers",
    "Settings › Keyboard",
    "Settings › Logs and database",
  ]);
});

/**
 * #1085: *All work* went to `#/ctx/all`, a room the ticket-first shell does
 * not have. #1417: every row's address reads back, through the route module,
 * as the section its label names.
 */
test("every row's address parses back to the section it names", () => {
  const places = navigationActions().map((action) => {
    const route = parseTfHash(action.hash);
    const section = sectionOf(route);
    if (section === "settings" && route.view === "full" && route.rest.length > 0)
      return [action.label, `settings/${settingsPlace(route.rest).section}`];
    return [action.label, section];
  });
  expect(places).toEqual([
    ["Tickets", "tickets"],
    ["Inbox", "inbox"],
    ["Code", "code"],
    ["Builds", "builds"],
    ["Docs", "docs"],
    ["Dependencies", "deps"],
    ["Monitors", "monitors"],
    ["Link sets", "sets"],
    ["Proposed links", "review"],
    ["Time", "time"],
    ["Settings", "settings"],
    ["Settings › Sources", "settings/sources"],
    ["Settings › Checkouts and Open in…", "settings/checkouts"],
    ["Settings › Time", "settings/time"],
    ["Settings › Desktop notifications", "settings/notifications"],
    ["Settings › Standup", "settings/standup"],
    ["Settings › Monitoring", "settings/monitoring"],
    ["Settings › Notes and capture", "settings/notes"],
    ["Settings › Backup and export", "settings/backup"],
    ["Settings › Appearance", "settings/appearance"],
    ["Settings › Performance", "settings/performance"],
    ["Settings › Renderers", "settings/renderers"],
    ["Settings › Keyboard", "settings/keyboard"],
    ["Settings › Logs and database", "settings/logs"],
  ]);
});

test("every row says in one line what it holds, and ids are unique", () => {
  const actions = navigationActions();
  for (const action of actions) {
    expect(action.detail, action.label).toMatch(/^\S.*\S$/);
    expect(action.detail, action.label).not.toContain("\n");
  }
  expect(new Set(actions.map((action) => action.id)).size).toBe(actions.length);
  expect(actions.find((action) => action.label === "Settings › Sources")?.detail).toBe(
    "Add, edit and sync the systems knobas reads",
  );
});

/**
 * Matching is `filterActions`' substring rule, unchanged: a word picks out
 * its section, and `set` picks out Settings and its sections (and Link sets,
 * whose name has the word in it).
 */
test("typing after > narrows the menu", () => {
  const labels = (text: string) => filterActions(navigationActions(), text).map((action) => action.label);
  expect(labels("inbox")).toEqual(["Inbox"]);
  expect(labels("In")[0]).toBe("Inbox");
  expect(labels("set")).toEqual([
    "Link sets",
    ...navigationActions()
      .map((action) => action.label)
      .filter((label) => label.startsWith("Settings")),
  ]);
});
