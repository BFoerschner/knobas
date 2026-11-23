/** What is XML (the XML view's design, 2026-11-23, section 4). */
import { expect, test } from "vitest";

import { hasDeclaration, isXmlFile, startsWithTag, SNIFF_BYTES, XML_ENDINGS } from "./xml-sniff";

test("a declaration makes any file XML, under a byte-order mark and white space", () => {
  expect(isXmlFile('<?xml version="1.0"?>\n<a/>', "notes.txt")).toBe(true);
  expect(isXmlFile('﻿  \n<?xml version="1.0" encoding="UTF-8"?><a/>', "data")).toBe(true);
  expect(hasDeclaration("<?xml-stylesheet href='a.xsl'?>")).toBe(false);
  expect(isXmlFile("<?xmlfoo?><a/>", "a.txt")).toBe(false);
});

test("an XML ending and a start tag make an XML file, past comments, instructions and a DOCTYPE", () => {
  expect(isXmlFile("<!-- built -->\n<!DOCTYPE project [ <!ENTITY a 'b>'> ]>\n<project/>", "App.csproj")).toBe(true);
  expect(isXmlFile("<?pi x?>\n<config/>", "web.config")).toBe(true);
  expect(startsWithTag("<!-- not closed")).toBe(false);
});

test("an ending alone, or a start tag alone, is not XML", () => {
  expect(isXmlFile('{ "a": 1 }', "app.config")).toBe(false);
  expect(isXmlFile("[section]\nkey=1", "app.manifest")).toBe(false);
  expect(isXmlFile("<script>\nlet a = 1;\n</script>", "App.svelte")).toBe(false);
  expect(isXmlFile("<template><div/></template>", "x.vue")).toBe(false);
  expect(isXmlFile("", "empty.xml")).toBe(false);
  expect(isXmlFile("   ", "blank.xml")).toBe(false);
});

test("the ending is compared without its case, and the path is the file's name's", () => {
  expect(isXmlFile("<a/>", "src/Main.XAML")).toBe(true);
  expect(isXmlFile("<a/>", "xml.d/notes")).toBe(false);
  expect(XML_ENDINGS).toContain("pom");
});

test("only the head is read", () => {
  const far = " ".repeat(SNIFF_BYTES) + "<?xml version='1.0'?>";
  expect(isXmlFile(far, "a.txt")).toBe(false);
});
