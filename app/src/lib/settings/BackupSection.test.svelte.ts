/**
 * The backup section of the settings view — spec §14's "small settings surface
 * (export now / schedule / restore)", issue #69.
 *
 * What is pinned here is what a reader can find out and what they can do about
 * it: what the schedule will actually do, where the archives are, that an
 * export reports where it went, and that a restore says what it will do before
 * it does it and says what failed when it does not. The internal shape of the
 * component is not tested — every assertion below goes through the rendered
 * view, the way a person meets it.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { BackupSchedule, BackupStatus } from "../ipc/backup";
import { settleRejections, takeUnhandled } from "../shell/unhandled";

const NOW = new Date("2026-08-29T09:14:00Z");
const DIR = "/Users/x/Library/Application Support/dev.knobas.desktop/backups";

/** Plain recorders, not `vi.fn`s, so the mock factory can close over them. */
const calls = {
  status: 0,
  now: 0,
  setSchedule: [] as BackupSchedule[],
  restore: [] as string[],
};

let status: BackupStatus;
let statusFails: unknown = null;
/**
 * Answers `backup_status` per call, when a test needs two reads in flight at
 * once. `status` alone cannot express that: it is read at resolution time, so
 * every pending read would answer with the same snapshot. Same shape as the
 * sources view's `answerList` — the two views share the hazard.
 */
let answerStatus: ((call: number) => Promise<BackupStatus>) | null = null;
let nowFails: unknown = null;
let restoreFails: unknown = null;
/**
 * What `set_backup_schedule` *stores*, given what was posted.
 *
 * The default is an echo, which is the ordinary case and is also why an echo
 * cannot on its own tell "redrew from the answer" apart from "redrew from the
 * draft". A test that needs the two distinguished sets this to something that
 * differs — which is not a contrivance: `BackupSchedule::clamped` is applied
 * on the Rust side after the post, so a posted schedule and a stored one are
 * allowed to differ.
 */
let stores: (posted: BackupSchedule) => BackupSchedule = (posted) => posted;

vi.mock("../ipc/backup", () => ({
  backupStatus: () => {
    calls.status += 1;
    if (answerStatus) return answerStatus(calls.status);
    return statusFails ? Promise.reject(statusFails) : Promise.resolve(status);
  },
  backupNow: () => {
    calls.now += 1;
    if (nowFails) return Promise.reject(nowFails);
    return Promise.resolve({
      taken_at: NOW.toISOString(),
      file: "knobas-20260829-091400.knobas",
      bytes: 4_194_304,
    });
  },
  setBackupSchedule: (schedule: BackupSchedule) => {
    calls.setSchedule.push(schedule);
    status = { ...status, schedule: stores(schedule) };
    return Promise.resolve(status);
  },
  restoreBackup: (file: string) => {
    calls.restore.push(file);
    return restoreFails ? Promise.reject(restoreFails) : Promise.resolve();
  },
}));

const { default: BackupSection } = await import("./BackupSection.svelte");
const { toasts } = await import("../shell/toasts.svelte");

function schedule(over: Partial<BackupSchedule> = {}): BackupSchedule {
  return { enabled: true, hour: 3, minute: 0, keep: 7, ...over };
}

function statusOf(over: Partial<BackupStatus> = {}): BackupStatus {
  return {
    schedule: schedule(),
    directory: DIR,
    last: {
      taken_at: "2026-08-29T01:00:00Z",
      file: "knobas-20260829-030000.knobas",
      bytes: 4_194_304,
    },
    next_due_at: "2026-08-30T01:00:00Z",
    archives: [
      { file: "knobas-20260829-030000.knobas", bytes: 4_194_304 },
      { file: "knobas-20260828-030000.knobas", bytes: 4_100_000 },
    ],
    ...over,
  };
}

let target: HTMLDivElement;
let app: Record<string, unknown> | undefined;

function render() {
  app = mount(BackupSection, { target, props: { now: NOW } });
  flushSync();
  return app;
}

async function settle() {
  for (let i = 0; i < 6; i += 1) await Promise.resolve();
  flushSync();
}

/** A button by its visible label, anywhere in the section. */
function button(label: string, within: ParentNode = target) {
  return [...within.querySelectorAll<HTMLButtonElement>("button")].find(
    (b) => b.textContent?.trim() === label,
  );
}

