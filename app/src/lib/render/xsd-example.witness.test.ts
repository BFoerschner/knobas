/**
 * The witness (the XSD design, section 24): every example of every global element of the three large families, with
 * required parts only and with everything, passes libxml2. The spike's rules reached 5,618 of 5,618; the build holds
 * them to it.
 */
import { expect, test } from "vitest";

import { check } from "./xsd-check";
import { makeExample, SEED } from "./xsd-example";
import { bundledFiles, ENTRY, familyFiles, loadAll, type Family } from "./xsd-fixture";

const FAMILIES: Family[] = ["opentrans-2.1", "ubl-2.1", "cii-d16b"];
/** The fewest examples each family makes, two an element: a set read short would make fewer and pass. */
const MADE: Partial<Record<Family, number>> = { "opentrans-2.1": 1494, "ubl-2.1": 3242, "cii-d16b": 2 };

test.each(FAMILIES)("every element of %s makes an example libxml2 takes, required parts and everything", async (name) => {
  const files = familyFiles(name);
  const set = loadAll(files, ENTRY[name]).set;
  const b = bundledFiles();
  const text = (p: string) => files.get(p) ?? b.texts.get(p) ?? null;
  const failed: string[] = [];
  let made = 0;
  for (const [key, c] of set.comps) {
    if (c.kind !== "el" || c.abstract) continue;
    for (const optional of ["required", "all"] as const) {
      const ex = makeExample(set, key, { seed: SEED, optional, repeats: 2, lang: "de", picks: {} });
      if (ex === null) continue;
      made += 1;
      const said = await check(set, text, ex.xml);
      if (said.state !== "checked" || said.findings.length > 0) failed.push(`${key} (${optional}): ${said.state === "checked" ? said.findings[0]?.message : said.why}`);
    }
  }
  expect(failed.slice(0, 5)).toEqual([]);
  // openTRANS's set declares 747 elements globally, UBL's invoice set 1,621, Cross Industry Invoice's one, its root.
  expect(made).toBeGreaterThanOrEqual(MADE[name] ?? 1);
}, 600_000);
