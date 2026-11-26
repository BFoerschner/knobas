// A press on a ticket's card asks the reads its page waits on (the ux30 end gate, Windows, Fable's ruling): the sources,
// what the key's candidates are, the ticket, its card's sections and its sub-tasks, each as the page asks it, so the
// page finds every one in the answer cache and asks none of them again.
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { disableAnswerCache, enableAnswerCache } from "../../ipc/answer-cache";
import { cardReads, entityBriefs, getEntity } from "../../ipc/entity";
import { listSources } from "../../ipc/sources";
import { resolveJob, segmentKey } from "./job";
import { bridgeTicketBoard } from "./ports";
import { forgetTicketReads, readTicketAhead } from "./press-reads";
import { cardFilters, SECTION_LIMIT } from "./sections";

type Bridge = { invoke: (cmd: string, args?: unknown) => Promise<unknown> };
let calls: { cmd: string; args: unknown }[];

const ID = "jira:PAY-12";
const ROW = { entity_id: ID, source_id: "jira", kind: "ticket", title: "Pay the bill" };

beforeEach(() => {
  calls = [];
  const bridge: Bridge = {
    invoke: vi.fn(async (cmd: string, args?: unknown) => {
      calls.push({ cmd, args });
      if (cmd === "list_sources") return [{ id: "jira", roles: [{ role: "ticket-system" }] }, { id: "gitea", roles: [{ role: "code-host" }] }];
      if (cmd === "entity_briefs") return [{ entity_id: ID, kind: "ticket", title: ROW.title }];
      if (cmd === "get_entity") return { row: ROW };
      if (cmd === "card_reads") return { reach: [], sections: [] };
      if (cmd === "mini_board") return { columns: [], lanes: [], more_lanes: 0 };
      return null;
    }),
  };
  const internals = {};
  Object.defineProperty(internals, "invoke", { value: (cmd: string, args?: unknown) => bridge.invoke(cmd, args) });
  Object.defineProperty(window, "__TAURI_INTERNALS__", { value: internals, configurable: true });
  enableAnswerCache();
  forgetTicketReads();
});

afterEach(() => {
  disableAnswerCache();
  delete (window as unknown as { __TAURI_INTERNALS__?: Bridge }).__TAURI_INTERNALS__;
});

const asked = (cmd: string) => calls.filter((c) => c.cmd === cmd).length;

test("a press asks the ticket page's reads, and the page's own asks find each of them", async () => {
  readTicketAhead(ID);
  // The ticket, its card's sections and its sub-tasks go at once; the candidates once the sources answer.
  expect(asked("get_entity")).toBe(1);
  expect(asked("card_reads")).toBe(1);
  expect(asked("mini_board")).toBe(1);
  await vi.waitFor(() => expect(asked("entity_briefs")).toBe(1));
  // The page, once built (`job-reader.svelte.ts`'s `readJob`): the same reads, none asked again.
  const listed = await listSources();
  const job = await resolveJob(ID, listed, getEntity, entityBriefs);
  await cardReads(ID, cardFilters(ID), SECTION_LIMIT);
  await bridgeTicketBoard(`under:${segmentKey(ID)}`);
  expect(job?.ticket.row.entity_id).toBe(ID);
  expect([asked("list_sources"), asked("entity_briefs"), asked("get_entity"), asked("card_reads"), asked("mini_board")]).toEqual([1, 1, 1, 1, 1]);
});

test("a press and the click after it ask once", () => {
  readTicketAhead(ID);
  readTicketAhead(ID);
  expect(asked("get_entity")).toBe(1);
});