function dialog() {
  return target.querySelector<HTMLElement>('[role="dialog"]');
}

function text() {
  return target.textContent ?? "";
}

beforeEach(() => {
  calls.status = 0;
  calls.now = 0;
  calls.setSchedule = [];
  calls.restore = [];
  status = statusOf();
  statusFails = null;
  answerStatus = null;
  nowFails = null;
  restoreFails = null;
  stores = (posted) => posted;
  toasts.items = [];
  target = document.createElement("div");
  document.body.append(target);
});

afterEach(() => {
  if (app) unmount(app);
  app = undefined;
  target.remove();
});

/**
 * The reading a person came for: what will happen, and where the files are.
 *
 * The `at 03:00` assertion is the load-bearing one and it is a *negative*:
 * issue #69 rules that the wording may not promise a fixed time, and the way
 * that rule gets broken is by someone writing the shorter, wronger sentence.
 */
test("the section states the schedule as a boundary, the retention, and the directory", async () => {
  render();
  await settle();

  expect(text()).toContain("after 03:00");
  expect(text()).not.toContain("at 03:00");
  expect(text()).toContain("7 archives");
  expect(text()).toContain(DIR);
});

/** A stamp and a size, not just "backed up": the question is *when*, and *did it work*. */
test("the last export is named, sized and dated", async () => {
  render();
  await settle();

  expect(text()).toContain("knobas-20260829-030000.knobas");
  expect(text()).toContain("4 MB");
  expect(text()).toContain("8 h ago");
});

/**
 * "No backups yet" is a fact; an empty stamp is a shrug. The engine returns
 * `last: null` on a knobas that has never exported, and that is the state a
 * new install is in for its first day.
 */
test("a knobas that has never exported says so", async () => {
  status = statusOf({ last: null, archives: [] });
  render();
  await settle();

  expect(text()).toMatch(/no backup|never/i);
  // Not a *Last export* line with nothing in it: an em dash where a file name
  // belongs reads as a backup that happened and cannot be named.
  expect(text()).not.toContain("Last export");
});

/**
 * *Export now* has to say **where the file went**, not merely that it worked.
 *
 * An archive is a thing a person later has to find in a file manager, and
 * "Backup complete" is the message that makes them go looking. The name and
 * the directory are both on screen afterwards: the directory because it is
 * always there, the name because the re-read has moved the *last export* line
 * onto the file that was just written.
 */
test("Export now names the file it wrote and where it went", async () => {
  render();
  await settle();

  status = statusOf({
    last: { taken_at: NOW.toISOString(), file: "knobas-20260829-091400.knobas", bytes: 4_194_304 },
    archives: [{ file: "knobas-20260829-091400.knobas", bytes: 4_194_304 }],
  });
  button("Export now")!.click();
  await settle();

  expect(calls.now).toBe(1);
  expect(text()).toContain("knobas-20260829-091400.knobas");
  expect(text()).toContain(DIR);
  expect(toasts.items.map((toast) => toast.text).join(" ")).toContain(
    "knobas-20260829-091400.knobas",
  );
});

/**
 * The archive list is re-read, not appended to.
 *
 * `export_now` writes a file *and* prunes past `keep`, so the list afterwards
 * is not the list before plus a row — an eighth export with `keep: 7` removes
 * one. A section that patched its own list would show an archive that is no
 * longer on disk, and offer to restore it.
 */
test("an export re-reads what is on disk rather than assuming", async () => {
  render();
  await settle();
  const readOnce = calls.status;

  button("Export now")!.click();
  await settle();

  expect(calls.status).toBeGreaterThan(readOnce);
});

/**
 * `pg_dump`'s stderr is the message, because it is the only thing that says
 * *why* — a full disk and a missing tool are different problems with different
 * fixes, and "Export failed" distinguishes neither.
 */
test("a failed export says what failed", async () => {
  nowFails = {
    code: "internal",
    message: "pg_dump: error: could not write to output file: No space left on device",
    source_id: null,
  };
  render();
  await settle();

  button("Export now")!.click();
  await settle();

  expect(toasts.items.map((toast) => toast.text).join(" ")).toContain("No space left on device");
});

