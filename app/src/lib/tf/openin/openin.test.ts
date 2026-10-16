import { expect, test } from "vitest";

import type { CheckoutSettings } from "../../ipc/entity";
import { argvText, cloneFailure, cloneTip, commandFor, fill, insertAt, menuOrder, warnings } from "./openin";

const AT = { path: "/Users/mara/code/payout-service", file: "/Users/mara/code/payout-service/src/lib.rs", line: 42 };

test("the placeholders are drawn from the checkout, its file and line, and {name} is the folder", () => {
  expect(fill("code --goto {file}:{line}", AT)).toBe("code --goto /Users/mara/code/payout-service/src/lib.rs:42");
  expect(fill("viewer {name} {path}", AT)).toBe("viewer payout-service /Users/mara/code/payout-service");
  // Outside a file, {file} is the checkout and {line} is 1, as the warning says.
  expect(fill("idea --line {line} {file}", { ...AT, file: null, line: null })).toBe(
    "idea --line 1 /Users/mara/code/payout-service",
  );
});

test("a started argv reads as one line, an argument with a space quoted", () => {
  expect(argvText(["open", "-a", "Visual Studio Code", "/Users/mara/My Code/x"])).toBe(
    "open -a 'Visual Studio Code' '/Users/mara/My Code/x'",
  );
});

test("the menu puts the default first and keeps the rest in order", () => {
  const settings: CheckoutSettings = {
    clones_root: null,
    git: { status: "no_git" },
    default_application: "fork",
    applications: [
      { id: "vscode", name: "VS Code", command: "code {path}", file_command: "code --goto {file}:{line}" },
      { id: "terminal", name: "Terminal", command: "open -a Terminal {path}", file_command: null },
      { id: "fork", name: "Fork", command: "open -a Fork {path}", file_command: null },
    ],
  };
  expect(menuOrder(settings).map((row) => [row.app.id, row.isDefault])).toEqual([
    ["fork", true],
    ["vscode", false],
    ["terminal", false],
  ]);
  // At a file the file command is used, where there is one.
  expect(commandFor(settings.applications[0]!, "src/lib.rs")).toBe("code --goto {file}:{line}");
  expect(commandFor(settings.applications[1]!, "src/lib.rs")).toBe("open -a Terminal {path}");
  expect(commandFor(settings.applications[0]!, null)).toBe("code {path}");
});

test("the dialog warns of a missing {path}, file placeholders in the command, and shell characters", () => {
  expect(warnings("")).toEqual([]);
  expect(warnings("code {path}")).toEqual([]);
  expect(warnings("viewer {name}")).toEqual([]);
  const all = warnings("code --goto {file} | tee x");
  expect(all).toHaveLength(3);
  expect(all[0]).toContain("has no {path}");
  expect(all[1]).toContain("belong in “For a file”");
  expect(all[2]).toContain("passed through as plain text");
  expect(warnings("a {path}; b")).toHaveLength(1);
});

test("a placeholder goes in at the cursor, after a space", () => {
  expect(insertAt("code", null, "{path}")).toEqual({ value: "code {path}", cursor: 11 });
  expect(insertAt("code  --x", 5, "{path}")).toEqual({ value: "code {path} --x", cursor: 11 });
  expect(insertAt("", 0, "{file}")).toEqual({ value: "{file}", cursor: 6 });
});

test("Clone's tooltip names the folder, and a failure says what to do", () => {
  expect(cloneTip("~/code", "booking-web")).toBe("Clones into ~/code/booking-web");
  expect(cloneTip(null, "booking-web")).toBe("Clones into your clones root/booking-web");
  expect(cloneFailure("auth", "fatal: Authentication failed", "https://git.tidewater.internal/t/booking-web")).toBe(
    "git could not sign in to git.tidewater.internal. knobas uses your own git credentials (ssh agent or credential helper); check them in a terminal",
  );
  expect(cloneFailure("other", "fatal: unable to access: Could not resolve host", null)).toBe(
    "fatal: unable to access: Could not resolve host",
  );
  expect(cloneFailure("disk_full", "No space left on device", null)).toContain("The disk is full");
});
