import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const ext = join(dirname(fileURLToPath(import.meta.url)), "..");

function build(out) {
  const printed = execFileSync("python3", ["-I", join(ext, "build.py"), "--out", out], { encoding: "utf8" });
  return printed.split("\n")[0];
}

test("the .xpi holds what Firefox loads, manifest at its root, and builds byte for byte the same", () => {
  const a = mkdtempSync(join(tmpdir(), "knobas-xpi-a-"));
  const b = mkdtempSync(join(tmpdir(), "knobas-xpi-b-"));
  try {
    const first = build(a);
    const second = build(b);
    const version = JSON.parse(readFileSync(join(ext, "manifest.json"), "utf8")).version;
    assert.ok(first.endsWith(`knobas-link-handoff-${version}.xpi`), first);
    assert.ok(readFileSync(first).equals(readFileSync(second)), "two builds differ");
    const entries = execFileSync("unzip", ["-Z1", first], { encoding: "utf8" }).trim().split("\n");
    assert.deepEqual(entries, [
      "background.html",
      "icons/off.svg",
      "icons/on.svg",
      "manifest.json",
      "src/background.js",
      "src/native.js",
      "src/protocol.js",
      "src/router.js",
      "src/rules.js",
      "src/toolbar.js",
    ]);
  } finally {
    rmSync(a, { recursive: true, force: true });
    rmSync(b, { recursive: true, force: true });
  }
});