/**
 * One press, one archive.
 *
 * An export of a real corpus takes long enough to look like nothing happened,
 * and the reader's response to a button that looks inert is to press it again.
 * Two concurrent `pg_dump`s writing two archives a second apart is not a
 * crash — it is a retention window quietly one night shorter.
 */
test("the export button is not offered twice while one is running", async () => {
  render();
  await settle();

  button("Export now")!.click();
  // No `settle`: this is the state a reader is looking at *during* the round
  // trip, which is the whole window in which a second press is possible.
  flushSync();
  expect(button("Export now"), "a second press would write a second archive").toBeUndefined();
  expect(target.querySelector("button[disabled]")).not.toBeNull();

  await settle();
  expect(calls.now).toBe(1);
  expect(button("Export now"), "and it comes back when the export is done").toBeTruthy();
});

/** A number field in the schedule dialog, by the label that names it. */
function field(label: string) {
  const dlg = dialog()!;
  const labelled = [...dlg.querySelectorAll("label")].find((l) =>
    l.textContent?.trim().startsWith(label),
  )!;
  const id = labelled.getAttribute("for")!;
  return dlg.querySelector<HTMLInputElement>(`#${id}`)!;
}

function type(input: HTMLInputElement, value: string) {
  input.value = value;
  input.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
}

test("Schedule… opens a dialog carrying the schedule that is in force", async () => {
  status = statusOf({ schedule: schedule({ hour: 22, minute: 30, keep: 3 }) });
  render();
  await settle();

  button("Schedule…")!.click();
  flushSync();

  expect(field("Hour").value).toBe("22");
  expect(field("Minute").value).toBe("30");
  expect(field("Keep").value).toBe("3");
  expect(dialog()!.querySelector<HTMLInputElement>('input[type="checkbox"]')!.checked).toBe(true);
  // Nothing is posted by opening it.
  expect(calls.setSchedule).toEqual([]);
});

/**
 * The section redraws from what `set_backup_schedule` *answered*, not from
 * what was typed into the dialog: the command returns the whole status from
 * the same read that stored the change, which is the only version of the
 * schedule that is known to be on disk.
 */
test("saving posts the edited schedule and redraws the sentence from the answer", async () => {
  render();
  await settle();
  expect(text()).toContain("after 03:00");

  button("Schedule…")!.click();
  flushSync();
  type(field("Hour"), "22");
  type(field("Minute"), "30");
  type(field("Keep"), "3");
  button("Save", dialog()!)!.click();
  await settle();

  expect(calls.setSchedule).toEqual([{ enabled: true, hour: 22, minute: 30, keep: 3 }]);
  expect(dialog()).toBeNull();
  expect(text()).toContain("after 22:30");
  expect(text()).not.toContain("after 03:00");
  expect(text()).toContain("3 archives");
});

/**
 * The test that can actually tell the two apart.
 *
 * The one above cannot: its `set_backup_schedule` echoes, so a section that
 * redrew from the draft would render the same text. Here the command stores a
 * minute that is not the one posted — the shape `BackupSchedule::clamped`
 * gives it on the Rust side — and the section has to say what is stored.
 */
test("the sentence is redrawn from the schedule that was stored, not the one typed", async () => {
  stores = (posted) => ({ ...posted, minute: 45 });
  render();
  await settle();

  button("Schedule…")!.click();
  flushSync();
  type(field("Hour"), "22");
  type(field("Minute"), "30");
  button("Save", dialog()!)!.click();
  await settle();

  expect(calls.setSchedule).toEqual([{ enabled: true, hour: 22, minute: 30, keep: 7 }]);
  expect(text()).toContain("after 22:45");
  expect(text()).not.toContain("after 22:30");
});

test("cancelling the schedule dialog posts nothing and changes nothing", async () => {
  render();
  await settle();

  button("Schedule…")!.click();
  flushSync();
  type(field("Hour"), "22");
  button("Cancel", dialog()!)!.click();
  await settle();

  expect(calls.setSchedule).toEqual([]);
  expect(text()).toContain("after 03:00");
});

/**
 * Switching the nightly run off is the moment a reader most needs telling that
 * *Export now* is untouched by it — `backup::export_now` never consults the
 * schedule, and a section that only said "off" would read as "backups are off".
 */
