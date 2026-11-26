# The schema checks' live files

Made for knobas's schema explorer and the XML view's check (`docs/decisions/2026-11-23-an-xml-schema-is-explored-across-its-files-and-an-example-is-made-from-it.md`, S3 to S3c): a partner's order in three versions, each a small made schema, and an order of each with one departure (a quantity in words). No file of anybody else's.

The seed pushes them onto the live suite's Gitea (`testenv/seed-gitea.sh`, step 4e), each repository's folder onto its `main`:

- `ledger-gateway/schemas/partner/partner-order.xsd` declares version 1, and no other repository does: `ledger-api/orders/partner/order-17.xml` names a file that is not in `ledger-api`, and knobas finds the schema in `ledger-gateway` by itself.
- Version 2 is declared in `ledger-gateway` and in `payout-service`: `order-18.xml` is not checked until a person picks one (*Two repositories declare it*, *Point to a file…*).
- Version 3 is served at an address on the Gitea itself (`public/partner-order-3.schema`, named so no index of `.xsd` files finds it): `order-19.xml` names that address, and knobas offers *Download it*.

The browser fake does not read this folder, so its corpus and counts stay as they were.
