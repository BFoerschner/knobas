/**
 * The page registry (#908): every feature's `page.ts` is found, and each
 * address draws the page it drew when `TfPage.svelte` was one `{#if}` chain.
 */
import { expect, test } from "vitest";

import { page as jobPage } from "./job/page";
import { PAGES, pageFor } from "./pages";
import { parseTfHash } from "./route";
import { page as ticketsPage } from "./tickets/page";

test("the glob finds every feature's page", () => {
  expect(PAGES).toContain(ticketsPage);
  expect(PAGES).toContain(jobPage);
});

test.each([
  ["#/tickets", "tickets"],
  ["", "tickets"],
  ["#/tickets/PAY-231", "tickets"],
  ["#/job/PAY-231", "job"],
  ["#/job/PAY-231/code", "job"],
  ["#/job", "shell"],
  ["#/time", "shell"],
  ["#/settings", "shell"],
  ["#/nowhere", "shell"],
])("%s draws the %s page", (hash, expected) => {
  const found = pageFor(parseTfHash(hash));
  const name = found === ticketsPage ? "tickets" : found === jobPage ? "job" : found === undefined ? "shell" : "other";
  expect(name).toBe(expected);
});

test("the job page hands TfJob the address's key and tab", () => {
  const route = parseTfHash("#/job/PAY-231/code");
  const props = jobPage.props({ route } as Parameters<typeof jobPage.props>[0]);
  expect(props).toMatchObject({ segment: "PAY-231", tab: "code" });
});
