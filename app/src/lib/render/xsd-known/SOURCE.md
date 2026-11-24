# Schemas that come with knobas

The schema explorer's last step when it looks for a file a schema names (`docs/decisions/2026-11-23-an-xml-schema-is-explored-across-its-files-and-an-example-is-made-from-it.md`,
section 17): the two W3C schemas an address in a schema most often names. A repository's own copy always wins. They load
with the schema worker's chunk.

| File | Namespace | Got from | Bytes | SHA-256 |
|---|---|---|---|---|
| `xml.xsd` | `http://www.w3.org/XML/1998/namespace` | `https://www.w3.org/2001/xml.xsd`, 2026-11-24 | 8,836 | `61960fb3131e38022caad5360e2f33a3382578ab3c80cd58bd74320ede61b20c` |
| `xmldsig-core-schema.xsd` | `http://www.w3.org/2000/09/xmldsig#` | the openTRANS 2.1 package (`fixtures/xsd/opentrans-2.1/`), the W3C's 2002 file | 10,293 | `35cf8197da812c85e40d57891b35c94187569ed474a2dac813ce5090dafcd35c` |

Both are unchanged, with their heads, under the W3C Software and Document License (`docs/third-party-notices.md`,
*Schemas that come with knobas*). Copyright © World Wide Web Consortium (Massachusetts Institute of Technology, European
Research Consortium for Informatics and Mathematics, Keio University, Beihang).
