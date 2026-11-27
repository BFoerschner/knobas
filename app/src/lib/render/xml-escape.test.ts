/** XML text and attribute values the app writes, read back as they were (#2525's examples, the XSD example, a check). */
import { expect, test } from "vitest";

import { xmlAttr, xmlText } from "./xml-escape";

test("text escapes what would start markup, an attribute value its quote and a line break too", () => {
  expect(xmlText(`a < b && c > "d"\n`)).toBe(`a &lt; b &amp;&amp; c &gt; "d"\n`);
  expect(xmlAttr(`a < b && c > "d"\ne`)).toBe("a &lt; b &amp;&amp; c &gt; &quot;d&quot;&#10;e");
  const read = new DOMParser().parseFromString(`<x a="${xmlAttr('one "two"\nthree & <four>')}">${xmlText("<five> & six")}</x>`, "application/xml");
  expect(read.documentElement.getAttribute("a")).toBe('one "two"\nthree & <four>');
  expect(read.documentElement.textContent).toBe("<five> & six");
});
