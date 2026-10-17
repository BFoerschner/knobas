/** The toast an import leaves in Dependencies (#1066). */
import { expect, test } from "vitest";

import { importedLine } from "./import";

const none = { assets_created: 0, routes_created: 0, properties_set: 0, properties_kept: 0, monitors_kept: 0, monitors_linked: 0 };

test("an import that wrote nothing says so", () => {
  expect(importedLine("testenv", none)).toBe("Imported testenv: nothing changed");
});

test("an import names what it wrote, plural where it is", () => {
  expect(importedLine("testenv", { ...none, assets_created: 3, routes_created: 1, properties_set: 2, properties_kept: 2, monitors_linked: 1 })).toBe(
    "Imported testenv: 3 assets created, 1 route created, 2 properties set, 2 properties kept as you edited them, 1 monitor attached",
  );
});
