// An example's header keeps Copy and Save to Downloads on the title's line however long its meta grows (#2626): the
// meta the explorer answers is twice the seed's, and wrapped the trail under the title at 1440 px. jsdom lays nothing
// out, so the rule is held here as the stylesheet writes it; `xsd-example-open` in `testenv/perf/moves.mjs` holds the
// buttons themselves in Chrome.
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { expect, test } from "vitest";

const flat = (name: string) =>
  readFileSync(join(process.cwd(), "src", "lib", "tf", "css", name), "utf8")
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/\s+/g, " ");

test("an example header's meta takes only the room the name and the trail leave, and cuts its line rather than wrap the trail", () => {
  expect(flat("code.css")).toContain(".tf-exhead > .tf-fmeta { flex: 1 1 0; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }");
});
