/**
 * Code's rules (#837, R-8.1, R-8.2), over what `list_entities` and
 * `entity_checkout` answer: how pull requests group, what a repository row
 * says, and which rows the filters keep.
 */
import { expect, test } from "vitest";

import type { CheckoutView, EntityPage, EntityRow, GroupTally } from "../../ipc/entity";
import {
  checkoutLine,
  compareHash,
  headerLine,
  openPrsOf,
  prLines,
  visiblePrs,
  visibleRepos,
  type PrFilter,
  type RepoFilter,
} from "./code";

const NOW = new Date(2026, 7, 22, 14, 32);

function pr(id: string, state: "open" | "merged" | "closed", updated = "2026-08-22T10:00:00Z"): EntityRow {
  const tone = state === "open" ? "info" : state === "merged" ? "ok" : "muted";
  return {
    entity_id: `gitea:tidewater/payout-service#${id}`,
    kind: "pr",
    source_id: "gitea",
    title: `PR ${id}`,
    updated_at: updated,
    synced_at: "2026-08-22T14:30:00Z",
    path: "tidewater/payout-service",
    state: { text: state, tone },
  };
}

function page(rows: EntityRow[], groups: GroupTally[] | null = null): EntityPage {
  return { rows, total: rows.length, tally: null, groups };
}

test("open pull requests group as the attention read groups them, then the settled ones", () => {
  const attention = page([pr("144", "open"), pr("142", "open"), pr("150", "open"), pr("151", "open")], [
    { group: "review_requested", count: 1 },
    { group: "my_pull_requests", count: 1 },
    { group: "open_pull_requests", count: 2 },
  ]);
  const all = page([pr("150", "open"), pr("139", "merged"), pr("144", "open"), pr("142", "open"), pr("151", "open"), pr("57", "closed")]);

  const lines = prLines(attention, all, NOW);

  expect(lines.map((line) => [line.number, line.group])).toEqual([
    ["#144", "review"],
    ["#142", "yours"],
    ["#150", "others"],
    ["#151", "others"],
    ["#139", "settled"],
    ["#57", "settled"],
  ]);
});

test("an open pull request untouched for more than five days is idle; a settled one never is", () => {
  const old = "2026-08-16T10:00:00Z";
  const lines = prLines(page([pr("1", "open", old), pr("2", "open")]), page([pr("1", "open", old), pr("2", "open"), pr("3", "merged", old)]), NOW);
  expect(lines.map((line) => [line.number, line.idle])).toEqual([
    ["#1", true],
    ["#2", false],
    ["#3", false],
  ]);
});

test("with no identity configured the open pull requests carry no group", () => {
  const lines = prLines(page([pr("1", "open")]), page([pr("1", "open"), pr("2", "closed")]), NOW);
  expect(lines.map((line) => line.group)).toEqual([null, "settled"]);
});

test("the pull request filter keeps by scope, state, flags, repository and words", () => {
  const other = { ...pr("9", "open"), path: "tidewater/ledger-api", title: "Fix ledger drift" };
  const lines = prLines(
    page([pr("144", "open"), pr("142", "open", "2026-08-10T10:00:00Z"), other], [
      { group: "review_requested", count: 1 },
      { group: "my_pull_requests", count: 1 },
      { group: "open_pull_requests", count: 1 },
    ]),
    page([pr("144", "open"), pr("142", "open"), other, pr("139", "merged"), pr("57", "closed")]),
    NOW,
  );
  const numbers = (filter: Partial<PrFilter>) =>
    visiblePrs(lines, { scope: "all", held: () => false, states: [], flags: [], repo: "", text: "", ...filter }).map(
      (line) => line.number,
    );

  expect(numbers({})).toEqual(["#144", "#142", "#9", "#139", "#57"]);
  expect(numbers({ scope: "mine" })).toEqual(["#144", "#142"]);
  expect(numbers({ scope: "job", held: (id) => id.endsWith("#139") })).toEqual(["#139"]);
  expect(numbers({ states: ["Merged", "Closed"] })).toEqual(["#139", "#57"]);
  expect(numbers({ flags: ["Waiting on me"] })).toEqual(["#144"]);
  expect(numbers({ flags: ["Idle > 5 days"] })).toEqual(["#142"]);
  expect(numbers({ repo: "tidewater/ledger-api" })).toEqual(["#9"]);
  expect(numbers({ text: "ledger drift" })).toEqual(["#9"]);
  expect(numbers({ text: "#57" })).toEqual(["#57"]);
});

