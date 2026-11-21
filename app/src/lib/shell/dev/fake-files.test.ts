/**
 * The browser's fake reads the fixture repositories' files (the everything-search decisions, D16; the small-fixes sweep,
 * 2026-11-21): `fixtures/tidewater/files/<owner>/<name>/…`, the files the mock source compiles in, not a table of its own
 * that drifts from them.
 */
import { readdirSync } from "node:fs";
import { join, resolve } from "node:path";
import { expect, test } from "vitest";

import { FAKE_FILE_REPOS, fakeFilesFound } from "./fake/launcher";

const ROOT = resolve(__dirname, "../../../../../fixtures/tidewater/files");

function walk(dir: string, at = ""): string[] {
  return readdirSync(join(dir, at), { withFileTypes: true }).flatMap((entry) => {
    const path = at === "" ? entry.name : `${at}/${entry.name}`;
    return entry.isDirectory() ? walk(dir, path) : [path];
  });
}

test("the fake's repositories hold the fixture's files, each under its repository", () => {
  const onDisk = walk(ROOT).sort();
  const inFake = FAKE_FILE_REPOS.flatMap((repo) => repo.files.map(([path]) => `${repo.title}/${path}`)).sort();
  expect(inFake).toEqual(onDisk);
  expect(FAKE_FILE_REPOS.map((repo) => repo.entity_id).sort()).toEqual([
    "mock:tidewater/ledger-api",
    "mock:tidewater/ledger-gateway",
    "mock:tidewater/payout-service",
  ]);
});

test("a file the engine's rules leave out is found by its name only, and counted by its rule", () => {
  const payout = FAKE_FILE_REPOS.find((repo) => repo.entity_id === "mock:tidewater/payout-service")!;
  expect(payout.left_out).toEqual({ lock_file: 1, folder: 1, binary: 1, generated: 1, long_line: 1 });
  // `RetryQueue` by five files' names and text.
  const retry = fakeFilesFound(["retryqueue"]).find((one) => one.repo === payout);
  expect(retry).toMatchObject({ byName: 2 });
  expect(retry!.byText).toBeGreaterThan(0);
  // Only the generated file's head names its generator, and its text is not kept.
  expect(fakeFilesFound(["pspgen"])).toEqual([]);
  // The renamed repository by its classes' names, and the client that only imports it by its text.
  const settle = fakeFilesFound(["settlebridge"]);
  expect(settle.map((one) => [one.repo.entity_id, one.byName > 0, one.byText > 0]).sort()).toEqual([
    ["mock:tidewater/ledger-api", false, true],
    ["mock:tidewater/ledger-gateway", true, true],
  ]);
});
