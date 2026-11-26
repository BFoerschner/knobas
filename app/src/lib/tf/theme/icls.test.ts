/**
 * A JetBrains colour scheme (`.icls`) onto the code roles (Björn, 2026-11-22: "as an option, … when there's time").
 * The fixture is Dracula's own JetBrains scheme at dracula/jetbrains 718eb13e (MIT), pruned to the attributes read.
 */
import { readFileSync } from "node:fs";
import { join } from "node:path";

import { expect, test, vi } from "vitest";

import { fromIcls, hasApp, hasCode, iclsColour, importThemes, REFUSED } from "./themes";

const DRACULA = readFileSync(join(process.cwd(), "src/lib/tf/theme/fixtures/dracula.icls"), "utf8");

test("Dracula's JetBrains scheme reads as a dark theme with a code part only", () => {
  const result = importThemes(DRACULA);
  if ("refused" in result) throw new Error(result.refused);
  expect(result.themes).toHaveLength(1);
  const theme = result.themes[0]!;
  expect(theme.name).toBe("Dracula");
  expect(theme.kind).toBe("dark");
  expect(hasCode(theme)).toBe(true);
  expect(hasApp(theme)).toBe(false);
  // Its instance field is an empty `<value />` (Darcula's own), so `prop` draws in ink.
  expect(theme.syntax).toEqual({
    com: "#6272A4",
    kw: "#FF79C6",
    str: "#F1FA8C",
    esc: "#50FA7B",
    num: "#BD93F9",
    con: "#BD93F9",
    fn: "#50FA7B",
    ty: "#8BE9FD",
    key: "#8BE9FD",
    tag: "#FF79C6",
    attr: "#8BE9FD",
    var: "#FFB86C",
    op: "#F8F8F2",
    punct: "#F8F8F2",
    head: "#BD93F9",
    link: "#BD93F9",
  });
});

test("a JetBrains colour is hex digits left-padded to six", () => {
  expect(iclsColour("cc7832")).toBe("#CC7832");
  expect(iclsColour("80")).toBe("#000080");
  expect(iclsColour("0")).toBe("#000000");
  expect(iclsColour(" ff ")).toBe("#0000FF");
  expect(iclsColour("1234567")).toBeNull();
  expect(iclsColour("zz")).toBeNull();
  expect(iclsColour("")).toBeNull();
  expect(iclsColour(undefined)).toBeNull();
});

const scheme = (attrs: string, head = '<scheme name="Mine" version="142" parent_scheme="Default">') => `${head}<attributes>${attrs}</attributes></scheme>`;
const fg = (name: string, value: string, extra = "") => `<option name="${name}"><value><option name="FOREGROUND" value="${value}" />${extra}</value></option>`;

test("the first attribute present wins, whatever order the file lists them in", () => {
  const t = fromIcls(scheme(fg("DEFAULT_FUNCTION_CALL", "00627a") + fg("DEFAULT_FUNCTION_DECLARATION", "0000ff") + fg("DEFAULT_DOC_COMMENT", "8c8c8c") + fg("DEFAULT_LINE_COMMENT", "808080")))!;
  expect(t.syntax).toEqual({ fn: "#0000FF", com: "#808080" });
  // A role no attribute names draws in ink.
  expect(fromIcls(scheme(fg("DEFAULT_KEYWORD", "0033b3")))!.syntax).toEqual({ kw: "#0033B3" });
});

test("an attribute with no colours of its own takes its base's, and one with its own keeps them", () => {
  const based = fromIcls(scheme('<option name="DEFAULT_KEYWORD" baseAttributes="DEFAULT_FUNCTION_CALL" />' + fg("DEFAULT_FUNCTION_CALL", "00627a")))!;
  expect(based.syntax.kw).toBe("#00627A");
  // An empty `<value />` is no colour of its own either.
  expect(fromIcls(scheme('<option name="DEFAULT_NUMBER" baseAttributes="DEFAULT_STRING"><value /></option>' + fg("DEFAULT_STRING", "067d17")))!.syntax.num).toBe("#067D17");
  const own = fromIcls(scheme('<option name="DEFAULT_KEYWORD" baseAttributes="DEFAULT_STRING"><value><option name="FOREGROUND" value="0033b3" /></value></option>' + fg("DEFAULT_STRING", "067d17")))!;
  expect(own.syntax.kw).toBe("#0033B3");
  // A base that names itself stops.
  expect(fromIcls(scheme('<option name="DEFAULT_KEYWORD" baseAttributes="DEFAULT_KEYWORD" />'))!.syntax).toEqual({});
});

test("its side: its page colour, else its parent, else its name", () => {
  const text = (v: string) => `<option name="TEXT"><value><option name="BACKGROUND" value="${v}" /></value></option>`;
  expect(fromIcls(scheme(text("ffffff"), '<scheme name="Night" parent_scheme="Darcula">'))!.kind).toBe("light");
  expect(fromIcls(scheme(text("2b2b2b"), '<scheme name="Day" parent_scheme="Default">'))!.kind).toBe("dark");
  expect(fromIcls(scheme("", '<scheme name="Paper Light" parent_scheme="Darcula">'))!.kind).toBe("dark");
  expect(fromIcls(scheme("", '<scheme name="Night" parent_scheme="Default">'))!.kind).toBe("light");
  expect(fromIcls(scheme("", '<scheme name="Paper Light">'))!.kind).toBe("light");
  expect(fromIcls(scheme("", '<scheme name="Paper">'))!.kind).toBe("dark");
});

test("a comment set upright says so; a slanted one or none leaves the default", () => {
  const style = (v: string) => scheme(fg("DEFAULT_LINE_COMMENT", "808080", `<option name="FONT_TYPE" value="${v}" />`));
  expect(fromIcls(style("0"))!.italicComments).toBe(false);
  expect(fromIcls(style("1"))!.italicComments).toBe(false);
  expect(fromIcls(style("2"))!.italicComments).toBeUndefined();
  expect(fromIcls(style("3"))!.italicComments).toBeUndefined();
  expect(fromIcls(scheme(fg("DEFAULT_LINE_COMMENT", "808080")))!.italicComments).toBeUndefined();
});

test("XML that is not a whole scheme is refused", () => {
  expect(importThemes("<html><body/></html>")).toEqual({ refused: REFUSED });
  expect(importThemes("<scheme name='broken'")).toEqual({ refused: REFUSED });
  expect(importThemes('<scheme name="cut"><attributes><option name="DEFAULT_KEYWORD">')).toEqual({ refused: REFUSED });
  expect(REFUSED).toContain("JetBrains");
});

test("a scheme parsed the way WebKit does, with the error inside its root, is refused", () => {
  // WebKit keeps the root of a broken file and puts a `<parsererror>` in it; jsdom makes the root the error.
  const Real = DOMParser;
  vi.stubGlobal(
    "DOMParser",
    class {
      parseFromString(text: string, type: DOMParserSupportedType) {
        const doc = new Real().parseFromString(`${text}</scheme>`, type);
        doc.documentElement.prepend(doc.createElement("parsererror"));
        return doc;
      }
    },
  );
  try {
    expect(importThemes('<scheme name="cut"><attributes>' + fg("DEFAULT_KEYWORD", "0033b3") + "</attributes>")).toEqual({ refused: REFUSED });
  } finally {
    vi.unstubAllGlobals();
  }
});
