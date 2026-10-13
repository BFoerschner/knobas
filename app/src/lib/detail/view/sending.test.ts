/**
 * What a write on its way draws (#752): the place and words each op's
 * payload gives, and the mark that says whether it is sending or waiting.
 */
import { expect, test } from "vitest";

import { echoesOf, sendingMark } from "./sending";

const PR = "gitea:knobas/app#7";

test("a comment and a review are words for the discussion; a review carries its verdict", () => {
  expect(echoesOf({ Comment: { entity: PR, body: "on my way" } })).toEqual([
    { place: "said", verdict: null, text: "on my way" },
  ]);
  expect(
    echoesOf({
      SubmitReview: {
        entity: PR,
        verdict: "request_changes",
        body: "",
        comments: [{ path: "a.rs", side: "new", line: 3, body: "bound it" }],
      },
    }),
  ).toEqual([{ place: "said", verdict: "changes requested", text: "1 line comment" }]);
});

test("an edit draws each part it changes, in its own place", () => {
  expect(echoesOf({ EditPullRequest: { entity: PR, title: "New name", body: "words", state: "closed" } })).toEqual([
    { place: "title", title: "New name" },
    { place: "line", text: "Description edited" },
    { place: "state", to: "closed" },
  ]);
  expect(echoesOf({ EditPullRequest: { entity: PR, title: null, body: null, state: "open" } })).toEqual([
    { place: "state", to: "open" },
  ]);
});

test("a merge, a delete and a move are the state they leave", () => {
  expect(echoesOf({ MergePullRequest: { entity: PR, style: "squash", delete_branch: false } })).toEqual([
    { place: "state", to: "merged" },
  ]);
  expect(echoesOf({ DeleteBranch: { entity: "gitea:knobas/app@feature" } })).toEqual([{ place: "state", to: "deleted" }]);
  expect(echoesOf({ Transition: { entity: "jira:PAY-1", status: "In Review" } })).toEqual([
    { place: "state", to: "In Review" },
  ]);
});

test("a setting's edit draws nothing here: its row has its own mark", () => {
  expect(echoesOf({ SetParameter: { entity: "teamcity:bt:1", name: "x", value: "1", base: null } })).toEqual([]);
});

test("the mark says sending until the queue gives a reason it waits", () => {
  expect(sendingMark({ wait_reason: null })).toBe("sending…");
  expect(sendingMark({ wait_reason: "unreachable" })).toBe("waiting: source unreachable");
});
