import { expect, test } from "vitest";

import type { ConnectionReport } from "../ipc/sources";
import { connectionLine, connectionNote, failedReport } from "./connection";

function report(over: Partial<ConnectionReport> = {}): ConnectionReport {
  return {
    ok: true,
    account: "mara.oyelaran",
    server_version: "9.12.4",
    secret_expires_at: null,
    error: null,
    code: null,
    elapsed_ms: 214,
    detail: null,
    discovered: {},
    ...over,
  };
}

test("a successful line names the account, the version and the time, in that order", () => {
  expect(connectionLine(report())).toBe("Connected as mara.oyelaran · 9.12.4 · 214 ms");
});

test("an absent reading is simply not written, with no separator left behind", () => {
  expect(connectionLine(report({ account: null, server_version: null, elapsed_ms: 88 }))).toBe(
    "Connected · 88 ms",
  );
});

test("a failed test's line is its error, or a plain sentence when it has none", () => {
  expect(connectionLine(report({ ok: false, error: "401 from /rest/api/2/myself" }))).toBe(
    "401 from /rest/api/2/myself",
  );
  expect(connectionLine(report({ ok: false, error: null }))).toBe("The connection failed.");
});

test("the note is shown only for a test that connected and had one", () => {
  expect(connectionNote(report({ detail: "Epic Link customfield_10101" }))).toBe(
    "Epic Link customfield_10101",
  );
  expect(connectionNote(report({ detail: null }))).toBeNull();
  expect(connectionNote(report({ detail: "" }))).toBeNull();
  expect(connectionNote(report({ ok: false, error: "refused", detail: "stale" }))).toBeNull();
});

test("a rejection becomes a failed report carrying the message and the code", () => {
  const rejected = failedReport({ code: "unauthorized", message: "refused", source_id: "jira" });
  expect(rejected.ok).toBe(false);
  expect(connectionLine(rejected)).toBe("refused");
  expect(rejected.code).toBe("unauthorized");
  expect(connectionNote(rejected)).toBeNull();
  // Not an IpcError: the message still reads, and there is no code to branch on.
  const plain = failedReport(new Error("bridge missing"));
  expect(connectionLine(plain)).toBe("bridge missing");
  expect(plain.code).toBeNull();
});
