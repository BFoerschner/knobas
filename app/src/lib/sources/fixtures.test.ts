/**
 * The transcription rule in `fixtures.ts`, enforced instead of remembered.
 *
 * ## What this file can check, and what it cannot
 *
 * It cannot check the half that matters most. "A property that is here is
 * verbatim" is a claim about `crates/knobas-source-*`, and nothing under
 * `app/` can read a Rust `serde_json::json!` literal whose `maximum` is a
 * `const` — which is precisely the property that drifted (#124). Any check of
 * *that* has to run somewhere the adapters are in reach; the repo already has
 * the idiom for it, in `crates/knobas-app/tests/sources_mirror.rs`, where a
 * Rust test `include_str!`s a TypeScript file and compares it against a real
 * value. That is the recommended home and it is deliberately not built here.
 *
 * What this file does check is the rule's *other* half — the one about
 * absence, which is otherwise kept by memory alone. A property added to or
 * removed from a fixture is a two-place edit: the schema, and the list of
 * absences the module header names. Pinned here, forgetting the second place
 * is a red instead of a header that quietly stops being true.
 *
 * And it pins the one drift that made a fixture *stricter* than reality,
 * because that is the direction that hides: a test written against an
 * invented constraint passes while the real form accepts what the test
 * believes it rejects.
 */
import { expect, test } from "vitest";

import { EMPTY_SCHEMA, GITEA_SCHEMA, JIRA_SCHEMA, TEAMCITY_SCHEMA } from "./fixtures";

/**
 * What each fixture transcribes, and what it leaves out — the module header's
 * two lists, in the one form a test can compare against.
 *
 * Spelled out here rather than derived from the fixture, which would make the
 * check tautological: this list is the second witness, and the point is that
 * the two have to be changed together.
 */
const TRANSCRIBED = [
  {
    name: "JIRA_SCHEMA",
    schema: JIRA_SCHEMA as unknown,
    present: ["flavor", "projects", "jql_filter", "username"],
    absent: [
      "epic_link_field",
      "page_size",
      "rate_per_sec",
      "rate_burst",
      "connect_timeout_secs",
      "request_timeout_secs",
    ],
  },
  {
    name: "GITEA_SCHEMA",
    schema: GITEA_SCHEMA as unknown,
    present: ["owners", "repos", "username"],
    absent: ["commits_per_repo", "prs_per_repo", "include_pr_comments", "rate_limit_per_sec"],
  },
  {
    name: "TEAMCITY_SCHEMA",
    schema: TEAMCITY_SCHEMA as unknown,
    present: ["project_ids", "build_type_ids", "builds_per_config", "username"],
    absent: ["rate_limit_per_sec"],
  },
] as const;

function propertiesOf(schema: unknown): Record<string, Record<string, unknown>> {
  return (schema as { properties: Record<string, Record<string, unknown>> }).properties;
}

test.each(TRANSCRIBED)(
  "$name declares exactly the properties the module header says it does",
  ({ present, absent, schema }) => {
    const declared = Object.keys(propertiesOf(schema));
    // Order too: it is the adapter author's sequencing intent and the only
    // signal the descriptor carries about it, so `schemaFields` preserves it
    // and a fixture that reordered would draw a different form.
    expect(declared).toEqual([...present]);
    for (const key of absent) {
      expect(
        declared,
        `${key} is in the fixture but the header still lists it as absent — both have to move together`,
      ).not.toContain(key);
    }
  },
);

/**
 * The drift that mattered most, because it pointed the wrong way.
 *
 * Jira's fixture declared `required: ["flavor"]`; the adapter declares no
 * `required` at all, so the fixture-driven form emitted a required error the
 * real one never produces — and four tests were written against it. A fixture
 * that is *stricter* than the thing it stands for cannot be caught by a form
 * that works, only by somebody reading both. `required` is still a shape the
 * form model supports and is exercised by schemas written to say so about
 * themselves; what it may not be is invented on an adapter's behalf.
 *
 * If an adapter ever does declare one, this is where it is said out loud.
 */
test.each(TRANSCRIBED)("$name invents no `required` on an adapter's behalf", ({ schema }) => {
  expect(
    schema as Record<string, unknown>,
    "no M1 adapter declares `required`; a fixture that does makes the form stricter than the source",
  ).not.toHaveProperty("required");
});

/**
 * `username` is drawable as text in every fixture that has one.
 *
 * The convention the Add-source dialog's fill is keyed on (#82), and the exact
 * shape the first drift broke: `{type: "string"}` where the adapter says
 * `{type: ["string", "null"]}` reads as "not a string", draws a JSON textarea,
 * and makes typing a username a parse error unless the reader knows to quote
 * it. `sources_registry.rs` asserts the same convention over the real
 * descriptors; this asserts it over the corpus the form tests actually run on,
 * which is the copy that was wrong.
 */
test.each(TRANSCRIBED)("$name's username is a string the dialog can fill", ({ schema }) => {
  const username = propertiesOf(schema).username;
  expect(username, "every M1 adapter has a username").toBeTruthy();
  const type = username!.type;
  const named = Array.isArray(type) ? type.filter((member) => member !== "null") : [type];
  expect(named).toEqual(["string"]);
});

test("the mock adapter needs no configuring, which is the empty-form case", () => {
  expect(EMPTY_SCHEMA).toEqual({ type: "object", properties: {} });
});
