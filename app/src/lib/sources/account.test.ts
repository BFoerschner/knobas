/**
 * The optional second credential's one rule (#452): **both halves or neither**.
 *
 * Extracted here because two forms offer an account — the Add-source dialog and
 * the re-enter strip — and a rule stated twice is a rule that can disagree with
 * itself. What it prevents is a username with no password reaching the
 * keychain: a credential that logs in to nothing, stored as though it were one,
 * and discovered when a pause is refused.
 */
import { expect, test } from "vitest";

import { ACCOUNT_INCOMPLETE, accountHalfDone, accountOf } from "./account";

test("both halves make an account and anything less makes none", () => {
  expect(accountOf({ username: "knobas", password: "knobas-dev" })).toEqual({
    username: "knobas",
    password: "knobas-dev",
  });
  // `null` and not `undefined`: the wire says *keep what is stored*, and an
  // absent key would say the same thing less clearly.
  expect(accountOf({ username: "", password: "" })).toBeNull();
  expect(accountOf({ username: "knobas", password: "" })).toBeNull();
  expect(accountOf({ username: "", password: "knobas-dev" })).toBeNull();
});

test("half filled in is a state a form can be in, and refuse", () => {
  expect(accountHalfDone({ username: "", password: "" })).toBe(false);
  expect(accountHalfDone({ username: "knobas", password: "knobas-dev" })).toBe(false);
  expect(accountHalfDone({ username: "knobas", password: "" })).toBe(true);
  expect(accountHalfDone({ username: "", password: "knobas-dev" })).toBe(true);
});

/** The two forms say the same sentence because they read the same constant. */
test("the incomplete message names both halves", () => {
  expect(ACCOUNT_INCOMPLETE).toContain("username");
  expect(ACCOUNT_INCOMPLETE).toContain("password");
});
