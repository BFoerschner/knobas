# Inter 4.1

The face knobas draws its interface, headings and Docs pages in (2026-11-20,
`docs/decisions/2026-11-20-the-faces-knobas-brings.md`).

- **Design:** Inter by Rasmus Andersson and the Inter Project Authors, <https://github.com/rsms/inter>, version 4.1
  (the files' `name` table: "Version 4.001;git-66647c0bb").
- **From:** Google Fonts' build of Inter, split by script, as the npm package `@fontsource-variable/inter` 5.3.0 ships
  it (<https://registry.npmjs.org/@fontsource-variable/inter/-/inter-5.3.0.tgz>, integrity
  `sha512-OupL48va4JNofb97w6NYeF9S7W/kHNKM0Er8Dem5nqi4jeOLrVJDoE8tZEpnMJmtkvNbB1EIPPwHcdkF6b1oUA==`): its
  `files/inter-*-opsz-normal.woff2`, the upright variable face with `wght` 100 to 900 and `opsz` 14 to 32. The
  `unicode-range` of each file in `../fonts.css` is the package's `opsz.css`'s.
- **Licence:** `OFL.txt`, the package's `LICENSE`: SIL Open Font License 1.1, "Copyright 2016 The Inter Project
  Authors (https://github.com/rsms/inter)". It reserves no font name (nor does Inter's own `LICENSE.txt` at the tag
  v4.1), so a subset may keep the name Inter. The `OS/2` table's `fsType` is 0: no restriction on embedding.

| File | Bytes | SHA-256 |
|---|---|---|
| `inter-latin-opsz-normal.woff2` | 72,920 | `2c295d99e26dcf357d4d01bcf270fd6924b600c9a13dd8c363ef114f4c6976fa` |
| `inter-latin-ext-opsz-normal.woff2` | 133,336 | `5e6d4fe9d9f4bff8b2a2469d25ab19576bb85331e22c6ed51398e16f95d56a9c` |
| `inter-cyrillic-opsz-normal.woff2` | 29,636 | `e1dea13a93c69f52837e370541ffe132508352666e9288ab910f2c422f4b9b50` |
| `inter-cyrillic-ext-opsz-normal.woff2` | 41,520 | `e3a8d2d7497a325b85c2e0c9d931ba1741415d23d1f828ebc5ce8c116dab44e8` |
| `inter-greek-opsz-normal.woff2` | 30,200 | `93c5ac80e25cda1cf289206c724027eeb187c9d398106f0079a912cbe80616ce` |
| `inter-greek-ext-opsz-normal.woff2` | 17,376 | `e0f32d9ee75a83f4031e1a7bc8926f962278e282d3535be7d8dbeb3bd62914b5` |
| `inter-vietnamese-opsz-normal.woff2` | 15,268 | `935ab355938e6cd9b5fa39f2c3250ee16f3b08807c65dc4162356b50dbc8c7a2` |
| `OFL.txt` | 4,477 | `3b0a5fca3d17942cde889069889dedbbbd075e9b599968c82a95f4d944e9b345` |