test("turning the nightly export off says so, and says the manual export still works", async () => {
  render();
  await settle();

  button("Schedule…")!.click();
  flushSync();
  dialog()!.querySelector<HTMLInputElement>('input[type="checkbox"]')!.click();
  flushSync();
  button("Save", dialog()!)!.click();
  await settle();

  expect(calls.setSchedule).toEqual([{ enabled: false, hour: 3, minute: 0, keep: 7 }]);
  expect(text()).toContain("The nightly export is off");
  expect(text()).toContain("Export now still takes one");
  // And the button it is talking about is still there.
  expect(button("Export now")).toBeTruthy();
});

/**
 * An emptied number field reads as `NaN`, and `JSON.stringify(NaN)` is `null`
 * — which the Rust `u32` refuses to decode, so the schedule would come back as
 * an `invalid` rejection rather than as a saved change. The field is a place a
 * person clears before typing, so this is the ordinary path, not an edge.
 */
test("an emptied field is never posted as a broken schedule", async () => {
  render();
  await settle();

  button("Schedule…")!.click();
  flushSync();
  type(field("Hour"), "");
  type(field("Keep"), "");
  button("Save", dialog()!)!.click();
  await settle();

  expect(calls.setSchedule).toHaveLength(1);
  const posted = calls.setSchedule[0]!;
  expect(Number.isInteger(posted.hour)).toBe(true);
  expect(Number.isInteger(posted.keep)).toBe(true);
  // `keep: 0` would delete the archive the next export just wrote.
  expect(posted.keep).toBeGreaterThanOrEqual(1);
});

function archiveRow(file: string) {
  return target.querySelector<HTMLElement>(`.arc[data-file="${file}"]`);
}

test("every archive on disk is listed with its size", async () => {
  render();
  await settle();

  expect([...target.querySelectorAll(".arc[data-file]")].length).toBe(2);
  expect(archiveRow("knobas-20260828-030000.knobas")!.textContent).toContain("3.9 MB");
});

test("no archives yet says so rather than showing an empty list", async () => {
  status = statusOf({ last: null, archives: [] });
  render();
  await settle();

  expect(text()).toMatch(/no archives|nothing to restore/i);
  expect(target.querySelectorAll(".arc[data-file]").length).toBe(0);
});

/**
 * Restore asks first, and the question names the archive.
 *
 * The second archive, not the first: a fixture with one row cannot tell a
 * component that restores *the one you clicked* from one that restores the
 * newest.
 */
test("Restore asks first, and confirming restores the archive that was clicked", async () => {
  render();
  await settle();

  button("Restore", archiveRow("knobas-20260828-030000.knobas")!)!.click();
  flushSync();
  expect(calls.restore, "asking is the point of asking").toEqual([]);
  expect(dialog()!.textContent).toContain("knobas-20260828-030000.knobas");

  button("Restore", dialog()!)!.click();
  await settle();
  expect(calls.restore).toEqual(["knobas-20260828-030000.knobas"]);
});

/**
 * What the confirm has to say, in the concrete.
 *
 * Restore is the one control here that changes the database, and the engine's
 * ratified shape is narrow in a way the reader cannot guess: it restores into
 * a knobas that holds **no data yet** and refuses over a populated one, rather
 * than merging or overwriting (merge-with-a-preview is M4). A confirm that
 * only asked "are you sure?" would leave a person expecting either an
 * overwrite or a merge, and both expectations are wrong.
 */
test("the restore confirm says what the restore does and what it will not do", async () => {
  render();
  await settle();
  button("Restore", archiveRow("knobas-20260829-030000.knobas")!)!.click();
  flushSync();

  const said = dialog()!.textContent ?? "";
  // It only lands in an empty knobas...
  expect(said).toMatch(/holds no|empty|nothing yet/i);
  // ...and says what happens instead of an overwrite, so nobody expects one.
  expect(said).toMatch(/refus|declin|will not/i);
  // The mirror is not in the archive, so sources re-sync afterwards.
  expect(said).toMatch(/re-sync|resync/i);
  // ...and the re-sync is not promised to bring the whole mirror back. The
  // archive carries `knobas.source_config.cursor` and `knobas_db::backup::
  // restore` clears only `knobas.setting`, so a restored source runs
  // `run_from_stored_cursor` from the position the archive recorded — it
  // fetches what changed upstream since, not everything it once held.
  expect(said).toMatch(/does not rebuild/i);
  expect(said).toMatch(/resume from the position/i);
});

