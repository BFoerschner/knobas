// A press on a repository's link asks the reads its header waits on (the ux30 end gate, Windows, Fable's ruling: a
// page's reads go with the press). In knobas's window at CPU 6x the header's pull requests went a round after the row,
// behind the page's build, and its checkout once the page was built: each the page's own ask, found in the cache.
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { disableAnswerCache, enableAnswerCache } from "../../ipc/answer-cache";
import { entityCheckout, forgetCheckoutsAhead, forgetReadsAhead, listEntities } from "../../ipc/entity";
import { forgetLandingReads, readLandingAhead } from "./hover-reads";
import { REPO_PRS_LIMIT, repoPrsFilter } from "./repo";

type Bridge = { invoke: (cmd: string, args?: unknown) => Promise<unknown> };
let calls: { cmd: string; args: unknown }[];

const REPO = "live-gitea:scale-data/warehouse-loader";
const ROW = { entity_id: REPO, source_id: "live-gitea", kind: "repo", title: "scale-data/warehouse-loader" };

beforeEach(() => {
  calls = [];
  const bridge: Bridge = {
    invoke: vi.fn(async (cmd: string, args?: unknown) => {
      calls.push({ cmd, args });
      if (cmd === "list_entities") {
        const kinds = (args as { filter: { kinds: string[] } }).filter.kinds;
        return kinds.includes("repo") ? { rows: [ROW], total: 1 } : { rows: [], total: 0 };
      }
      if (cmd === "get_entity") return { row: ROW, tree: null };
      if (cmd === "entity_checkout") return { path: null, tree: null };
      return null;
    }),
  };
  const internals = {};
  Object.defineProperty(internals, "invoke", { value: (cmd: string, args?: unknown) => bridge.invoke(cmd, args) });
  Object.defineProperty(window, "__TAURI_INTERNALS__", { value: internals, configurable: true });
  enableAnswerCache();
  forgetLandingReads();
  forgetReadsAhead();
  forgetCheckoutsAhead();
});

afterEach(() => {
  disableAnswerCache();
  delete (window as unknown as { __TAURI_INTERNALS__?: Bridge }).__TAURI_INTERNALS__;
});

const asked = (cmd: string, kind?: string) =>
  calls.filter((c) => c.cmd === cmd && (kind === undefined || (c.args as { filter: { kinds: string[] } }).filter.kinds.includes(kind))).length;

test("a press on a repository the Code list holds asks its pull requests, and the page's own ask finds them", async () => {
  // The Code page's list, as it drew it.
  await listEntities({ sources: null, kinds: ["repo"], updated_within_days: null, order: "title_asc", context: null, project: null, include_deleted: false, reach: null } as never, 100, 0);
  readLandingAhead(REPO);
  expect(asked("list_entities", "pr")).toBe(1);
  // The page, once built, reads its pull requests from its row: the same read, asked no second time.
  await listEntities(repoPrsFilter(ROW), REPO_PRS_LIMIT, 0);
  expect(asked("list_entities", "pr")).toBe(1);
});

test("a press on a repository nothing holds the row of asks no pull requests: the page reads them from its row", () => {
  readLandingAhead(REPO);
  expect(asked("list_entities", "pr")).toBe(0);
});

test("a press asks the repository's checkout, and the page's first ask takes it; a later ask reads again", async () => {
  readLandingAhead(REPO);
  expect(asked("entity_checkout")).toBe(1);
  await entityCheckout(REPO);
  expect(asked("entity_checkout")).toBe(1);
  // The window's focus or a clone's end reads the checkout again.
  await entityCheckout(REPO);
  expect(asked("entity_checkout")).toBe(2);
});
