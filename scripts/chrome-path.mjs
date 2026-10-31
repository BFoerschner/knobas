// The Chrome binary the headless scripts start (scripts/shot-steps.mjs,
// scripts/keep-alive-budget.mjs, scripts/shot-test.sh), found the same way for all three:
// SHOT_CHROME when set, else the macOS app bundle, else the first of google-chrome,
// google-chrome-stable, chromium and chromium-browser on PATH (Linux). Until this file
// they named the macOS path alone, so on Linux every shot failed to start Chrome and
// shot-test.sh skipped its Chrome cases on a host that had one.
//
// As a module: `chromePath()` returns the path, or null when none is found.
// As a command (`node scripts/chrome-path.mjs`): prints it and exits 0, or exits 1 silently.
import { accessSync, constants } from "node:fs";
import { delimiter, join } from "node:path";
import { fileURLToPath } from "node:url";

const MACOS = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const NAMES = ["google-chrome", "google-chrome-stable", "chromium", "chromium-browser"];

const runnable = (path) => {
  try {
    accessSync(path, constants.X_OK);
    return true;
  } catch {
    return false;
  }
};

export const chromePath = (env = process.env) => {
  if (env.SHOT_CHROME) return env.SHOT_CHROME;
  if (runnable(MACOS)) return MACOS;
  const dirs = (env.PATH || "").split(delimiter).filter(Boolean);
  for (const name of NAMES) {
    for (const dir of dirs) {
      const path = join(dir, name);
      if (runnable(path)) return path;
    }
  }
  return null;
};

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const path = chromePath();
  if (!path) process.exit(1);
  console.log(path);
}