test("cancelling the restore confirm restores nothing", async () => {
  render();
  await settle();
  button("Restore", archiveRow("knobas-20260829-030000.knobas")!)!.click();
  flushSync();
  button("Cancel", dialog()!)!.click();
  await settle();

  expect(calls.restore).toEqual([]);
  expect(dialog()).toBeNull();
});

/**
 * The refusal a person can act on carries the reason from the database itself
 * — which table and how many rows. "Restore failed" would leave them with no
 * way to tell a populated knobas from a broken `pg_restore`.
 */
test("a refused restore says why, in the database's own words", async () => {
  restoreFails = {
    code: "conflict",
    message:
      "the database already holds knobas data (14 row(s) in knobas.entity); " +
      "restoring over it would merge, which knobas cannot do yet",
    source_id: null,
  };
  render();
  await settle();
  button("Restore", archiveRow("knobas-20260829-030000.knobas")!)!.click();
  flushSync();
  button("Restore", dialog()!)!.click();
  await settle();

  const said = toasts.items.map((toast) => toast.text).join(" ");
  expect(said).toContain("14 row(s) in knobas.entity");
  expect(calls.restore).toEqual(["knobas-20260829-030000.knobas"]);
});

/**
 * A restore replaces what every open view is drawing, and nothing in the app
 * re-reads itself when it happens: the health store, the room tabs and the
 * launcher's corpus are all still the old database's. Saying "restored" and
 * leaving the window on stale data is how a person concludes the restore did
 * not work.
 */
test("a completed restore says the window has to be restarted to see it", async () => {
  render();
  await settle();
  button("Restore", archiveRow("knobas-20260829-030000.knobas")!)!.click();
  flushSync();
  button("Restore", dialog()!)!.click();
  await settle();

  expect(toasts.items.map((toast) => toast.text).join(" ")).toMatch(/restart/i);
});

/**
 * A failed read is not an empty disk.
 *
 * `backup_status` rejects with `not_ready` for the whole of bring-up, and
 * "no archives on disk yet" in that window is a claim the section has not
 * earned — the reader would go looking for backups that are, in fact, there.
 */
test("backup_status failing renders what failed, not an empty backup story", async () => {
  statusFails = { code: "not_ready", message: "the database is still starting", source_id: null };
  render();
  await settle();

  expect(text()).toContain("the database is still starting");
  expect(text()).not.toMatch(/no archives|No backup has been taken/i);
  expect(button("Export now"), "there is nothing to export from yet").toBeUndefined();

  statusFails = null;
  button("Retry")!.click();
  await settle();
  expect(text()).toContain("after 03:00");
  expect(button("Export now")).toBeTruthy();
});

/**
 * Two reads in flight answer in whatever order they like, and the last answer
 * to land used to win.
 *
 * Reaching it here takes a second *Retry* over a failing `backup_status` —
 * genuinely rarer than the sources view's *Sync all*, which puts one read per
 * source in flight at once (#83, #99). It is the same hazard nonetheless, and
 * the settings shell is built to grow sections, so both call sites go through
 * `latestRead` rather than through two hand-rolled counters (#107).
 */
test("a backup_status overtaken by a later one does not write what it read", async () => {
  const stale = () => statusOf({ last: null });
  const fresh = () => statusOf();

  statusFails = { code: "not_ready", message: "the database is still starting", source_id: null };
  render();
  await settle();
  expect(button("Retry"), "the failed read is what puts a second one within reach").toBeTruthy();

  // The first *Retry*'s read is held open; the second one answers straight away.
  let release: (() => void) | undefined;
  const held = calls.status + 1;
  answerStatus = (call) =>
    call === held
      ? new Promise<BackupStatus>((resolve) => {
          release = () => resolve(stale());
        })
      : Promise.resolve(fresh());

  button("Retry")!.click();
  await settle();
  button("Retry")!.click();
  await settle();
  expect(calls.status, "both retries read the status").toBe(held + 1);
  expect(text()).toContain("Last export");

  release!();
  await settle();

  expect(text(), "the overtaken read wrote its stale snapshot over the newer one").toContain(
    "Last export",
  );
  expect(text()).not.toContain("No backup has been taken yet");
});

