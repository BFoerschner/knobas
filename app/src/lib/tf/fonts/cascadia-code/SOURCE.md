# Cascadia Code 2407.24

The face knobas draws code in (2026-11-20, `docs/decisions/2026-11-20-the-faces-knobas-brings.md`).

- **From:** Microsoft's release v2407.24,
  <https://github.com/microsoft/cascadia-code/releases/download/v2407.24/CascadiaCode-2407.24.zip>
  (SHA-256 `e67a68ee3386db63f48b9054bd196ea752bc6a4ebb4df35adce6733da50c8474`), its `woff2/CascadiaCode.woff2` and
  `woff2/CascadiaCodeItalic.woff2`: the variable files, weights 200 to 700, with ligatures (`calt`).
- **Licence:** `OFL.txt`, the repository's `LICENSE` at the tag v2407.24,
  <https://raw.githubusercontent.com/microsoft/cascadia-code/v2407.24/LICENSE>: SIL Open Font License 1.1,
  "Copyright (c) 2019 - Present, Microsoft Corporation, with Reserved Font Name Cascadia Code." The `OS/2` table's
  `fsType` is 0: no restriction on embedding.
- **Unmodified.** The licence reserves the name "Cascadia Code": a modified copy (subset, re-encoded, hinting
  stripped) may not be called that. These are Microsoft's own woff2 files, byte for byte, so the name stands. Do not
  subset them; take a newer release whole.

| File | Bytes | SHA-256 |
|---|---|---|
| `CascadiaCode.woff2` | 218,824 | `3ec1f7e7250c055d35504a1500e8a9ba12b26f25214a86da6537e28abc60dccc` |
| `CascadiaCodeItalic.woff2` | 159,760 | `7b38c6e24c4f114ecdbc73c9114bf59cd7e998309a8a2ee54ef1e0289481405f` |
| `OFL.txt` | | `82c05d6c53dfa0c9025985c19e371810020b74ed2c61d51d370f2a8ab2506d52` (LF line ends, as every text file in the repository; the tag's file has CRLF, SHA-256 `51882cd3cdba4e16f220f44ddb08a635c38c44ea6e0975db2574f4be6f958238`) |
