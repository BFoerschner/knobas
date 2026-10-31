/**
 * *Apply a link set…* and *Save as a link set…* (#869, R-7.8, R-7.9), pure:
 * the drafts they save, what the save dialog offers, and their words.
 */
import { describe, expect, test } from "vitest";

import type { LinkEnd, LinkSetRow } from "../../ipc/entity";
import { applyToast, epicAbove, epicOf, offered, parentOf, projectOf, savedDraft, savedName, savedToast, withHandRule, withoutHandRule } from "./apply";

const SET: LinkSetRow = {
  id: "payout-stack",
  name: "Payout stack",
  description: "The payout path.",
  members: [
    { kind: "repo", entity_id: "gitea:tidewater/payout-service", title: "tidewater/payout-service", line: null, state: null },
    { kind: "asset", entity_id: "asset:db", title: "db", line: null, state: null },
  ],
  patterns: [
    {
      kind: "pipeline",
      query: "project:Payouts name:Payout_*",
      count: 2,
      matches: [
        { kind: "pipeline", entity_id: "teamcity:Payout_Build", title: "Payout_Build", line: null, state: null },
        { kind: "pipeline", entity_id: "teamcity:Payout_Deploy", title: "Payout_Deploy", line: null, state: null },
      ],
    },
  ],
  rules: [{ kind: "epic", value: "jira:PAY-200", title: "Payout reliability" }],
  tickets: 4,
  mine: 1,
};

describe("applying by hand", () => {
  test("adds a by-hand rule for the ticket after the set's own, keeping members and patterns", () => {
    expect(withHandRule(SET, "jira:PAY-231")).toEqual({
      id: "payout-stack",
      name: "Payout stack",
      description: "The payout path.",
      members: [
        { kind: "repo", entity_id: "gitea:tidewater/payout-service" },
        { kind: "asset", entity_id: "asset:db" },
      ],
      patterns: [{ kind: "pipeline", query: "project:Payouts name:Payout_*" }],
      rules: [
        { kind: "epic", value: "jira:PAY-200" },
        { kind: "manual", value: "jira:PAY-231" },
      ],
    });
  });

  test("Undo takes that one rule out again and leaves every other rule", () => {
    const applied = { ...SET, rules: [...SET.rules, { kind: "manual" as const, value: "jira:PAY-231", title: null }] };
    expect(withoutHandRule(applied, "jira:PAY-231").rules).toEqual([{ kind: "epic", value: "jira:PAY-200" }]);
  });

  test("the toast counts every item the set holds, patterns' matches included", () => {
    expect(applyToast(SET, "PAY-231")).toBe("Payout stack applied to PAY-231. 4 items show on the job.");
  });
});

const end = (entity_id: string, kind: string, title: string): LinkEnd => ({ entity_id, kind, title, deleted_at: null });

describe("what Save as a link set offers", () => {
  test("the ticket's repos, pipelines, pages, monitors and assets; a PR, branch or commit offers its repository", () => {
    expect(
      offered([
        end("gitea:tidewater/payout-service#142", "pr", "Idempotent retry window"),
        end("gitea:tidewater/ledger@refs/heads/feat/x", "branch", "feat/x"),
        end("gitea:tidewater/payout-service@abc123", "commit", "Fix retry"),
        end("teamcity:Payout_Build", "build_config", "Payout_Build"),
        end("teamcity:run:77", "build", "#77"),
        end("confluence:ENG-SEPA", "page", "SEPA design"),
        end("asset:db", "asset", "db"),
        end("kuma:5", "monitor", "payout health"),
        end("jira:PAY-228", "ticket", "Another ticket"),
      ]),
    ).toEqual([
      { kind: "repo", entity_id: "gitea:tidewater/payout-service", title: "tidewater/payout-service" },
      { kind: "repo", entity_id: "gitea:tidewater/ledger", title: "tidewater/ledger" },
      { kind: "pipeline", entity_id: "teamcity:Payout_Build", title: "Payout_Build" },
      { kind: "page", entity_id: "confluence:ENG-SEPA", title: "SEPA design" },
      { kind: "monitor", entity_id: "kuma:5", title: "payout health" },
      { kind: "asset", entity_id: "asset:db", title: "db" },
    ]);
  });

  test("a repository linked directly keeps its own title", () => {
    expect(offered([end("gitea:tidewater/ledger", "repo", "ledger"), end("gitea:tidewater/ledger#3", "pr", "PR")])).toEqual([
      { kind: "repo", entity_id: "gitea:tidewater/ledger", title: "ledger" },
    ]);
  });
});

