/**
 * The card's sections table (#2401): Code's rollup, and which one signal a
 * row draws. Pure rules over rows; the shell tests drive the table itself.
 */
import { expect, test } from "vitest";

import type { EntityRow } from "../../ipc/entity";
import { codeRollup, worstSignal } from "./job";

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