/**
 * The half most easily dropped when the guard is copied by hand: a *stale
 * rejection* must not blank a status that has since read fine.
 *
 * Without it the section lands on "Could not read the backup settings" while
 * holding a perfectly good status, and the message it shows is the one from
 * the read that was already out of date.
 */
test("a backup_status rejection that has been overtaken does not blank the section", async () => {
  statusFails = { code: "not_ready", message: "the database is still starting", source_id: null };
  render();
  await settle();

  let reject: (() => void) | undefined;
  const held = calls.status + 1;
  answerStatus = (call) =>
    call === held
      ? new Promise<BackupStatus>((_resolve, fail) => {
          reject = () => fail({ code: "internal", message: "the stale failure", source_id: null });
        })
      : Promise.resolve(statusOf());

  button("Retry")!.click();
  await settle();
  button("Retry")!.click();
  await settle();
  expect(text()).toContain("after 03:00");

  reject!();
  await settle();

  expect(text(), "a stale rejection blanked a status that read fine").toContain("after 03:00");
  expect(text()).not.toContain("Could not read the backup settings");
  expect(text()).not.toContain("the stale failure");
  expect(button("Retry")).toBeUndefined();
});

/**
 * A save is a read too, and it has to be stamped like one.
 *
 * `set_backup_schedule` answers with the whole status, read back after the
 * write — so the section redraws from it rather than from the draft. Written
 * straight to `status`, that write is outside the guard: a `backup_status`
 * already in flight when the save started is *older* than the save's answer
 * and used to land on top of it, putting the schedule that was just replaced
 * back on the screen while the disk holds the new one. The same currency rule
 * this component already applies to its reads, applied to the one write that
 * was left out of it (#129).
 *
 * *Export now* is what puts the read in flight; it is the section's own way of
 * having a `backup_status` outstanding while the reader does something else.
 */
test("a backup_status in flight across a schedule save does not overwrite what was saved", async () => {
  render();
  await settle();
  expect(text()).toContain("after 03:00");

  // Snapshotted eagerly: the `set_backup_schedule` mock rewrites the shared
  // `status`, and a held read that built its answer at resolution time would
  // hand back the *new* schedule and agree with the save by accident.
  const before = statusOf();
  let release: (() => void) | undefined;
  const held = calls.status + 1;
  answerStatus = (call) =>
    call === held
      ? new Promise<BackupStatus>((resolve) => {
          release = () => resolve(before);
        })
      : Promise.resolve(status);

  button("Export now")!.click();
  await settle();
  expect(calls.status, "the export's re-read is the one held open").toBe(held);

  button("Schedule…")!.click();
  flushSync();
  type(field("Hour"), "22");
  type(field("Minute"), "30");
  button("Save", dialog()!)!.click();
  await settle();
  expect(text()).toContain("after 22:30");

  release!();
  await settle();

  expect(
    text(),
    "a backup_status from before the save wrote the old schedule back over it",
  ).toContain("after 22:30");
  expect(text()).not.toContain("after 03:00");
});

/**
 * Closing the schedule dialog must not throw into a promise nobody is holding.
 *
 * Both exits are walked, because they are different code paths onto the same
 * hazard: *Cancel* drops the draft, and *Save* drops it after a round trip.
 */
test("closing the schedule dialog raises nothing, whichever way it is closed", async () => {
  render();
  await settle();

  button("Schedule…")!.click();
  flushSync();
  type(field("Hour"), "22");
  button("Cancel", dialog()!)!.click();
  await settle();
  await settleRejections();
  expect(takeUnhandled(), "cancelling raised").toEqual([]);

  button("Schedule…")!.click();
  flushSync();
  type(field("Keep"), "3");
  button("Save", dialog()!)!.click();
  await settle();
  await settleRejections();
  expect(takeUnhandled(), "saving raised").toEqual([]);

  // And the restore confirm, which is the other dialog over a nullable value.
  button("Restore", archiveRow("knobas-20260829-030000.knobas")!)!.click();
  flushSync();
  button("Cancel", dialog()!)!.click();
  await settle();
  await settleRejections();
  expect(takeUnhandled(), "closing the restore confirm raised").toEqual([]);
});
