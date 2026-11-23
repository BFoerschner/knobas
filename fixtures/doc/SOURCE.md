# The XML view's fixtures

Made for the XML view's design (2026-11-23, `docs/decisions/2026-11-23-an-xml-file-is-read-as-a-document.md`) by the
clickdummy's data script, `build-xmlview-data.mjs`, from the XSD design's own openTRANS 2.1 example: no document of
anybody else's.

- `order-4710.xml`: an openTRANS 2.1 order of 1,814 lines, valid against openTRANS's schemas (libxml2 judged it). It
  carries a real attachment in openTRANS's own element for one (`MIME_EMBEDDED/MIME_DATA`, declared
  `xmime:base64Binary`), an image at an address, an image in the repository (`../media/kugellager-6204-2rs.png`),
  addresses and a path in text, and in the item's free extension (`ITEM_UDX`, `xsd:anyType`) a JWT, a JSON string, an
  XML string, a batch number that only looks like base64, and a PNG and a gzipped log in base64 that nothing declares.
- `order-4711.xml`: the same order with five departures put in by hand (libxml2 still finds the five).
- `pom.xml`: a Maven POM, whose schema is named by an address.
- `kugellager-6204-2rs.png`, `rahmenvertrag-2026.pdf`: the image the orders name and a contract, made byte by byte in
  the same script (a ball bearing drawn by a function of its pixels; a PDF of plain text).

Large files are made in the tests that need them (the order's first item repeated), never committed.
