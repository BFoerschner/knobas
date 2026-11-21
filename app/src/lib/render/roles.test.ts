/** The roles a token carries, and how each engine's own names become them (ADR-0051). */
import { expect, test } from "vitest";

import { DATA_LANGS, PIERRE_ROLE, pierreRole, ROLE_NAMES, ROLES, roleIn } from "./roles";

test("there are seventeen roles, each with the name a person reads", () => {
  expect(ROLES).toHaveLength(17);
  expect(Object.keys(ROLE_NAMES).sort()).toEqual([...ROLES].sort());
  // The six of before keep their names, so nothing drawn is renamed.
  for (const old of ["kw", "str", "num", "com", "fn", "ty"]) expect(ROLES).toContain(old);
});

test("every Pierre type in the table gives its role", () => {
  for (const [type, role] of PIERRE_ROLE) expect(pierreRole(type)).toBe(role);
  expect(pierreRole("keyword-import")).toBe("kw");
  expect(pierreRole("type-class")).toBe("ty");
  expect(pierreRole("constructor")).toBe("ty");
  expect(pierreRole("attribute")).toBe("attr");
  expect(pierreRole("preproc")).toBe("attr");
  expect(pierreRole("string-escape")).toBe("esc");
  expect(pierreRole("punctuation-special")).toBe("esc");
  expect(pierreRole("boolean")).toBe("con");
  expect(pierreRole("title")).toBe("head");
  expect(pierreRole("emphasis-strong")).toBe("strong");
});

test("a JSON key is a key, not a property", () => {
  expect(pierreRole("property-json-key")).toBe("key");
  expect(pierreRole("property")).toBe("prop");
});

test("a punctuation type is punctuation unless it is the special one; a type not in the table is plain", () => {
  expect(pierreRole("punctuation-bracket")).toBe("punct");
  expect(pierreRole("punctuation-something-new")).toBe("punct");
  for (const plain of ["variable", "namespace", "embedded", "primary", "foreground", "diff-plus", "diff-minus", "made-up", "toString"]) expect(pierreRole(plain)).toBe("");
});

test("a data language folds properties, tags and attributes into keys, and leaves code alone", () => {
  for (const lang of ["json", "jsonc", "yaml", "properties", "ini", "toml"]) {
    expect(DATA_LANGS.has(lang)).toBe(true);
    expect(roleIn(lang, "prop")).toBe("key");
    expect(roleIn(lang, "tag")).toBe("key");
    expect(roleIn(lang, "attr")).toBe("key");
    expect(roleIn(lang, "str")).toBe("str");
  }
  expect(roleIn("yaml", "tag")).toBe("key");
  expect(roleIn("typescript", "prop")).toBe("prop");
  expect(roleIn("xml", "tag")).toBe("tag");
  expect(roleIn("java", "attr")).toBe("attr");
});
