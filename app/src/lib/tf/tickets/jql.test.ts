/** *Make an editable copy*'s reading of a Jira view's JQL (#880, R-3.25). */
import { describe, expect, test } from "vitest";

import type { JiraSource } from "../../ipc/search";
import { translateJira, translateJql } from "./jql";

function source(jql: string, extra: Partial<JiraSource> = {}): JiraSource {
  return {
    kind: "filter",
    board_type: null,
    jql,
    sub_query: null,
    sprint: null,
    last_read: "2026-10-17T09:56:00Z",
    web_url: null,
    ...extra,
  };
}

describe("the seeded Jira views (testenv/seed-atlassian-content.sh)", () => {
  test("the favourite filter keeps its project and leaves out the fix version, saying why", () => {
    const copy = translateJira(source("project = PAY AND fixVersion is EMPTY ORDER BY updated DESC"));
    expect(copy.query).toBe("project:PAY");
    expect(copy.left).toEqual([{ clause: "fixVersion is EMPTY", reason: "knobas does not sync fix versions" }]);
    expect(copy.kept).toBe(1);
  });

  test("the Kanban board keeps both projects and leaves out Jira's default sub-filter", () => {
    const copy = translateJira(
      source("project in (PAY, OPS) ORDER BY Rank ASC", {
        kind: "board",
        board_type: "kanban",
        sub_query: "fixVersion in unreleasedVersions() OR fixVersion is EMPTY",
      }),
    );
    expect(copy.query).toBe("project:PAY project:OPS");
    expect(copy.left).toEqual([
      {
        clause: "fixVersion in unreleasedVersions() OR fixVersion is EMPTY",
        reason: "knobas does not sync fix versions",
      },
    ]);
    expect(copy.kept).toBe(1);
  });

  test("a Scrum board's active sprint is a condition, as Jira adds it to the board's query", () => {
    const copy = translateJira(source("project = PAY ORDER BY Rank ASC", { kind: "board", board_type: "scrum" }));
    expect(copy.query).toBe("project:PAY sprint:current");
    expect(copy.left).toEqual([]);
    expect(copy.kept).toBe(2);
  });
});

describe("clauses knobas can test", () => {
  test.each([
    ["status = Done", "status:Done"],
    ['status = "In Review"', 'status:"In Review"'],
    ["status != Done", "-status:Done"],
    ["status not in (Done, Closed)", "-status:Done -status:Closed"],
    ["issuetype = Bug", "issuetype:Bug"],
    ["type = Story", "issuetype:Story"],
    ["priority in (High, Highest)", "priority:High priority:Highest"],
    ["assignee = currentUser()", "assignee:me"],
    ["assignee = jbecker", "assignee:jbecker"],
    ["issuetype not in (Epic)", "-issuetype:Epic"],
    ["labels = on-call", "label:on-call"],
    ["sprint in openSprints()", "sprint:current"],
    ["parent = PAY-12", "under:PAY-12"],
    ['"Epic Link" = PAY-12', "under:PAY-12"],
    ["updated >= -7d", "updated:7d"],
    ["updated > -2w", "updated:2w"],
    ["PROJECT = pay", "project:pay"],
    ["project = 'PAY'", "project:PAY"],
  ])("%s reads as %s", (jql, query) => {
    const copy = translateJql(jql);
    expect(copy.left).toEqual([]);
    expect(copy.query).toBe(query);
  });

  test("conditions keep the query's order, and ORDER BY is not a part", () => {
    const copy = translateJql("project in (PAY, OPS) AND status != Done ORDER BY Rank ASC");
    expect(copy.query).toBe("project:PAY project:OPS -status:Done");
    expect(copy.kept).toBe(2);
    expect(copy.left).toEqual([]);
  });

  test("an empty query keeps nothing and leaves nothing out", () => {
    expect(translateJql("ORDER BY Rank ASC")).toEqual({ query: "", kept: 0, left: [] });
  });
});

describe("clauses knobas leaves out, with the reason", () => {
  test.each([
    ["component = on-call", "knobas does not sync components"],
    ["fixVersion = 2.4", "knobas does not sync fix versions"],
    ["affectedVersion = 2.3", "knobas does not sync affected versions"],
    ["resolution = Unresolved", "knobas does not sync resolutions"],
    ["statusCategory != Done", "knobas does not sync status categories"],
    ["issueFunction in linkedIssuesOf(\"project = PAY\")", "a ScriptRunner function, only Jira can answer it"],
    ["assignee in membersOf(payments)", "a Jira function, only Jira can answer it"],
    ["summary ~ refund", "a Jira text search, only Jira can answer it"],
    ["status was Done", "knobas does not keep a ticket's history"],
    ["assignee is EMPTY", "knobas has no condition for an empty field"],
    ["sprint = 41", "knobas knows only the active sprint"],
    ["parent != PAY-12", "knobas can only keep the tickets under an epic"],
    ["created >= -7d", "knobas has no condition for created"],
    ["updated <= -7d", "knobas tests only how recently a ticket changed"],
    ["NOT status = Done", "knobas cannot read this part of the query"],
    ["assignee != currentUser()", "knobas would also keep the tickets with no assignee"],
    ["labels != tech-debt", "knobas would also keep the tickets with no label"],
    ["sprint not in openSprints()", "knobas would also keep the tickets with no sprint"],
    ["priority not in (Low)", "knobas would also keep the tickets with no priority"],
  ])("%s: %s", (jql, reason) => {
    const copy = translateJql(`project = PAY AND ${jql}`);
    expect(copy.query).toBe("project:PAY");
    expect(copy.left).toEqual([{ clause: jql, reason }]);
  });

  test("an OR outside parentheses makes the whole query one part, as JQL binds AND tighter", () => {
    const jql = "project = PAY AND status = Done OR priority = High ORDER BY Rank ASC";
    expect(translateJql(jql)).toEqual({
      query: "",
      kept: 0,
      left: [
        {
          clause: "project = PAY AND status = Done OR priority = High",
          reason: "an OR between conditions, which knobas cannot say",
        },
      ],
    });
  });

  test("an OR inside parentheses stays in its own part", () => {
    const copy = translateJql("project = PAY AND (status = Done OR priority = High)");
    expect(copy.query).toBe("project:PAY");
    expect(copy.left.map((part) => part.clause)).toEqual(["(status = Done OR priority = High)"]);
  });

  test("an OR over a field knobas does not sync names the field", () => {
    expect(translateJql("component = a OR status = Done").left[0]?.reason).toBe("knobas does not sync components");
  });

  test("a second clause on the same field and sign is left out: knobas would read the two as either", () => {
    const copy = translateJql("labels = a AND labels = b");
    expect(copy.query).toBe("label:a");
    expect(copy.left).toEqual([{ clause: "labels = b", reason: "knobas reads two values of one field as either" }]);
  });

  test("an AND inside parentheses or quotes does not split the clause", () => {
    const copy = translateJql('(status = Done AND priority = High) AND summary ~ "a AND b"');
    expect(copy.left.map((part) => part.clause)).toEqual(["(status = Done AND priority = High)", 'summary ~ "a AND b"']);
  });
});
