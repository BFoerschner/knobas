# UBL 2.1: the invoice and the 13 files it reaches

A test fixture of the schema explorer (`docs/decisions/2026-11-23-an-xml-schema-is-explored-across-its-files-and-an-example-is-made-from-it.md`,
section 32). Every file is kept byte for byte as it was downloaded, with its own head: none may be edited and kept
under its name.

Downloaded on 2026-11-23 from `https://docs.oasis-open.org/ubl/os-UBL-2.1/xsd/`, `common/` and `maindoc/`. Unchanged,
with their heads, in the folders the specification gives them, so the invoice's `../common/` references stand.

**Notice.** Copyright © OASIS Open 2013. All Rights Reserved. "This document and translations of it may be copied and
furnished to others, and derivative works that comment on or otherwise explain it or assist in its implementation may be
prepared, copied, published, and distributed, in whole or in part, without restriction of any kind, provided that the
above copyright notice and this section are included on all such copies and derivative works. However, this document
itself may not be modified in any way, including by removing the copyright notice or references to OASIS, except as
needed for the purpose of developing any document or deliverable produced by an OASIS Technical Committee (in which case
the rules applicable to copyrights, as set forth in the OASIS IPR Policy, must be followed) or as required to translate
it into languages other than English." (the UBL 2.1 specification's notices; the files' own heads say "Copyright (c)
OASIS Open 2013. All Rights Reserved.")

14 files, 2,841,749 bytes.

| File | Bytes | SHA-256 |
|---|---|---|
| `xsd/common/CCTS_CCT_SchemaModule-2.1.xsd` | 45,268 | `dd546e4809df86b6445589f69f0d6c9df162840ae386574ddfc1da7638103e15` |
| `xsd/common/UBL-CommonAggregateComponents-2.1.xsd` | 2,420,444 | `939172ad8dd057cd403e7f763f6532184dd5ed4b9de24c42ebb35db4792ba613` |
| `xsd/common/UBL-CommonBasicComponents-2.1.xsd` | 219,895 | `bd4ad043ee1d9da1c7f8018dabf739cfafdfb59143d0d16b9ef769e6b7c408a7` |
| `xsd/common/UBL-CommonExtensionComponents-2.1.xsd` | 9,491 | `ad7a4e490978adfbcfc5ec0bb20941cf11ac960ccf0c4de8791a7c731a8dbe87` |
| `xsd/common/UBL-CommonSignatureComponents-2.1.xsd` | 5,548 | `3db472305f029bba5c1ae157bfd0178f715c3f9b94bd8e6c557dbce5e88da874` |
| `xsd/common/UBL-ExtensionContentDataType-2.1.xsd` | 4,314 | `fcee77a11870208e6377ea6311b9f2a050bca24bdad8606ea02d71e9f9e72f8d` |
| `xsd/common/UBL-QualifiedDataTypes-2.1.xsd` | 3,590 | `7dcb156e610239c97ae70940cf4653b88e48c3595bf5f56a2204a32e2893e6cf` |
| `xsd/common/UBL-SignatureAggregateComponents-2.1.xsd` | 7,784 | `17bb6b62d709b4fd81449a37655af36aa6a1276ad4fdb1b2e249a5ed4b7c2172` |
| `xsd/common/UBL-SignatureBasicComponents-2.1.xsd` | 4,207 | `cef924d7ba3d1d8ade14469325cde1364f8c174e46f0198ec02da8e9e748a489` |
| `xsd/common/UBL-UnqualifiedDataTypes-2.1.xsd` | 27,300 | `09052d406b4293e2a5f9c2bfee6df10ad4d8d5f0b36e24a6349d7f7936d89eb6` |
| `xsd/common/UBL-XAdESv132-2.1.xsd` | 21,664 | `a4f726bcf8cc3f7d9ffa4dab99e005535a8e8b60dced1e5d94578d2e05afa96e` |
| `xsd/common/UBL-XAdESv141-2.1.xsd` | 1,316 | `1fa4625e9cefcb7a9abb5ac1b64315547450031eece8a55bd584e4ba4b79dbc1` |
| `xsd/common/UBL-xmldsig-core-schema-2.1.xsd` | 10,750 | `101909c9f06456d61ddcc4fb982f1d40dc357b439f393b1a2eb46e42acd60809` |
| `xsd/maindoc/UBL-Invoice-2.1.xsd` | 60,178 | `40fae8cb436f3a9506d7acce65ba162caef3b0bed4d5cbc0992b2153ded4edf4` |
