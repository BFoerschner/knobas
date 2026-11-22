/**
 * The explorer's rail, its pure half (ux31, section 5): untagged operations grouped by the first part of their path
 * that tells them apart, each group's shared start, the path tree, a path cut for the rail, and the rows.
 */
import { readFileSync, writeFileSync } from "node:fs";

import { expect, test } from "vitest";

import { SpecFiles, type OpRow, type SpecIndex } from "./openapi";
import { DEEP_30_JSON, MODEL_31_JSON, PETS_30_YAML, bigSpec } from "./openapi-fixture";
import { commonSegments, firstOpen, fitPath, fitRows, groupsOf, matches, pathTree, railRows, terms, TELL_APART, type RailRow } from "./openapi-view";

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
  expect(commonSegments(["/a/{id}/b", "/a/{id}/c"])).toBe(1);
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
  // As many whole parts at the end as fit, and not one more.
  expect(fitPath("/a/bb/cc/dd/ee", 11)).toBe("/a/…/dd/ee");
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
  const ranked = { ops: [{ op, marks: [0, 3] }], schemas: [{ name: "Thing", marks: null }] };
  const groups = groupsOf(index);
  expect(railRows(index, { mode: "tags", groups, tree: null, open: new Set(), ranked }).map(label)).toEqual(["get /things/{id}", "s Thing"]);
  expect(railRows(index, { mode: "schemas", groups, tree: null, open: new Set(), ranked }).map(label)).toEqual(["s Thing"]);
});

/** Operation rows over `paths`, all `GET` unless a path starts with its method, as the filter lists them. */
function opRows(paths: readonly string[], d = 0): RailRow[] {
  return paths.map((line, i) => {
    const [method, path] = line.includes(" ") ? line.split(" ") : ["get", line];
    const op = { i, method: method!, path: path!, summary: "", tags: [], operationId: null, deprecated: false, anchor: `${method} ${path}` } as unknown as OpRow;
    return { k: `o:${i}`, kind: "op", d, op, shown: path!, marks: null };
  });
}

/** What each row reads: an operation its method and its path as fitted, any other row its key. */
function readAs(rows: readonly RailRow[], chars: number): string[] {
  const fits = fitRows(rows, () => chars);
  return rows.map((r) => (r.kind === "op" ? `${r.op.method} ${fits.get(r.k)}` : r.k));
}

/** The pairs of operation rows of one run, at most `within` apart, that read the same though their paths differ. */
function readingSame(rows: readonly RailRow[], chars: number, within: number): string[] {
  const fits = fitRows(rows, () => chars);
  const out: string[] = [];
  let run: (RailRow & { kind: "op" })[] = [];
  const check = () => {
    for (let i = 0; i < run.length; i++)
      for (let j = i + 1; j < run.length && j <= i + within; j++) {
        const a = run[i]!;
        const b = run[j]!;
        if (a.op.method === b.op.method && a.shown !== b.shown && fits.get(a.k) === fits.get(b.k)) out.push(`${a.op.method} ${a.shown} | ${b.shown} as ${fits.get(a.k)}`);
      }
    run = [];
  };
  for (const r of rows) {
    if (r.kind === "op") run.push(r);
    else check();
  }
  check();
  return out;
}

// GitHub's operations under the filter *list repositories*, in the order Björn's shot drew them (2026-11-22).
const LIST_REPOSITORIES = [
  "/installation/repositories",
  "/networks/{owner}/{repo}/events",
  "/orgs/{org}/actions/permissions/repositories",
  "/orgs/{org}/actions/runner-groups/{runner_group_id}/repositories",
  "put /orgs/{org}/actions/permissions/repositories/{repository_id}",
  "delete /orgs/{org}/actions/permissions/repositories/{repository_id}",
  "/orgs/{org}/actions/secrets/{secret_name}/repositories",
  "/orgs/{org}/actions/variables/{name}/repositories",
  "/orgs/{org}/codespaces/secrets/{secret_name}/repositories",
  "/orgs/{org}/dependabot/secrets/{secret_name}/repositories",
  "/orgs/{org}/private-registries/{secret_name}/repositories",
  "/orgs/{org}/settings/network-configurations/{id}/repositories",
  "/orgs/{org}/code-security/configurations/{configuration_id}/repositories",
  "/orgs/{org}/repos",
  "/orgs/{org}/teams/{team_slug}/repos",
  "/repositories",
  "/user/codespaces/secrets/{secret_name}/repositories",
  "/user/installations/{installation_id}/repositories",
  "/user/repos",
];

