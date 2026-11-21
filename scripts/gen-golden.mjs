#!/usr/bin/env node
// Writes app/src/lib/render/golden/<engine>/<fixture>.json: each highlighter's answer for the fixtures beside them (the
// highlighter re-ruling, 2026-11-21, test 4). Run after a highlighter's version bump, in that bump's own pull request,
// and read the diff: every changed line is a token that changed its role.
//
//   node scripts/gen-golden.mjs
import { spawnSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const app = join(dirname(fileURLToPath(import.meta.url)), "..", "app");
const run = spawnSync("npx", ["vitest", "run", "src/lib/render/golden.test.ts"], { cwd: app, stdio: "inherit", env: { ...process.env, UPDATE_GOLDEN: "1" } });
process.exit(run.status ?? 1);
