/**
 * The page registry (#908): every feature's `page.ts` is found, and each
 * address draws the page it drew when `TfPage.svelte` was one `{#if}` chain.
 */
import { expect, test } from "vitest";

import { page as buildsPage } from "./builds/page";
import { page as assetTreePage } from "./asset-tree/page";
import { page as codePage } from "./code/page";
import { page as codeRowPage } from "./code-row/page";
import { page as controlsPage } from "./controls/page";
import { page as depsPage } from "./deps/page";
import { page as diffPage } from "./diff/page";
import { page as inboxPage } from "./inbox/page";
import { page as jobPage } from "./job/page";
import { PAGES, pageFor } from "./pages";
import { parseTfHash } from "./route";
import { page as prPage } from "./pr/page";
import { page as repoPage } from "./repo/page";
import { page as reviewPage } from "./review/page";
import { page as settingsPage } from "./settings/page";
import { page as setsPage } from "./sets/page";
import { page as ticketsPage } from "./tickets/page";
import { page as timePage } from "./time/page";

test("the glob finds every feature's page", () => {
  expect(PAGES).toContain(ticketsPage);
  expect(PAGES).toContain(jobPage);
  expect(PAGES).toContain(timePage);
  expect(PAGES).toContain(settingsPage);
  expect(PAGES).toContain(reviewPage);
  expect(PAGES).toContain(buildsPage);
  expect(PAGES).toContain(depsPage);
  expect(PAGES).toContain(assetTreePage);
  expect(PAGES).toContain(inboxPage);
  expect(PAGES).toContain(codePage);
  expect(PAGES).toContain(repoPage);
  expect(PAGES).toContain(setsPage);
  expect(PAGES).toContain(codeRowPage);
  expect(PAGES).toContain(diffPage);
  expect(PAGES).toContain(controlsPage);
});

const ADDRESSES = [
  ["#/tickets", "tickets"],
  ["", "tickets"],
  ["#/tickets/PAY-231", "tickets"],
  // One page for a ticket and the job (spec #2396).
  ["#/ticket/jira%3APAY-231", "job"],
  ["#/ticket/jira%3APAY-231/map", "job"],
  ["#/ticket", "tickets"],
  ["#/job/PAY-231", "job"],
  ["#/job/PAY-231/code", "job"],
  ["#/job", "shell"],
  ["#/time", "time"],
  ["#/time/2026-10-15", "time"],
  ["#/settings", "settings"],
  ["#/settings/source/teamcity", "settings"],
  ["#/review", "review"],
  ["#/review/link-1", "review"],
  ["#/builds", "builds"],
  ["#/builds/teamcity%3AbuildType%3APayout_Build", "builds"],
  ["#/build/x/y", "builds"],
  ["#/code", "code"],
  ["#/code/repos", "code"],
  ["#/code/prs", "code"],
  ["#/repo/gitea%3Atidewater%2Fpayout-service", "repo"],
  ["#/repo/gitea%3Atidewater%2Fpayout-service/files/src/lib.rs", "repo"],
  ["#/pr/gitea%3Atidewater%2Fpayout-service%23142", "pr"],
  ["#/pr/gitea%3Atidewater%2Fpayout-service%23142/commits", "pr"],
  ["#/branch/gitea%3Atidewater%2Fpayout-service%40refs%2Fheads%2Fmain", "code row"],
  ["#/commit/mock%3A9f2c1ab", "code row"],
  ["#/diff/gitea%3Atidewater%2Fpayout-service/main/feature%2Fsepa", "diff"],
  ["#/diff/gitea%3Atidewater%2Fpayout-service/main", "shell"],
  ["#/deps", "deps"],
  ["#/deps/job/PAY-231", "deps"],
  ["#/map", "deps"],
  ["#/assets", "asset tree"],
  ["#/asset/asset:vm-pay-01", "asset tree"],
  ["#/route/route:payouts", "asset tree"],
  ["#/inbox", "inbox"],
  ["#/sets", "sets"],
  ["#/sets/payout-stack", "sets"],
  // The controls specimen (#2280): a dev build only, which the tests are.
  ["#/controls", "controls"],
  ["#/controls/more", "shell"],
  ["#/nowhere", "shell"],
] as const;

test.each(ADDRESSES)("%s draws the %s page", (hash, expected) => {
  const found = pageFor(parseTfHash(hash));
  const names = new Map([
    [ticketsPage, "tickets"],
    [jobPage, "job"],
    [timePage, "time"],
    [settingsPage, "settings"],
    [reviewPage, "review"],
    [buildsPage, "builds"],
    [depsPage, "deps"],
    [assetTreePage, "asset tree"],
    [inboxPage, "inbox"],
    [codePage, "code"],
    [repoPage, "repo"],
    [prPage, "pr"],
    [setsPage, "sets"],
    [codeRowPage, "code row"],
    [diffPage, "diff"],
    [controlsPage, "controls"],
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

test("the deps page opens from the ticket its address names, with the open job's id when it is that ticket", () => {
  type Ctx = Parameters<typeof depsPage.props>[0];
  const props = (hash: string, navJob: Ctx["navJob"]) =>
    depsPage.props({ route: parseTfHash(hash), ready: true, job: "PAY-231", navJob } as Ctx);
  const nav = { key: "PAY-231", entityId: "jira:PAY-231" };
  expect(props("#/deps/job/PAY-231", nav)).toMatchObject({ ticket: "PAY-231", ticketId: "jira:PAY-231", job: "PAY-231" });
  expect(props("#/deps/job/PAY-228", nav)).toMatchObject({ ticket: "PAY-228", ticketId: null });
  expect(props("#/deps", nav)).toMatchObject({ ticket: null, ticketId: null, job: "PAY-231" });
  expect(props("#/map", null)).toMatchObject({ ticket: null });
});

test("the review page hands TfReview the addressed proposal, or none", () => {
  const props = (hash: string) => reviewPage.props({ route: parseTfHash(hash) } as Parameters<typeof reviewPage.props>[0]);
  expect(props("#/review/link%3Afake-2")).toMatchObject({ sid: "link:fake-2" });
  expect(props("#/review")).toMatchObject({ sid: null });
});

test("the builds page hands TfBuilds the address's head and the pipeline's entity id", () => {
  const route = parseTfHash("#/builds/teamcity%3AbuildType%3APayout_Build");
  const props = buildsPage.props({ route, ready: true, job: null } as Parameters<typeof buildsPage.props>[0]);
  expect(props).toMatchObject({ head: "builds", rest: ["teamcity:buildType:Payout_Build"] });
  const run = parseTfHash("#/build/p/r");
  expect(buildsPage.props({ route: run, ready: true, job: null } as Parameters<typeof buildsPage.props>[0])).toMatchObject({
    head: "build",
    rest: ["p", "r"],
  });
});

test("a ticket address and a job address of one ticket are one page, and only the job address chooses (spec #2396)", () => {
  const props = (hash: string) => jobPage.props({ route: parseTfHash(hash), back: null } as unknown as Parameters<typeof jobPage.props>[0]);
  expect(props("#/ticket/jira%3APAY-231/map")).toMatchObject({ segment: "jira:PAY-231", tab: "map", choose: false });
  expect(props("#/job/jira%3APAY-231")).toMatchObject({ segment: "jira:PAY-231", tab: null, choose: true });
  const address = (hash: string) => jobPage.address?.(parseTfHash(hash));
  expect(address("#/job/jira%3APAY-231")).toBe(address("#/ticket/jira%3APAY-231/conversation"));
});