test("a cut row keeps the parts that tell it from its neighbours: no two rows read the same (Björn's shot)", () => {
  const rows = opRows(LIST_REPOSITORIES);
  const read = readAs(rows, 32);
  expect(read).toEqual([
    "get /installation/repositories",
    "get /networks/{owner}/{repo}/events",
    "get /orgs/…/permissions/repositories",
    "get …/runner-groups/…/repositories",
    "put …/permissions/…/{repository_id}",
    "delete …/permissions/…/{repository_id}",
    "get /orgs/…/secrets/…/repositories",
    "get …/actions/variables/…/repositories",
    "get …/codespaces/…/repositories",
    "get …/dependabot/…/repositories",
    "get …/private-registries/…/repositories",
    "get /orgs/…/settings/…/repositories",
    "get …/code-security/…/repositories",
    "get /orgs/{org}/repos",
    "get /orgs/…/teams/{team_slug}/repos",
    "get /repositories",
    "get /user/codespaces/…/repositories",
    "get …/installations/…/repositories",
    "get /user/repos",
  ]);
  for (let i = 1; i < read.length; i++) expect(read[i]).not.toBe(read[i - 1]);
  expect(readingSame(rows, 32, rows.length)).toEqual([]);
});

test("a row with no neighbour is cut as before: its first part and as many last parts as fit", () => {
  expect(readAs(opRows(["/organizations/{org}/settings/billing/retention-limit"]), 34)).toEqual(["get /organizations/…/retention-limit"]);
  // A {parameter} goes before a plain part: it says least.
  expect(readAs(opRows(["/repos/{owner}/{repo}/actions/runs/{run_id}/logs"]), 30)).toEqual(["get /repos/…/actions/runs/…/logs"]);
});

test("a pair the neighbour rule leaves the same is told apart, and one only whole parts can tell apart is drawn whole", () => {
  // Rows two apart: the middle row differs from each, but the outer two would read the same.
  const rows = opRows(["/a/one/xxxxxxxx/zz/end", "/a/two/yyyyyyyy/zz/end", "/a/one/wwwwwwww/zz/end"]);
  expect(readingSame(rows, 14, TELL_APART)).toEqual([]);
  // Only the first and last parts stay, and they say all there is: the pair cannot be cut apart within the width.
  const tight = opRows(["/aaaa/bbbbbbbbbbbb/cccc", "/aaaa/dddddddddddd/cccc"]);
  expect(readingSame(tight, 10, TELL_APART)).toEqual([]);
});

test("a fold head ends a run: the rows of two groups are not compared", () => {
  const index = indexOf(PETS_30_YAML);
  const groups = groupsOf(index);
  const rows = railRows(index, { mode: "tags", groups, tree: null, open: new Set(groups.map((g) => g.key)), ranked: null });
  const fits = fitRows(rows, () => 80);
  for (const r of rows) if (r.kind === "op") expect(fits.get(r.k)).toBe(r.shown);
});

/**
 * GitHub's 1,281 operations: no two adjacent rows read the same, with every fold open in *Tags* and in *Paths*, and
 * under the filters *list repositories*, *repos* and *issues* in the spec's order, by path and reversed. Runs where
 * `KNOBAS_OAS_SPECS` names a folder holding `github.json`.
 */
const real = process.env["KNOBAS_OAS_SPECS"];
test.skipIf(real === undefined)("GitHub's rail never draws two rows near each other that read the same", () => {
  const index = indexOf(readFileSync(`${real}/github.json`, "utf8"));
  const out: string[] = [];
  expect(index.ops.length).toBe(1281);
  const groups = groupsOf(index);
  const tree = pathTree(index.ops);
  const lists: [string, RailRow[]][] = [];
  const allOpen = new Set<string>(["w", ...groups.map((g) => g.key)]);
  const dirs = (d: typeof tree) => {
    for (const k of d.dirs) {
      allOpen.add(`p:${k.path}`);
      dirs(k);
    }
  };
  dirs(tree);
  lists.push(["tags", railRows(index, { mode: "tags", groups, tree: null, open: allOpen, ranked: null })]);
  lists.push(["paths", railRows(index, { mode: "paths", groups, tree, open: allOpen, ranked: null })]);
  for (const typed of ["list repositories", "repos", "issues"]) {
    const words = terms(typed);
    const kept = index.ops.filter((op) => matches(`${op.method} ${op.path} ${op.summary} ${op.tags.join(" ")} ${op.operationId ?? ""}`.toLowerCase(), words));
    const byPath = [...kept].sort((a, b) => a.path.localeCompare(b.path) || a.method.localeCompare(b.method));
    for (const [how, ops] of [["spec", kept], ["path", byPath], ["reversed", [...byPath].reverse()]] as const) {
      lists.push([`"${typed}" ${how}`, railRows(index, { mode: "tags", groups, tree: null, open: new Set(), ranked: { ops: ops.map((op) => ({ op, marks: null })), schemas: [] } })]);
    }
  }
  for (const chars of [24, 30, 34, 44]) {
    for (const [name, rows] of lists) {
      const read = readAs(rows, chars);
      const adjacent = read.filter((r, i) => i > 0 && r === read[i - 1]);
      const near = readingSame(rows, chars, TELL_APART);
      if (chars === 30) out.push(`${name}: ${read.length} rows, ${adjacent.length} adjacent the same, ${near.length} pairs within ${TELL_APART} the same`);
      expect(adjacent, `${name} at ${chars}`).toEqual([]);
      expect(near, `${name} at ${chars}`).toEqual([]);
    }
  }
  if (process.env["KNOBAS_OAS_REPORT"] !== undefined) writeFileSync(process.env["KNOBAS_OAS_REPORT"], `${out.join("\n")}\n`);
});
