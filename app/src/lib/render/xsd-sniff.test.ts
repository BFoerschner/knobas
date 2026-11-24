import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { expect, test } from "vitest";

import { isXsd, sniffNamespace } from "./xsd-sniff";

const XS = 'xmlns:xs="http://www.w3.org/2001/XMLSchema"';

test("a schema with a BOM, a declaration, comments and another prefix is one", () => {
  expect(isXsd(`﻿<?xml version="1.0"?>\n<!-- head -->\n<xs:schema ${XS} targetNamespace="urn:a"/>`)).toBe(true);
  expect(isXsd(`<xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema"></xsd:schema>`)).toBe(true);
  expect(sniffNamespace(`<xs:schema ${XS} targetNamespace='urn:a'/>`)).toBe("urn:a");
  expect(sniffNamespace(`<xs:schema ${XS}/>`)).toBe("");
});

test("the namespace as the default is one; xmldsig's DOCTYPE with its internal subset is passed", () => {
  expect(isXsd(`<schema xmlns="http://www.w3.org/2001/XMLSchema" targetNamespace="urn:b"><element name="x"/></schema>`)).toBe(true);
  const dsig = readFileSync(resolve(__dirname, "xsd-known/xmldsig-core-schema.xsd"), "utf8");
  expect(sniffNamespace(dsig)).toBe("http://www.w3.org/2000/09/xmldsig#");
});

test("a prefix bound to another namespace, the prefix xs assumed, or an html root is not one", () => {
  expect(isXsd(`<xs:schema xmlns:xs="urn:not-xml-schema"/>`)).toBe(false);
  expect(isXsd(`<xs:schema/>`)).toBe(false);
  expect(isXsd(`<html><body/></html>`)).toBe(false);
  expect(isXsd(`<xs:element ${XS}/>`)).toBe(false);
  expect(sniffNamespace(`<root/>`)).toBeUndefined();
});
