/**
 * The protocol's two pure rules (issue #289).
 *
 * The action-item fixture is **the same body** `knobas_app::protocol`'s
 * `action_items_are_the_bullets_under_their_own_heading` uses, and the
 * expectations are the same three items. That is what keeps one rule with two
 * implementations from becoming two rules: a change to either side that made
 * them disagree fails here or there.
 */
import { expect, test } from "vitest";

import { actionItems, presumedSource, publishableSources } from "./protocol";

/** The fixture the Rust battery uses, character for character. */
const BODY = `# Standup 2026-09-03

## Notes

- Jonas is on the payout retry
- [ ] this looks like an action item and is not one

## Action items

- [ ] Ask Ines about the SEPA retry
- [x] Book the postmortem
* Star bullets count
-
- [ ]

## Attendees

- Mara
`;

test("only the bullets under the action-items heading are action items", () => {
  expect(actionItems(BODY)).toEqual([
    { text: "Ask Ines about the SEPA retry", done: false },
    { text: "Book the postmortem", done: true },
    { text: "Star bullets count", done: false },
  ]);
});

test("a heading is a heading however it is capitalised", () => {
  expect(actionItems("### action ITEMS\n- [ ] ship it\n")).toEqual([
    { text: "ship it", done: false },
  ]);
});

test("an empty bullet is not an action item", () => {
  // The template ships exactly this, and a *Create ticket* button on it would
  // file a ticket called nothing.
  expect(actionItems("## Action items\n\n- [ ] \n")).toEqual([]);
});

const CONFLUENCE = { adapter_kind: "confluence", write_ops: ["comment", "create_page"] };
const JIRA = { adapter_kind: "jira", write_ops: ["comment", "create_ticket"] };

function source(id: string, kind: string, enabled = true) {
  return { id, display_name: id.toUpperCase(), adapter_kind: kind, enabled };
}

test("a source is offered because its adapter declares create_page", () => {
  const offered = publishableSources(
    [source("wiki", "confluence"), source("tracker", "jira")],
    [CONFLUENCE, JIRA],
  );
  expect(offered).toEqual([{ id: "wiki", name: "WIKI" }]);
});

test("an adapter that does not declare create_page offers nothing, whatever it is called", () => {
  // The same source id, the same adapter kind, one declaration removed. If the
  // rule read the name or the kind instead of the declaration, this would
  // still be offered — and `submit_write` would refuse the op it queued.
  expect(
    publishableSources([source("confluence", "confluence")], [
      { adapter_kind: "confluence", write_ops: ["comment"] },
    ]),
  ).toEqual([]);
});

test("a disabled source is not offered", () => {
  expect(publishableSources([source("wiki", "confluence", false)], [CONFLUENCE])).toEqual([]);
});

test("one Confluence needs no question and two have no default", () => {
  expect(presumedSource([{ id: "wiki", name: "WIKI" }])).toBe("wiki");
  // Story 68: knobas picking one would put the team's standup in the wrong
  // instance, so there is no answer until somebody gives one.
  expect(
    presumedSource([
      { id: "wiki", name: "WIKI" },
      { id: "wiki2", name: "WIKI2" },
    ]),
  ).toBeNull();
  expect(presumedSource([])).toBeNull();
});
