/**
 * Which sources the launcher names, and which it stays quiet about (#141).
 *
 * The rendered half is in `Launcher.test.svelte.ts`; these are the rules on
 * their own, including the two that decide whether anything appears at all.
 */
import { expect, test } from "vitest";

import type { FilterAnswer, SearchResponse } from "../ipc";
import { authorGaps } from "./coverage";

function response(
  sources: { source_id: string; display_name: string; answer: FilterAnswer }[] | null,
): SearchResponse {
  return {
    interpreted: {
      text: "sepa",
      prefix: null,
      filters: { sources: [], kinds: [], updated_within_days: null, mine: false, authors: [] },
      unknown_tokens: [],
    },
    groups: [],
    total: 0,
    took_ms: 3,
    coverage: sources === null ? [] : [{ dimension: "author", sources }],
  };
}

/** The gap itself: a source that could not be asked is named and explained. */
test("a source whose corpus names nobody is named, and says why", () => {
  const gaps = authorGaps(
    response([
      { source_id: "jira", display_name: "Jira", answer: "answered" },
      { source_id: "teamcity", display_name: "Buildserver", answer: "no_values" },
    ]),
  );
  expect(gaps).toEqual([
    {
      sourceId: "teamcity",
      name: "Buildserver",
      reason: "nothing it has synced names a person",
    },
  ]);
});

/**
 * The miss direction: nothing to explain, nothing said.
 *
 * All three of these are the same thing to a reader — the query never asked an
 * author question, or every source answered it — and an explanation that
 * appeared anyway would be noise on the ordinary path.
 */
test("a query with nothing to explain explains nothing", () => {
  expect(authorGaps(null)).toEqual([]);
  expect(authorGaps(response(null))).toEqual([]);
  expect(
    authorGaps(
      response([
        { source_id: "jira", display_name: "Jira", answer: "answered" },
        { source_id: "gitea", display_name: "Gitea", answer: "answered" },
      ]),
    ),
  ).toEqual([]);
});

/**
 * The report is read by dimension, not by position.
 *
 * `coverage` is a list so that a second filter is an addition rather than a
 * reshape (the backend's own reason for the shape). A reader that took
 * `coverage[0]` would silently start explaining the wrong filter the day one is
 * added — so this hands it a dimension it does not know, first.
 */
test("only the author dimension is read, whatever else is in the list", () => {
  const answer: SearchResponse = response([
    { source_id: "teamcity", display_name: "Buildserver", answer: "no_values" },
  ]);
  const withOther: SearchResponse = {
    ...answer,
    coverage: [
      { dimension: "assignee" as never, sources: [] },
      ...answer.coverage,
    ],
  };
  expect(authorGaps(withOther).map((gap) => gap.sourceId)).toEqual(["teamcity"]);
});
