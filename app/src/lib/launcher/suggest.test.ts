/**
 * Which token of the box is being typed, and how picking a value rewrites it
 * (#601). The lists themselves, and what the launcher draws from them, are
 * pinned in `Launcher.test.svelte.ts`.
 */
import { expect, test } from "vitest";

import { activeToken, complete, suggest } from "./suggest";

const LISTS = {
  kinds: [{ kind: "repo", label: "Repository" }],
  sources: [{ id: "jira-eu", display_name: "Jira EU" }],
  parents: [{ name: "Runbooks Overview", kind: "page" }],
};

test("the grammar's other spellings of a key draw from the same list", () => {
  expect(activeToken("Type:")?.key).toBe("kind");
  expect(activeToken("in:ji")?.key).toBe("source");
  expect(activeToken("src:")?.key).toBe("source");
  expect(activeToken("UNDER:x")?.key).toBe("under");
});

test("only the last token is being typed, and a quoted run with a space in it is one token", () => {
  expect(activeToken("kind:repo sepa")).toBeNull();
  expect(activeToken("kind:repo ")).toBeNull();
  expect(activeToken(":repo")).toBeNull();
  const quoted = activeToken('sepa under:"Runbooks Ov')!;
  expect(quoted).toMatchObject({
    key: "under",
    start: 5,
    partial: "Runbooks Ov",
  });
  expect(suggest(quoted, LISTS).map((s) => s.value)).toEqual(["Runbooks Overview"]);
});

test("completing keeps the key as typed, quotes a name with a space, and ends the token", () => {
  const token = activeToken('sepa Under:"runb')!;
  expect(complete('sepa Under:"runb', token, "Runbooks Overview")).toBe(
    'sepa Under:"Runbooks Overview" ',
  );
  const kind = activeToken("type:re")!;
  expect(complete("type:re", kind, "repo")).toBe("type:repo ");
});

test("a value typed out in full closes the list, even when a longer value contains it", () => {
  const lists = {
    ...LISTS,
    parents: [
      { name: "vm-1", kind: "asset" },
      { name: "vm-10", kind: "asset" },
    ],
  };
  expect(suggest(activeToken("under:vm-")!, lists).map((s) => s.value)).toEqual([
    "vm-1",
    "vm-10",
  ]);
  expect(suggest(activeToken("under:VM-1")!, lists)).toEqual([]);
});
