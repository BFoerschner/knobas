/**
 * The card's sections table (#2401): Code's rollup, and which one signal a
 * row draws. Pure rules over rows; the shell tests drive the table itself.
 */
import { expect, test } from "vitest";

import type { EntityRow } from "../../ipc/entity";
import { cardSections, type CardReads } from "./card-sections";
import { codeRollup, worstSignal, type AssetLine } from "./job";

function row(entity_id: string, kind: string, state: EntityRow["state"] = null): EntityRow {
  return { entity_id, kind, source_id: "gitea", title: entity_id, updated_at: null, synced_at: "2026-08-22T00:00:00Z", path: null, state };
}

const OPEN = { text: "Open", tone: "info" } as const;
const MERGED = { text: "Merged", tone: "muted" } as const;

test("Code counts its open pull requests, and a merged one is not open", () => {
  const rollup = codeRollup([row("pr-1", "pr", OPEN), row("pr-2", "pr", MERGED), row("pr-3", "pr", OPEN)], new Set());
  expect(rollup).toEqual({ lead: "3 items", signals: [{ tone: "info", text: "2 open PRs" }] });
});

test("Code counts the branches with commits not pushed, and puts them before the open pull requests", () => {
  const rows = [row("pr-1", "pr", OPEN), row("b-1", "branch"), row("b-2", "branch"), row("b-3", "branch"), row("c-1", "commit")];
  const rollup = codeRollup(rows, new Set(["b-1", "b-3"]));
  expect(rollup).toEqual({
    lead: "5 items",
    signals: [
      { tone: "warn", text: "2 not pushed" },
      { tone: "info", text: "1 open PR" },
    ],
  });
});

test("an unpushed id that is not one of the rows' branches counts nothing", () => {
  expect(codeRollup([row("pr-1", "pr", MERGED)], new Set(["pr-1", "elsewhere"]))).toEqual({ lead: "1 item", signals: [] });
});

test("the row draws the worst signal, whatever order the rollup lists them in", () => {
  expect(
    worstSignal({
      lead: "",
      signals: [
        { tone: "ok", text: "5 passing" },
        { tone: "info", text: "1 running" },
        { tone: "err", text: "1 failing" },
      ],
    }),
  ).toEqual({ tone: "err", text: "1 failing" });
  expect(worstSignal({ lead: "3 pages", signals: [] })).toBeNull();
});

test("a settled signal is no state worth a row's words", () => {
  expect(worstSignal({ lead: "", signals: [{ tone: "ok", text: "3 fine" }] })).toBeNull();
});

const NOTHING: CardReads = {
  entityId: "jira:PAY-231",
  subtasks: null,
  code: [],
  unpushed: new Set(),
  builds: [],
  pages: [],
  monitors: [],
  assets: [],
};
const asset = (id: string): AssetLine => ({ id, name: id, type: "service", environment: null, on: null, tone: null });

test("the Assets row opens the asset tree at the ticket's first asset, its id encoded", () => {
  const { rows } = cardSections({ ...NOTHING, assets: [asset("asset:a/1"), asset("asset:b")] });
  expect(rows.map((row) => [row.name, row.count, row.href, row.opens])).toEqual([["Assets", 2, "#/asset/asset%3Aa%2F1", "assets"]]);
});

test("a ticket that reaches no asset draws no Assets row, and has no first asset to open", () => {
  expect(cardSections({ ...NOTHING, assets: [] }).rows).toEqual([]);
});

const coded = (kind: string, n: number, state = "open"): EntityRow[] =>
  Array.from({ length: n }, (_, i) => ({ entity_id: `${kind}-${i}`, kind, chip: { text: state, tone: "info" }, payload: {} }) as unknown as EntityRow);

test("Code counts what its two tabs land on, names both kinds, and leaves a branch and a commit out", () => {
  const { rows } = cardSections({ ...NOTHING, code: [...coded("repo", 2), ...coded("pr", 1), ...coded("branch", 1), ...coded("commit", 1)] });
  expect(rows[0]?.count).toBe(3);
  expect(rows[0]?.words).toContain("2 repositories, 1 pull request");
});

test("Code with one kind lets the count say it; with only a branch it still draws a row", () => {
  const repos = cardSections({ ...NOTHING, code: coded("repo", 2) }).rows[0];
  expect([repos?.count, repos?.note, repos?.words]).toEqual([2, undefined, "2 repositories"]);
  expect(cardSections({ ...NOTHING, code: coded("branch", 1) }).rows[0]?.count).toBe(1);
});
