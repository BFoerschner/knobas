/**
 * The explorer's rail, its pure half (ux31, section 5): untagged operations grouped by the first part of their path
 * that tells them apart, each group's shared start, the path tree, a path cut for the rail, and the rows.
 */
import { expect, test } from "vitest";

import { SpecFiles, type OpRow, type SpecIndex } from "./openapi";
import { DEEP_30_JSON, MODEL_31_JSON, PETS_30_YAML, bigSpec } from "./openapi-fixture";
import { commonSegments, firstOpen, fitPath, groupsOf, pathTree, railRows, type RailRow } from "./openapi-view";

function indexOf(text: string): SpecIndex {
  const answer = new SpecFiles("spec.json").open(text);
  if (!answer.ok) throw new Error(answer.error);
  return answer.index;
}

const label = (r: RailRow) =>
  r.kind === "overview" ? "Overview" : r.kind === "group" ? `# ${r.group.name}` : r.kind === "dir" ? `/ ${r.dir.name}` : r.kind === "webhooks" ? `# Webhooks ${r.n}` : r.kind === "op" ? `${"  ".repeat(r.d)}${r.op.method} ${r.shown}` : r.kind === "schema" ? `s ${r.name}` : r.kind;

test("operations no tag holds, every path under /v1, group by the part after it", () => {
  const index = indexOf(DEEP_30_JSON);
  expect(groupsOf(index).map((g) => [g.name, g.ops.length])).toEqual([
    ["/v1/charges", 3],
    ["/v1/customers", 2],
    ["/v1/payouts", 1],
  ]);
});

test("a spec with one untagged path groups by its first part; a shared start never goes past a parameter", () => {
  const index = indexOf(PETS_30_YAML);
  expect(groupsOf(index).at(-1)?.name).toBe("/health");
  expect(commonSegments(["/a/{id}", "/a/{id}/b"])).toBe(1);
  expect(commonSegments(["/v1/charges", "/v1/charges/{charge}"])).toBe(1);
  // Never the whole of the shortest path.
  expect(commonSegments(["/v1/charges", "/v1/charges/x"])).toBe(1);
  expect(commonSegments(["/a/b", "/a/b"])).toBe(1);
  expect(commonSegments(["/x", "/y"])).toBe(0);
});

test("a tag's group says its shared start, and its rows say their paths from there", () => {
  const ops: OpRow[] = ["/app/rest/builds", "/app/rest/builds/{id}/status", "/app/rest/builds/{id}"].map((path, i) => ({
    i, method: "get", path, summary: "", operationId: null, tags: ["Build"], deprecated: false, anchor: `get ${path}`,
  }));
  const index = { ...indexOf(PETS_30_YAML), tags: [{ name: "Build", description: "" }], ops, pathRefs: [], webhooks: [] };
  const groups = groupsOf(index);
  expect(groups[0]).toMatchObject({ name: "Build", shared: "/app/rest" });
  const rows = railRows(index, { mode: "tags", groups, tree: null, open: new Set(["g:Build"]), ranked: null });
  expect(rows.map(label)).toEqual(["Overview", "# Build", "  get /builds", "  get /builds/{id}/status", "  get /builds/{id}"]);
});

test("the path tree joins a chain of single folders, never a folder that holds operations", () => {
  const ops = ["/app/rest/builds", "/app/rest/agents", "/app/rest/agents/{id}"].map((path, i) => ({ i, method: "get", path } as OpRow));
  const tree = pathTree(ops);
  expect(tree.dirs.map((d) => d.name)).toEqual(["app/rest"]);
  const rest = tree.dirs[0]!;
  expect(rest.dirs.map((d) => [d.name, d.count])).toEqual([
    ["builds", 1],
    ["agents", 2],
  ]);
  // `agents` holds an operation of its own, so `{id}` is a folder under it, not joined.
  expect(rest.dirs[1]!.ops.length).toBe(1);
  expect(rest.dirs[1]!.dirs.map((d) => d.name)).toEqual(["{id}"]);
});

test("a long path keeps its first and last whole parts; a short one or one of two parts is whole", () => {
  expect(fitPath("/organizations/{org}/settings/billing/retention-limit", 34)).toBe("/organizations/…/retention-limit");
  const fitted = fitPath("/a/bbbbbbbbbb/cccccccccc/dddddddddd/eeeeeeeeee", 30);
  expect(fitted).toBe("/a/…/dddddddddd/eeeeeeeeee");
  expect(fitted.length).toBeLessThanOrEqual(30);
  expect(fitPath("/short", 4)).toBe("/short");
  expect(fitPath("/very-long-part/another-very-long-part", 10)).toBe("/very-long-part/another-very-long-part");
  // A part is never cut inside: every part of the answer is a whole part of the path.
  for (const part of fitPath("/one/two/three/four/five/six", 18).split("/").filter((s) => s !== "" && s !== "…")) {
    expect(["one", "two", "three", "four", "five", "six"]).toContain(part);
  }
});

test("5,000 operations with every fold shut are the overview and one row a group", () => {
  const index = indexOf(bigSpec(5000));
  const groups = groupsOf(index);
  const open = firstOpen(index, groups, pathTree(index.ops));
  const rows = railRows(index, { mode: "tags", groups, tree: null, open, ranked: null });
  expect(rows.length).toBe(1 + groups.length);
});

test("a small spec opens with every fold open; the path tree's single top chain is open", () => {
  const index = indexOf(MODEL_31_JSON);
  const groups = groupsOf(index);
  const tree = pathTree(index.ops);
  const open = firstOpen(index, groups, tree);
  expect(railRows(index, { mode: "tags", groups, tree, open, ranked: null }).map(label)).toEqual([
    "Overview",
    "# /things",
    "  get /things",
    "  post /things",
    "  get /things/{id}",
    "# Webhooks 1",
    "  post thingMade",
  ]);
  expect(railRows(index, { mode: "paths", groups, tree, open, ranked: null }).map(label)).toEqual([
    "Overview",
    "/ things",
    "  get /things",
    "  post /things",
    "/ {id}",
    "    get /{id}",
    "# Webhooks 1",
    "  post thingMade",
  ]);
  expect(railRows(index, { mode: "schemas", groups, tree, open, ranked: null }).map(label)).toEqual(["s Thing"]);
});

test("with words typed the rail is flat: the operations kept, best first, then the schemas; Schemas lists only schemas", () => {
  const index = indexOf(MODEL_31_JSON);
  const op = index.ops[2]!;
  const ranked = { ops: [{ op, marks: [[0, 3] as const] }], schemas: [{ name: "Thing", marks: null }] };
  const groups = groupsOf(index);
  expect(railRows(index, { mode: "tags", groups, tree: null, open: new Set(), ranked }).map(label)).toEqual(["get /things/{id}", "s Thing"]);
  expect(railRows(index, { mode: "schemas", groups, tree: null, open: new Set(), ranked }).map(label)).toEqual(["s Thing"]);
});
