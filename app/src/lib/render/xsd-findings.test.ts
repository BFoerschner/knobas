import { expect, test } from "vitest";

import { findingWords, plain } from "./xsd-findings";

const label = (ns: string, local: string) => (ns === "urn:b" ? `bmecat:${local}` : local);
const say = (message: string) => plain(findingWords({ message, line: 1, xpath: null }, label).text);

test("each sentence libxml2 writes, in knobas's words (section 26)", () => {
  expect(say("Element '{urn:a}DELIVERY_DATE': This element is not expected. Expected is ( {urn:a}ORDER_DATE ).")).toBe("DELIVERY_DATE stands where ORDER_DATE is expected.");
  expect(say("Element '{urn:a}DISCOUNT': This element is not expected. Expected is one of ( {urn:a}A, {urn:a}B, {urn:b}C ).")).toBe("DISCOUNT may not stand here. Expected: A, B, bmecat:C.");
  expect(say("Element '{urn:a}X': This element is not expected.")).toBe("X may not stand here.");
  expect(say("Element '{urn:a}ORDER_INFO': Missing child element(s). Expected is ( {urn:a}ORDER_DATE ).")).toBe("ORDER_INFO ends too soon: ORDER_DATE is missing.");
  expect(say("Element '{urn:a}QUANTITY': 'zwölf' is not a valid value of the atomic type '{urn:b}dtNUMBER'.")).toBe("zwölf is not a bmecat:dtNUMBER, which is what QUANTITY holds.");
  expect(say("Element '{urn:b}ORDER_UNIT': [facet 'enumeration'] The value 'Stück' is not an element of the set {'C62', 'H87', 'KGM'}.")).toBe("Stück is not one of the 3 values bmecat:ORDER_UNIT may hold.");
  expect(say("Element '{urn:a}X': [facet 'pattern'] The value 'tRUe' is not accepted by the pattern '[Tt]rue|[Ff]alse'.")).toBe("tRUe does not match the pattern X is held to, [Tt]rue|[Ff]alse.");
  expect(say("Element '{urn:b}SUPPLIER_PID': [facet 'maxLength'] The value has a length of '37'; this exceeds the allowed maximum length of '32'.")).toBe("bmecat:SUPPLIER_PID is 37 characters long; it may be 32 at most.");
  expect(say("Element '{urn:a}N': [facet 'minInclusive'] The value '-1' is less than the minimum value allowed ('0').")).toBe("-1 is out of range for N: at least 0.");
  expect(say("Element '{urn:a}ORDER': The attribute 'version' is required but missing.")).toBe("ORDER lacks its attribute version.");
  expect(say("Element '{urn:a}ORDER', attribute 'a': The attribute 'a' is not allowed.")).toBe("ORDER has an attribute the schema does not know, a.");
  expect(say("Element '{urn:a}X': Character content other than whitespace is not allowed because the content type is 'element-only'.")).toBe("X holds text, and may hold only elements.");
  expect(say("Element '{urn:a}X': Element content is not allowed, because the content type is a simple type definition.")).toBe("X holds elements, and may hold only a value.");
  expect(say("Element '{urn:a}X': No matching global declaration available for the validation root.")).toBe("The schema declares no element X a document may start with.");
  expect(say("Element '{urn:a}X': The value 'b' does not match the fixed value constraint 'a'.")).toBe("X must be a; it is b.");
});

test("a sentence knobas has no words for is kept whole, as libxml2 wrote it", () => {
  const w = findingWords({ message: "Element '{urn:a}X': Something new libxml2 says.\n", line: 4, xpath: "/*" }, label);
  expect(w.said).toBe(false);
  expect(plain(w.text)).toBe("Element '{urn:a}X': Something new libxml2 says.");
});
