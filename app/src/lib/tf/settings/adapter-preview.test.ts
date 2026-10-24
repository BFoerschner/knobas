import { expect, test } from "vitest";

import type { AdapterPreview } from "../../ipc/sources";
import { limitsLine, reach, rolesLine, updateChanges } from "./adapter-preview";

function release(over: Partial<AdapterPreview> = {}): AdapterPreview {
  return {
    adapter_kind: "wasm-fixture",
    name: "Harbour surveys",
    adapter_version: "0.1.0",
    interface_version: "1.0.0",
    source_url: "https://github.test/harbour/surveys",
    tag: "v0.1.0",
    artifact_sha256: "5".repeat(64),
    confirm_sha256: "7".repeat(64),
    hosts: ["{base_url}", "api.example.com"],
    roles: [{ role: "ci", reads: ["run_log"] }],
    limits: { deadline_secs: 600, deadline_asked: false, memory_mib: 256, memory_asked: false },
    ...over,
  };
}

test("a release that asks for what is installed changes nothing", () => {
  expect(updateChanges(release(), release({ tag: "v0.2.0", artifact_sha256: "6".repeat(64), confirm_sha256: "8".repeat(64) }))).toEqual([]);
});

test("an update that narrows says what it no longer reaches or claims, and a deadline that moves", () => {
  const next = release({
    hosts: ["{base_url}", "{mirror}"],
    roles: [{ role: "wasm-fixture:survey-desk", reads: [] }],
    limits: { deadline_secs: 30, deadline_asked: true, memory_mib: 256, memory_asked: false },
  });
  expect(updateChanges(release(), next)).toEqual([
    "It may newly reach the host in its mirror setting.",
    "It no longer reaches api.example.com.",
    "It newly claims wasm-fixture:survey-desk.",
    "It no longer claims CI.",
    "Each call may take 30 s, was 10 m 00 s.",
  ]);
});

test("a claim whose reads change is the same role", () => {
  expect(updateChanges(release(), release({ roles: [{ role: "ci", reads: ["run_log", "run_tests"] }] }))).toEqual([]);
});

test("the limits say which the manifest asks for and which are the default", () => {
  expect(limitsLine(release().limits)).toBe("10 m 00 s per call (the default), 256 MiB of memory (the default)");
  expect(limitsLine({ deadline_secs: 600, deadline_asked: false, memory_mib: 512, memory_asked: true })).toBe(
    "10 m 00 s per call (the default), 512 MiB of memory (asked for)",
  );
});

test("an adapter with no host and no role says so", () => {
  expect(reach([])).toBe("It may reach no host.");
  expect(rolesLine([])).toBe("None");
});
