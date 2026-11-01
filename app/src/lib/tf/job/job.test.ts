/** A job address read against two sources holding the same key (#1476). */
import { expect, test } from "vitest";

import type { EntityDetail } from "../../ipc/entity";
import { footAction, resolveJob, resolveTicket } from "./job";

const SOURCES = [{ id: "mock" }, { id: "gitea" }, { id: "jira" }];
/** The mirror: PAY-231 in the mock source and in Jira, PAY-9 in Jira alone. */
const HELD = ["mock:PAY-231", "jira:PAY-231", "jira:PAY-9"];

function read(entityId: string): Promise<EntityDetail> {
  if (!HELD.includes(entityId)) return Promise.reject({ code: "not_found", message: `${entityId} is not in the mirror` });
  return Promise.resolve({ row: { entity_id: entityId, kind: "ticket" } } as EntityDetail);
}

const ids = (details: EntityDetail[]) => details.map((detail) => detail.row.entity_id);

test("an address holding a source names its ticket, though another source is listed first", async () => {
  expect((await resolveTicket("jira:PAY-231", SOURCES, read))?.row.entity_id).toBe("jira:PAY-231");
  const job = await resolveJob("jira:PAY-231", SOURCES, read);
  expect(job?.ticket.row.entity_id).toBe("jira:PAY-231");
  expect(ids(job!.others)).toEqual(["mock:PAY-231"]);
});

test("a bare key two sources hold opens the first listed and names the other, never silently", async () => {
  const job = await resolveJob("PAY-231", SOURCES, read);
  expect(job?.ticket.row.entity_id).toBe("mock:PAY-231");
  expect(ids(job!.others)).toEqual(["jira:PAY-231"]);
});

test("a key one source holds opens as before, with nobody else", async () => {
  const job = await resolveJob("PAY-9", SOURCES, read);
  expect(job?.ticket.row.entity_id).toBe("jira:PAY-9");
  expect(job?.others).toEqual([]);
  expect(await resolveJob("PAY-404", SOURCES, read)).toBeNull();
  expect(await resolveJob("gitea:PAY-231", SOURCES, read)).toBeNull();
});

test("a read that fails for another reason than a miss is said, not taken for one", async () => {
  const down = () => Promise.reject({ code: "database", message: "the database is down" });
  await expect(resolveJob("PAY-231", SOURCES, down)).rejects.toMatchObject({ code: "database" });
});

test("the card's foot: the decision while reading, the timer once pinned, nothing on a done ticket read (spec #2396)", () => {
  const reading = { pinned: false, timerOnIt: false };
  expect(footAction({ ...reading, status: "To Do", mine: true })).toEqual({ label: "Start work", does: "start-work" });
  expect(footAction({ ...reading, status: "In Progress", mine: true })).toEqual({ label: "Resume work", does: "resume" });
  expect(footAction({ ...reading, status: "In Review", mine: true })?.label).toBe("Resume work");
  expect(footAction({ ...reading, status: "To Do", mine: false })).toEqual({ label: "Work on this", does: "choose" });
  expect(footAction({ ...reading, status: null, mine: false })?.label).toBe("Work on this");
  expect(footAction({ ...reading, status: "Done", mine: true })).toBeNull();
  expect(footAction({ ...reading, status: "done", mine: false })).toBeNull();

  const pinned = { pinned: true, status: "Done", mine: false };
  expect(footAction({ ...pinned, timerOnIt: true })).toEqual({ label: "Stop and log", does: "stop" });
  expect(footAction({ ...pinned, timerOnIt: false })).toEqual({ label: "Start timer", does: "start-timer" });
});
