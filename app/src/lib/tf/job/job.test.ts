/** A job address read against two sources holding the same key (#1476). */
import { expect, test } from "vitest";

import type { EntityDetail } from "../../ipc/entity";
import { footAction, loggedRow, resolveJob, resolveTicket } from "./job";

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

// ---- the card's Logged row (#2403) ----

const logged = (
  time_spent_seconds?: number,
  estimate_seconds: number | null = null,
  logged_by?: { author: string; seconds: number }[],
): Pick<EntityDetail, "time_spent_seconds" | "estimate_seconds" | "logged_by"> => ({
  estimate_seconds,
  ...(time_spent_seconds === undefined ? {} : { time_spent_seconds }),
  ...(logged_by === undefined ? {} : { logged_by }),
});

test("the Logged row: spent of the estimate over a meter, the time past the estimate its own segment in the error tint", () => {
  expect(loggedRow(logged(7800, 6 * 3600), "mara")).toEqual({
    text: "2h 10m of 6h",
    yours: null,
    meter: { mine: 0, others: (7800 / 21600) * 100, past: 0 },
  });
  // 7h against 6h: the track is the 7h, the estimate's 6h fill it as logged, the hour past it is its own segment.
  expect(loggedRow(logged(7 * 3600, 6 * 3600), "mara")?.meter).toEqual({ mine: 0, others: (6 / 7) * 100, past: (1 / 7) * 100 });
  expect(loggedRow(logged(6 * 3600, 6 * 3600), "mara")?.meter?.past, "exactly the estimate is not over it").toBe(0);
  // Mine first within the estimate, then the others' up to it; what is past it is past it, whoever logged it.
  const over = logged(7 * 3600, 6 * 3600, [
    { author: "mara", seconds: 3600 },
    { author: "lena", seconds: 6 * 3600 },
  ]);
  expect(loggedRow(over, "mara")?.meter).toEqual({ mine: (1 / 7) * 100, others: (5 / 7) * 100, past: (1 / 7) * 100 });
});

test("the Logged row tells my share only when somebody else has logged too", () => {
  const shared = logged(7800, 6 * 3600, [
    { author: "lena", seconds: 1800 },
    { author: "mara", seconds: 6000 },
  ]);
  expect(loggedRow(shared, "mara")).toEqual({
    text: "2h 10m of 6h",
    yours: "1h 40m yours",
    meter: { mine: (6000 / 21600) * 100, others: (1800 / 21600) * 100, past: 0 },
  });
  // All of it mine: nothing to tell apart, one segment.
  const alone = logged(6000, 6 * 3600, [{ author: "mara", seconds: 6000 }]);
  expect(loggedRow(alone, "mara")).toMatchObject({ yours: null, meter: { mine: 0 } });
  // None of it mine, or no username to match: one segment, no share.
  expect(loggedRow(shared, "jonas")?.yours).toBeNull();
  expect(loggedRow(shared, null)?.yours).toBeNull();
});

test("the Logged row: the time alone with no estimate, 0m of an estimate with nothing logged, no row with neither", () => {
  expect(loggedRow(logged(13200), "mara")).toEqual({ text: "3h 40m", yours: null, meter: null });
  expect(loggedRow(logged(13200, 0), "mara"), "a zero estimate is no track to draw").toEqual({ text: "3h 40m", yours: null, meter: null });
  expect(loggedRow(logged(undefined, 2 * 3600), "mara")).toEqual({
    text: "0m of 2h",
    yours: null,
    meter: { mine: 0, others: 0, past: 0 },
  });
  expect(loggedRow(logged(), "mara")).toBeNull();
});
