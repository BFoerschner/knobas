# Cascadia Mono 2407.24

The face knobas draws code in (2026-11-21, `docs/decisions/2026-11-21-code-is-cascadia-mono.md`): Cascadia Code
without its programming ligatures, so `->`, `=>`, `---`, `|--` and box drawing stay the characters they are.

- **From:** Microsoft's release v2407.24, the same archive as `../cascadia-code/`,
  <https://github.com/microsoft/cascadia-code/releases/download/v2407.24/CascadiaCode-2407.24.zip>
  (SHA-256 `e67a68ee3386db63f48b9054bd196ea752bc6a4ebb4df35adce6733da50c8474`), its `woff2/CascadiaMono.woff2` and
  `woff2/CascadiaMonoItalic.woff2`: the variable files, weights 200 to 700. Its metrics are Cascadia Code's to the unit
  (2048 an em, x-height 1060, cap height 1420, ascent 1900, descent 480, advance 1200), so switching between the two
  moves nothing.
- **Licence:** `OFL.txt`, the repository's `LICENSE` at the tag v2407.24,
  <https://raw.githubusercontent.com/microsoft/cascadia-code/v2407.24/LICENSE>: SIL Open Font License 1.1,
  "Copyright (c) 2019 - Present, Microsoft Corporation, with Reserved Font Name Cascadia Code." The release ships
  Cascadia Mono under that one licence. The `OS/2` table's `fsType` is 0: no restriction on embedding.
- **Unmodified.** The licence reserves the name "Cascadia Code", and Cascadia Mono is the same family's face: a
  modified copy (subset, re-encoded, hinting stripped) may carry neither name. These are Microsoft's own woff2 files,
  byte for byte. Do not subset them; take a newer release whole.

| File | Bytes | SHA-256 |
|---|---|---|
| `CascadiaMono.woff2` | 210,484 | `7d4986a68fbbd674c90cdd8cd5d4a28c698640d3472572b006d2978006049799` |
| `CascadiaMonoItalic.woff2` | 153,708 | `c00f68dbd1b860cd5634e7c446d6fe68fd7f64e18ce3eb8ef7e52cd27f27116a` |
| `OFL.txt` | | `82c05d6c53dfa0c9025985c19e371810020b74ed2c61d51d370f2a8ab2506d52` (LF line ends, as every text file in the repository; the tag's file has CRLF, SHA-256 `51882cd3cdba4e16f220f44ddb08a635c38c44ea6e0975db2574f4be6f958238`) |
