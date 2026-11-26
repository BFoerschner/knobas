/**
 * The two W3C schemas that come with knobas (the XSD design, section 17): the last step when a schema names a file.
 * Loaded with the schema worker's chunk, never at start-up (`xsd-known/SOURCE.md`).
 */
import xmlXsd from "./xsd-known/xml.xsd?raw";
import xmldsig from "./xsd-known/xmldsig-core-schema.xsd?raw";

import type { Bundled } from "./xsd-answer";
import { ns } from "./xml-names";

export const BUNDLED: Bundled = {
  byNs: new Map([
    [ns("www.w3.org/XML/1998/namespace"), "@knobas/xml.xsd"],
    [ns("www.w3.org/2000/09/xmldsig#"), "@knobas/xmldsig-core-schema.xsd"],
  ]),
  texts: new Map([
    ["@knobas/xml.xsd", xmlXsd],
    ["@knobas/xmldsig-core-schema.xsd", xmldsig],
  ]),
};
