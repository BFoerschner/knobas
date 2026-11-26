# Cascadia Code and Cascadia Mono 2009.22

The older Cascadia, offered in Settings › Fonts › Code beside the current one (2026-11-21, Björn: "maybe include also
the older version of cascadia code then, I think that'd be interesting"; `docs/decisions/2026-11-21-fonts-you-can-tune.md`).
Its box drawing glyphs are taller than the current release's (`│` runs 1.56 em, from −891 to 2309 of 2048 units, and
its `rclt` feature swaps no box glyph), so boxes join at any of knobas's code row heights. Loaded only when chosen.

- **From:** Microsoft's release v2009.22,
  <https://github.com/microsoft/cascadia-code/releases/download/v2009.22/CascadiaCode-2009.22.zip>
  (8,217,333 bytes, SHA-256 `801bcc94b79ae46bc573c9c07feb44d581e12d15f79b176be165f00f8fdf9846`), its
  `woff2/CascadiaCode.woff2` (with ligatures, `calt`) and `woff2/CascadiaMono.woff2` (without): the variable files,
  weights 200 to 700. The release has no italic; comments draw upright (`fonts.css` declares no synthesised slant).
- **Licence:** `OFL.txt`, the repository's `LICENSE` at the tag v2009.22,
  <https://raw.githubusercontent.com/microsoft/cascadia-code/v2009.22/LICENSE>: SIL Open Font License 1.1,
  "Copyright (c) 2019 - Present, Microsoft Corporation, with Reserved Font Name Cascadia Code." The `OS/2` table's
  `fsType` is 0: no restriction on embedding. The text is the same as v2407.24's.
- **Unmodified.** The licence reserves the name "Cascadia Code": a modified copy (subset, re-encoded, hinting
  stripped) may not be called that. These are Microsoft's own woff2 files, byte for byte, so the name stands. Do not
  subset them.
- **Metrics** (the same as 2407.24's to the unit): 2048 an em, ascent 1900, descent 480, no line gap, advance 1200,
  x-height 1060. The `name` table's version is "Version 2009.022".

| File | Bytes | SHA-256 |
|---|---|---|
| `CascadiaCode.woff2` | 112,440 | `eaaf96dfdff5fe8a3ba122651b34410c7de07ae7ee3c476ad205be34e58df956` |
| `CascadiaMono.woff2` | 105,444 | `6bcbea7cc3f005774213c3ee86c1afb1bec34973d86dde3ae70eac5105929a1e` |
| `OFL.txt` | | `82c05d6c53dfa0c9025985c19e371810020b74ed2c61d51d370f2a8ab2506d52` (LF line ends, as every text file in the repository; the tag's file has CRLF, SHA-256 `51882cd3cdba4e16f220f44ddb08a635c38c44ea6e0975db2574f4be6f958238`) |