function repo(name: string, updated = "2026-08-20T10:00:00Z"): EntityRow {
  return {
    entity_id: `gitea:tidewater/${name}`,
    kind: "repo",
    source_id: "gitea",
    title: `tidewater/${name}`,
    updated_at: updated,
    synced_at: "2026-08-22T14:30:00Z",
    path: null,
    state: null,
  };
}

function checkout(path: string | null, tree: Partial<Extract<NonNullable<CheckoutView["tree"]>, { status: "read" }>> | null = {}): CheckoutView {
  return {
    repo_entity_id: "gitea:tidewater/payout-service",
    repo_url: null,
    path,
    found_by: path === null ? "nothing" : "scan",
    clones_root: "/Users/mara/src",
    clone_command: null,
    tree:
      path === null || tree === null
        ? null
        : { status: "read", branch: "main", head: null, upstream: "origin/main", ahead: 0, behind: 0, changed: 0, untracked: 0, conflicted: 0, ...tree },
  };
}

test("a checkout reads as its branch and where it stands, or says it is not here", () => {
  expect(checkoutLine(undefined)).toBeNull();
  expect(checkoutLine(checkout(null))).toBe("not on this machine");
  expect(checkoutLine(checkout("/src/a"))).toBe("main, clean");
  expect(checkoutLine(checkout("/src/a", { branch: "feature/PAY-231-sepa-retry", ahead: 3, changed: 1 }))).toBe(
    "feature/PAY-231-sepa-retry, 3 ahead, 1 changed",
  );
  expect(checkoutLine(checkout("/src/a", { behind: 2, untracked: 1 }))).toBe("main, 2 behind, 1 changed");
  expect(checkoutLine(checkout("/src/a", null))).toBe("/src/a");
});

test("a repository counts its open pull requests by its declared parent path", () => {
  const lines = prLines(page([pr("1", "open"), pr("2", "open")]), page([pr("1", "open"), pr("2", "open"), pr("3", "merged")]), NOW);
  expect(openPrsOf(lines)).toEqual(new Map([["gitea tidewater/payout-service", 2]]));
});

test("the repository filter keeps by scope, flags and words; mine first, then last push", () => {
  const rows = [repo("ledger-api", "2026-08-21T10:00:00Z"), repo("payout-service", "2026-08-19T10:00:00Z"), repo("fx-rates", "2026-08-22T10:00:00Z")];
  const lines = prLines(page([pr("1", "open")], [{ group: "my_pull_requests", count: 1 }]), page([pr("1", "open")]), NOW);
  const checkouts = { "gitea:tidewater/ledger-api": checkout("/src/ledger-api"), "gitea:tidewater/fx-rates": checkout(null) };
  const names = (filter: Partial<RepoFilter>) =>
    visibleRepos(rows, lines, checkouts, { scope: "all", held: () => false, flags: [], text: "", ...filter }).map((row) => row.title);

  expect(names({})).toEqual(["tidewater/payout-service", "tidewater/fx-rates", "tidewater/ledger-api"]);
  expect(names({ scope: "mine" })).toEqual(["tidewater/payout-service"]);
  expect(names({ scope: "job", held: (id) => id.endsWith("fx-rates") })).toEqual(["tidewater/fx-rates"]);
  expect(names({ flags: ["Cloned here"] })).toEqual(["tidewater/ledger-api"]);
  expect(names({ flags: ["Has open PRs"] })).toEqual(["tidewater/payout-service"]);
  expect(names({ text: "fx" })).toEqual(["tidewater/fx-rates"]);
});

test("New pull request opens the compare page for the job's branch in its repository", () => {
  const branch = { ...repo("x"), entity_id: "gitea:tidewater/payout-service@refs/heads/feature/PAY-231-sepa-retry", kind: "branch", title: "feature/PAY-231-sepa-retry", path: "tidewater/payout-service" };
  expect(compareHash([branch], [repo("ledger-api"), repo("payout-service")])).toBe(
    "#/repo/gitea%3Atidewater%2Fpayout-service/compare/feature%2FPAY-231-sepa-retry",
  );
  expect(compareHash([branch], [repo("ledger-api")])).toBeNull();
  expect(compareHash([], [repo("payout-service")])).toBeNull();
});

test("the header names the one host the repositories come from", () => {
  expect(headerLine(47, [repo("a")], [{ id: "gitea", base_url: "https://git.example.com/" }])).toBe(
    "47 repositories on git.example.com, what asks for you first.",
  );
  expect(headerLine(2, [repo("a"), { ...repo("b"), source_id: "gitea-eu" }], [])).toBe("2 repositories, what asks for you first.");
  expect(headerLine(1, [repo("a")], [{ id: "gitea", base_url: "not a url" }])).toBe("1 repository, what asks for you first.");
});
