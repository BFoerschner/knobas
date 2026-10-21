/**
 * A branch's or a commit's address (#1440): `#/branch/<id>` and
 * `#/commit/<id>` send the reader to the repository the entity's declared
 * parent names. The bridge is injected; what `get_entity` and
 * `list_entities` answer is `crates/knobas-app/tests/it/entity.rs`'s.
 */
import { flushSync, mount, tick, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { EntityDetail, EntityFilter, EntityPage, EntityRow } from "../../ipc/entity";
import { demoHandlers } from "../../shell/dev/fake-tauri";
import TfCodeRow, { type CodeRowPorts } from "./TfCodeRow.svelte";

const REPO_ID = "gitea:tidewater/payout-service";
const BRANCH_ID = `${REPO_ID}@refs/heads/feat/PAY-231-idempotent-retry`;
const COMMIT_ID = "gitea:9f2c1ab";

function row(entity_id: string, kind: string, title: string, path: string | null, source_id = "gitea"): EntityRow {
  return { entity_id, kind, source_id, title, updated_at: null, synced_at: "2026-08-22T14:30:00Z", path, state: null };
}

const REPOS = [
  row("gitea:tidewater/ledger", "repo", "tidewater/ledger", "tidewater"),
  row(REPO_ID, "repo", "tidewater/payout-service", "tidewater"),
];

function detail(of: EntityRow): EntityDetail {
  return { row: of } as EntityDetail;
}

function ports(entity: EntityRow | Error | { code: string; message: string }, repos = REPOS) {
  const replace = vi.fn<(hash: string) => void>();
  const listEntities = vi.fn(async (filter: EntityFilter) => {
    const rows = repos.filter((r) => filter.sources?.includes(r.source_id) && filter.kinds?.includes(r.kind));
    return { rows, total: rows.length };
  });
  const answer: CodeRowPorts = {
    getEntity: async () => {
      if ("entity_id" in entity) return detail(entity);
      throw entity;
    },
    listEntities: listEntities as unknown as CodeRowPorts["listEntities"],
    replace,
  };
  return { answer, replace, listEntities };
}

let target: HTMLDivElement;
let view: ReturnType<typeof mount> | null = null;

beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
});
afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
});

async function settle() {
  for (let round = 0; round < 6; round++) {
    await tick();
    await Promise.resolve();
  }
  flushSync();
}

function open(head: "branch" | "commit", id: string, with_: CodeRowPorts, ready = true) {
  view = mount(TfCodeRow, { target, props: { head, id, ready, ports: with_ } });
}

test("a branch lands on its repository's Branches tab, in place of its own address (R-8.1)", async () => {
  const { answer, replace } = ports(row(BRANCH_ID, "branch", "feat/PAY-231-idempotent-retry", "tidewater/payout-service"));
  open("branch", BRANCH_ID, answer);
  await settle();
  expect(replace).toHaveBeenCalledExactlyOnceWith("#/repo/gitea%3Atidewater%2Fpayout-service/branches");
});

test("a commit lands on its own page under its repository's Commits (R-8.7)", async () => {
  const { answer, replace } = ports(row(COMMIT_ID, "commit", "payout: key the retry on the mandate id", "tidewater/payout-service"));
  open("commit", COMMIT_ID, answer);
  await settle();
  expect(replace).toHaveBeenCalledExactlyOnceWith("#/repo/gitea%3Atidewater%2Fpayout-service/commits/gitea%3A9f2c1ab");
});

test("the repository is read in the entity's own source, never guessed from its id", async () => {
  const elsewhere = [row("other:tidewater/payout-service", "repo", "tidewater/payout-service", "tidewater", "other")];
  const { answer, replace, listEntities } = ports(row(BRANCH_ID, "branch", "feat/x", "tidewater/payout-service"), elsewhere);
  open("branch", BRANCH_ID, answer);
  await settle();
  expect(listEntities.mock.calls[0]?.[0]).toMatchObject({ sources: ["gitea"], kinds: ["repo"] });
  expect(replace).not.toHaveBeenCalled();
  expect(target.textContent).toContain("feat/x");
});

test("a branch or commit with no declared parent draws a plain not-found with its title, and nothing throws", async () => {
  const { answer, replace, listEntities } = ports(row(COMMIT_ID, "commit", "payout: key the retry on the mandate id", null));
  open("commit", COMMIT_ID, answer);
  await settle();
  expect(replace).not.toHaveBeenCalled();
  expect(listEntities).not.toHaveBeenCalled();
  expect(target.querySelector(".tf-empty")?.textContent?.replace(/\s+/g, " ").trim()).toBe(
    "payout: key the retry on the mandate id is not under a repository in the mirror. Go to Code",
  );
  expect(target.querySelector(".tf-empty a")?.getAttribute("href")).toBe("#/code");
});

test("an id the mirror does not hold reads Nothing lives at, as every other entity page does", async () => {
  const { answer, replace } = ports({ code: "not_found", message: "gone" });
  open("branch", "gitea:nope", answer);
  await settle();
  expect(replace).not.toHaveBeenCalled();
  expect(target.textContent).toContain("Nothing lives at");
});

test("a failed read says so, with the address's id", async () => {
  const { answer, replace } = ports({ code: "internal", message: "database is down" });
  open("commit", COMMIT_ID, answer);
  await settle();
  expect(replace).not.toHaveBeenCalled();
  expect(target.textContent).toContain(`Could not read ${COMMIT_ID}: database is down`);
});

test("nothing is read before the database answers", async () => {
  const { answer, replace, listEntities } = ports(row(BRANCH_ID, "branch", "feat/x", "tidewater/payout-service"));
  open("branch", BRANCH_ID, answer, false);
  await settle();
  expect(listEntities).not.toHaveBeenCalled();
  expect(replace).not.toHaveBeenCalled();
});

test("?fake-ipc: job PAY-231's branch and commit rows land on payout-service's Branches tab and the commit's page", async () => {
  type Calls = Record<string, (args: Record<string, unknown>) => unknown>;
  const h = demoHandlers() as Calls;
  const replace = vi.fn<(hash: string) => void>();
  const fake: CodeRowPorts = {
    getEntity: async (entityId) => h["get_entity"]!({ entityId }) as EntityDetail,
    listEntities: async (filter, limit, offset) => h["list_entities"]!({ filter, limit, offset }) as EntityPage,
    replace,
  };
  open("branch", "mock:tidewater/payout-service@refs/heads/feat/PAY-231-idempotent-retry", fake);
  await settle();
  unmount(view!);
  open("commit", "mock:9f2c1ab", fake);
  await settle();
  expect(replace.mock.calls).toEqual([
    ["#/repo/mock%3Atidewater%2Fpayout-service/branches"],
    ["#/repo/mock%3Atidewater%2Fpayout-service/commits/mock%3A9f2c1ab"],
  ]);
});
