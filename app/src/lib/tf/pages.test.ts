/**
 * The page registry (#908): every feature's `page.ts` is found, and each
 * address draws the page it drew when `TfPage.svelte` was one `{#if}` chain.
 */
import { expect, test } from "vitest";

import { page as jobPage } from "./job/page";
import { PAGES, pageFor } from "./pages";
import { parseTfHash } from "./route";
import { page as settingsPage } from "./settings/page";
import { page as ticketsPage } from "./tickets/page";
import { page as timePage } from "./time/page";

test("the glob finds every feature's page", () => {
  expect(PAGES).toContain(ticketsPage);
  expect(PAGES).toContain(jobPage);
  expect(PAGES).toContain(timePage);
  expect(PAGES).toContain(settingsPage);
});

const ADDRESSES = [
  ["#/tickets", "tickets"],
  ["", "tickets"],
  ["#/tickets/PAY-231", "tickets"],
  ["#/job/PAY-231", "job"],
  ["#/job/PAY-231/code", "job"],
  ["#/job", "shell"],
  ["#/time", "time"],
  ["#/time/2026-10-15", "time"],
  ["#/settings", "settings"],
  ["#/settings/source/teamcity", "settings"],
  ["#/builds", "shell"],
  ["#/nowhere", "shell"],
] as const;

test.each(ADDRESSES)("%s draws the %s page", (hash, expected) => {
  const found = pageFor(parseTfHash(hash));
  const names = new Map([
    [ticketsPage, "tickets"],
    [jobPage, "job"],
    [timePage, "time"],
    [settingsPage, "settings"],
  ]);
  const name = found === undefined ? "shell" : (names.get(found) ?? "other");
  expect(name).toBe(expected);
});

test("no address is matched by two pages", () => {
  for (const [hash] of ADDRESSES) {
    const route = parseTfHash(hash);
    expect(PAGES.filter((page) => page.matches(route)).length, hash).toBeLessThanOrEqual(1);
  }
});

test("the job page hands TfJob the address's key and tab", () => {
  const route = parseTfHash("#/job/PAY-231/code");
  const props = jobPage.props({ route } as Parameters<typeof jobPage.props>[0]);
  expect(props).toMatchObject({ segment: "PAY-231", tab: "code" });
});
