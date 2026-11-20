/**
 * The faces knobas brings along, with what their licences ask to travel with them (2026-11-20,
 * `docs/decisions/2026-11-20-the-faces-knobas-brings.md`). Settings › Fonts lists them under *Fonts that come with
 * knobas*, and each licence's full text opens there: the SIL Open Font License asks that its copyright notice and text
 * go with every copy of the font, and the app is a copy. The texts are their own chunk, read when one is opened.
 *
 * Each face's files and licence are in `fonts/<folder>/`, with `SOURCE.md` naming where they came from, their version
 * and their SHA-256. `bundled-faces.test.ts` holds this list to the folders.
 */
export interface BundledFace {
  /** As a person names it. */
  name: string;
  /** The folder under `fonts/` that holds its files, `OFL.txt` and `SOURCE.md`. */
  folder: string;
  version: string;
  /** Who holds its copyright, as its licence says. */
  by: string;
  /** The Settings › Fonts roles it is the face of. */
  roles: string;
  /** The licence's full text. */
  licence: () => Promise<string>;
}

export const BUNDLED_FACES: readonly BundledFace[] = [
  {
    name: "Inter",
    folder: "inter",
    version: "4.1",
    by: "The Inter Project Authors",
    roles: "Interface, Headings, Reading",
    licence: () => import("./inter/OFL.txt?raw").then((text) => text.default),
  },
  {
    name: "Cascadia Code",
    folder: "cascadia-code",
    version: "2407.24",
    by: "Microsoft Corporation",
    roles: "Code",
    licence: () => import("./cascadia-code/OFL.txt?raw").then((text) => text.default),
  },
];

/** What every face here is licensed under. */
export const LICENCE_NAME = "SIL Open Font License 1.1";