describe("where a saved set applies", () => {
  test("the epic is the top of the ticket's parent chain, in the ticket's source", () => {
    expect(epicOf({ entity_id: "jira:PAY-232", source_id: "jira", path: "PAY-200 › PAY-231" })).toBe("jira:PAY-200");
    expect(epicOf({ entity_id: "jira:PAY-231", source_id: "jira", path: "PAY-200" })).toBe("jira:PAY-200");
    expect(epicOf({ entity_id: "jira:PAY-200", source_id: "jira", path: null })).toBeNull();
  });

  test("a ticket's nearest parent is the bottom of its own path line, in its own source", () => {
    expect(parentOf({ entity_id: "jira:PAY-232", source_id: "jira", path: "PAY-200 › PAY-231" })).toBe("jira:PAY-231");
    expect(parentOf({ entity_id: "jira:PAY-231", source_id: "jira", path: "PAY-200" })).toBe("jira:PAY-200");
    expect(parentOf({ entity_id: "jira:PAY-200", source_id: "jira", path: null })).toBeNull();
  });

  /** A stand-in `get_entity` over `key → path line`, recording what it read. */
  function chain(paths: Record<string, string | null>, read: string[] = []) {
    return (id: string) => {
      read.push(id);
      const key = id.slice(id.indexOf(":") + 1);
      if (!(key in paths)) return Promise.reject({ code: "not_found", message: "", source_id: null });
      return Promise.resolve({ row: { entity_id: id, source_id: "jira", title: `${key} title`, path: paths[key] ?? null } });
    };
  }

  test("a sub-task's epic is found by walking up through its story", async () => {
    const read: string[] = [];
    const get = chain({ "PAY-231": "PAY-200", "PAY-200": null }, read);
    expect(await epicAbove({ entity_id: "jira:PAY-232", source_id: "jira", path: "PAY-231" }, get)).toEqual({
      entity_id: "jira:PAY-200",
      title: "PAY-200 title",
    });
    expect(read).toEqual(["jira:PAY-231", "jira:PAY-200"]);
  });

  test("a story under an epic and a ticket with no parent read as before", async () => {
    const get = chain({ "PAY-200": null });
    expect(await epicAbove({ entity_id: "jira:PAY-231", source_id: "jira", path: "PAY-200" }, get)).toEqual({
      entity_id: "jira:PAY-200",
      title: "PAY-200 title",
    });
    expect(await epicAbove({ entity_id: "jira:PAY-200", source_id: "jira", path: null }, get)).toBeNull();
  });

  test("an unreadable ancestor or a loop stops the walk where it is", async () => {
    expect(await epicAbove({ entity_id: "jira:PAY-232", source_id: "jira", path: "PAY-231" }, chain({}))).toEqual({
      entity_id: "jira:PAY-231",
      title: null,
    });
    const loop = chain({ "PAY-1": "PAY-2", "PAY-2": "PAY-1" });
    expect(await epicAbove({ entity_id: "jira:PAY-3", source_id: "jira", path: "PAY-1" }, loop)).toEqual({
      entity_id: "jira:PAY-2",
      title: "PAY-2 title",
    });
  });

  test("the project is the key's prefix in the ticket's source", () => {
    expect(projectOf({ entity_id: "jira:PAY-231", source_id: "jira" })).toBe("jira:PAY");
    expect(projectOf({ entity_id: "jira:nokey", source_id: "jira" })).toBeNull();
  });

  test("the draft holds the ticked items in order and the one rule, saved from the ticket", () => {
    expect(
      savedDraft("Payout kit", "PAY-231", [{ kind: "repo", entity_id: "gitea:a/b", title: "a/b" }], { kind: "epic", value: "jira:PAY-200" }),
    ).toEqual({
      id: null,
      name: "Payout kit",
      description: "Saved from PAY-231.",
      members: [{ kind: "repo", entity_id: "gitea:a/b" }],
      patterns: [],
      rules: [{ kind: "epic", value: "jira:PAY-200" }],
    });
  });

  test("the saved toast names the set and the rule that now shows it", () => {
    expect(savedToast({ name: "Payout kit", rules: [{ kind: "project", value: "jira:PAY", title: null }] })).toBe(
      "Saved Payout kit. Every ticket in project PAY now shows it.",
    );
  });

  test("the name it suggests is the summary's last two words and kit", () => {
    expect(savedName("Retry failed SEPA payouts")).toBe("SEPA payouts kit");
  });
});
