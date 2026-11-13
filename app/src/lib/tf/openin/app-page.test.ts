import { expect, test } from "vitest";

import type { CheckoutApplication } from "../../ipc/entity";
import { escTarget } from "../escape";
import { parentOf, parseTfHash, sectionTabOf } from "../route";
import { settingsKnows } from "../settings/settings";
import { appPageHash, appPagePlace, draftFrom, entryOf, tryObject, withEntry, withoutTry } from "./app-page";

const rest = (hash: string) => {
  const route = parseTfHash(hash);
  return route.view === "full" ? route.rest : [];
};

test("the page's address says which application, and the repository and object it came from, and reads back the same", () => {
  const cases = [
    [{}, "#/settings/checkouts/add"],
    [{ app: "vscode" }, "#/settings/checkouts/app/vscode"],
    [{ repo: "gitea:tidewater/payout-service" }, "#/settings/checkouts/add/gitea%3Atidewater%2Fpayout-service"],
    [
      { repo: "gitea:tidewater/payout-service", kind: "file", value: "src/sepa/retry.rs" },
      "#/settings/checkouts/add/gitea%3Atidewater%2Fpayout-service/file/src%2Fsepa%2Fretry.rs",
    ],
    [{ app: "tig", repo: "gitea:t/x", kind: "commit", value: "c90d11e4a7b2" }, "#/settings/checkouts/app/tig/gitea%3At%2Fx/commit/c90d11e4a7b2"],
  ] as const;
  for (const [at, hash] of cases) {
    expect(appPageHash(at)).toBe(hash);
    expect(appPagePlace(rest(hash))).toEqual({ app: null, repo: null, kind: null, value: null, ...at });
  }
  // Checkouts itself, and a word it doesn't know, are not the page.
  expect(appPagePlace(["checkouts"])).toBeNull();
  expect(appPagePlace(["checkouts", "nope"])).toBeNull();
  expect(appPagePlace(["sources"])).toBeNull();
});

test("Settings knows the page, Esc goes up to Checkouts from it, and the cog doesn't come back to it", () => {
  expect(settingsKnows(["checkouts", "add"])).toBe(true);
  expect(settingsKnows(["checkouts", "app", "vscode"])).toBe(true);
  expect(settingsKnows(["checkouts", "nope"])).toBe(false);
  const route = parseTfHash("#/settings/checkouts/add/gitea%3At%2Fx");
  expect(parentOf(route)).toBe("#/settings/checkouts");
  expect(sectionTabOf(route)).toBeNull();
  expect(sectionTabOf(parseTfHash("#/settings/checkouts"))).toBe("settings");
  // Opened from a repository's Open in…, Esc and Back go there.
  expect(escTarget(route, { hash: "#/repo/gitea%3At%2Fx", name: "t/x" })).toEqual({ hash: "#/repo/gitea%3At%2Fx", name: "t/x", opened: true });
});

const TERMINAL: CheckoutApplication = { id: "terminal", name: "Terminal", command: "wt -d {path}", file_command: null };
const VSCODE: CheckoutApplication = { id: "vscode", name: "VS Code", command: "code.cmd {path}", file_command: null };

test("the entry goes in its own place, or at the end when new; a try is taken back by its id alone", () => {
  const edited = entryOf({ ...draftFrom(TERMINAL), command: "wt -d {path} -p knobas" }, "terminal");
  expect(withEntry([TERMINAL, VSCODE], edited).map((app) => app.command)).toEqual(["wt -d {path} -p knobas", "code.cmd {path}"]);
  const added = entryOf({ ...draftFrom(null), name: " Tig ", command: " tig -C {path} ", commit: "tig show {sha}" }, "");
  expect(added).toEqual({ id: "", name: "Tig", command: "tig -C {path}", file_command: null, commit_command: "tig show {sha}" });
  expect(withEntry([TERMINAL], added).map((app) => app.id)).toEqual(["terminal", ""]);

  // A new one tried is gone again; an edited one is as it was. Nothing else moves.
  const tried = { ...added, id: "tig" };
  expect(withoutTry([VSCODE, TERMINAL, tried], "tig", null)).toEqual([VSCODE, TERMINAL]);
  expect(withoutTry([VSCODE, edited], "terminal", TERMINAL)).toEqual([VSCODE, TERMINAL]);
});

test("a range is the picked commit and the one before it; a pull request needs its branches", () => {
  const picks = { file: null, at: null, commits: ["c3", "c2", "c1"], commit: "c2", pr: null };
  expect(tryObject("range", picks)).toEqual({ kind: "range", base: "c1", head: "c2" });
  expect(tryObject("range", { ...picks, commit: "c1" })).toBeNull();
  expect(tryObject("pull_request", picks)).toBeNull();
  expect(tryObject("file", picks)).toBeNull();
  expect(tryObject("repository", picks)).toEqual({ kind: "repository" });
});
