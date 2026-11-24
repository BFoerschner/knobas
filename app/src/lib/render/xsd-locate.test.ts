import { describe, expect, test } from "vitest";

import { joinPath, locate, nearest, type LocateIn, type SchemaIndex } from "./xsd-locate";

const NS = { a: "urn:a", b: "urn:b" };
function inn(files: Record<string, string | undefined>, opts: Partial<LocateIn> & { indexed?: boolean; catalogs?: SchemaIndex["catalogs"] } = {}): LocateIn {
  const paths = new Set(Object.keys(files));
  const declares = new Map(Object.entries(files));
  const index: SchemaIndex | null = opts.indexed === false ? null : { declares, catalogs: opts.catalogs ?? [], total: paths.size, read: paths.size };
  return {
    paths,
    declares: (p) => (declares.has(p) ? declares.get(p) : null),
    pointed: opts.pointed ?? [],
    index,
    elsewhere: opts.elsewhere ?? null,
    bundled: opts.bundled ?? new Map(),
  };
}

describe("the order (section 14)", () => {
  test("beside it, with ../ read from the file's folder", () => {
    const got = locate(inn({ "s/main.xsd": NS.a, "lib/b.xsd": NS.b }), "s/main.xsd", NS.a, "import", NS.b, "../lib/b.xsd");
    expect(got).toMatchObject({ to: "lib/b.xsd", how: "beside" });
  });

  test("a path that climbs out of the repository is not found beside it", () => {
    expect(joinPath("main.xsd", "../b.xsd")).toBeNull();
    const got = locate(inn({ "main.xsd": NS.a }), "main.xsd", NS.a, "import", NS.b, "../b.xsd");
    expect(got).toMatchObject({ to: null, why: "No file of that name is here." });
  });

  test("the index is asked for only when beside it fails", () => {
    const files = { "main.xsd": NS.a, "b.xsd": NS.b };
    expect(locate(inn(files, { indexed: false }), "main.xsd", NS.a, "import", NS.b, "b.xsd")).toMatchObject({ how: "beside" });
    expect(locate(inn(files, { indexed: false }), "main.xsd", NS.a, "import", NS.b, "c.xsd")).toEqual({ need: ["?index"] });
  });

  test("a catalog's uri, rewriteURI and uriSuffix answer before a name", () => {
    const files = { "main.xsd": NS.a, "x/real.xsd": NS.b, "y/b.xsd": NS.b };
    const catalogs = [{ path: "catalog.xml", entries: [{ kind: "exact" as const, match: "http://b.example/b.xsd", to: "x/real.xsd" }] }];
    expect(locate(inn(files, { catalogs }), "main.xsd", NS.a, "import", NS.b, "http://b.example/b.xsd")).toMatchObject({ to: "x/real.xsd", how: "catalog", via: "catalog.xml" });
    const prefix = [{ path: "c.xml", entries: [{ kind: "prefix" as const, match: "http://b.example/", to: "x/" }] }];
    const f2 = { "main.xsd": NS.a, "x/real.xsd": NS.b };
    expect(locate(inn(f2, { catalogs: prefix }), "main.xsd", NS.a, "import", NS.b, "http://b.example/real.xsd")).toMatchObject({ to: "x/real.xsd", how: "catalog" });
    const suffix = [{ path: "c.xml", entries: [{ kind: "suffix" as const, match: "/real.xsd", to: "x/real.xsd" }] }];
    expect(locate(inn(f2, { catalogs: suffix }), "main.xsd", NS.a, "import", NS.b, "https://far.example/v2/real.xsd")).toMatchObject({ to: "x/real.xsd", how: "catalog" });
  });

  test("two files of one name: the nearer is taken and the other kept", () => {
    const files = { "a/s/main.xsd": NS.a, "a/t/b.xsd": NS.b, "z/b.xsd": NS.b };
    expect(locate(inn(files), "a/s/main.xsd", NS.a, "import", NS.b, "gone/b.xsd")).toMatchObject({ to: "a/t/b.xsd", how: "name", others: ["z/b.xsd"] });
    expect(nearest(["z/q/x.xsd", "a/x.xsd", "a/s/x.xsd"], "a/s/m.xsd")).toEqual(["a/s/x.xsd", "a/x.xsd", "z/q/x.xsd"]);
  });

  test("by its namespace, for an import only", () => {
    const files = { "main.xsd": NS.a, "deep/other-name.xsd": NS.b };
    expect(locate(inn(files), "main.xsd", NS.a, "import", NS.b, "b.xsd")).toMatchObject({ to: "deep/other-name.xsd", how: "namespace" });
    expect(locate(inn(files), "main.xsd", NS.a, "import", NS.b, null)).toMatchObject({ to: "deep/other-name.xsd", how: "namespace" });
    expect(locate(inn({ "main.xsd": NS.a, "x.xsd": NS.a }), "main.xsd", NS.a, "include", null, "y.xsd")).toMatchObject({ to: null });
  });

  test("a name that fits with the wrong namespace is passed over and named", () => {
    const got = locate(inn({ "main.xsd": NS.a, "b.xsd": "urn:b-fd" }), "main.xsd", NS.a, "import", NS.b, "b.xsd");
    expect(got).toMatchObject({ to: null, why: "b.xsd is there, but it declares urn:b-fd." });
  });

  test("an include takes a file of the includer's namespace or none (a chameleon)", () => {
    expect(locate(inn({ "main.xsd": NS.a, "inc.xsd": "" }), "main.xsd", NS.a, "include", null, "inc.xsd")).toMatchObject({ how: "beside" });
    expect(locate(inn({ "main.xsd": NS.a, "inc.xsd": NS.b }), "main.xsd", NS.a, "include", null, "inc.xsd")).toMatchObject({ to: null });
  });

  test("an address is never joined as a path: found by its last part, else offered", () => {
    expect(joinPath("main.xsd", "https://example.com/b.xsd")).toBeNull();
    const files = { "main.xsd": NS.a, "vendor/b.xsd": NS.b };
    expect(locate(inn(files), "main.xsd", NS.a, "import", NS.b, "https://example.com/b.xsd")).toMatchObject({ to: "vendor/b.xsd", how: "name" });
    const nowhere = locate(inn({ "main.xsd": NS.a }), "main.xsd", NS.a, "import", NS.b, "https://example.com/b.xsd");
    expect(nowhere).toMatchObject({ to: null, address: "https://example.com/b.xsd", why: "It is an address. knobas fetches none by itself." });
  });

  test("what the person pointed to, or downloaded, stands first", () => {
    const files = { "main.xsd": NS.a, "b.xsd": NS.b };
    const pointed = [{ namespace: NS.b, location: "b.xsd", to: { repo: "r2", path: "lib/b.xsd" } }];
    expect(locate(inn(files, { pointed }), "main.xsd", NS.a, "import", NS.b, "b.xsd")).toMatchObject({ to: "@repo/r2/lib/b.xsd", how: "pointed" });
    const dl = [{ namespace: NS.b, location: "https://x/b.xsd", to: { url: "https://x/b.xsd", sha256: "abc", at: "2026-11-23" } }];
    expect(locate(inn(files, { pointed: dl }), "main.xsd", NS.a, "import", NS.b, "https://x/b.xsd")).toMatchObject({ to: "@download/abc", how: "downloaded" });
  });

  test("another repository answers where exactly one declares the namespace, after this one; two are asked", () => {
    const one = (ns: string) => (ns === NS.b ? [{ repo: "r9", name: "tidewater/catalog-tools", branch: "main", path: "xsd/b.xsd" }] : []);
    expect(locate(inn({ "main.xsd": NS.a }, { elsewhere: () => null }), "main.xsd", NS.a, "import", NS.b, "b.xsd")).toEqual({ need: [`?elsewhere:${NS.b}`] });
    expect(locate(inn({ "main.xsd": NS.a }, { elsewhere: one }), "main.xsd", NS.a, "import", NS.b, "b.xsd")).toMatchObject({ to: "@repo/r9/xsd/b.xsd", how: "repo" });
    // The repository's own file wins over another repository's.
    expect(locate(inn({ "main.xsd": NS.a, "q/b.xsd": NS.b }, { elsewhere: one }), "main.xsd", NS.a, "import", NS.b, "b.xsd")).toMatchObject({ to: "q/b.xsd" });
    const two = () => [
      { repo: "r9", name: "t/one", branch: "main", path: "b.xsd" },
      { repo: "r8", name: "t/two", branch: "main", path: "b.xsd" },
    ];
    expect(locate(inn({ "main.xsd": NS.a }, { elsewhere: two }), "main.xsd", NS.a, "import", NS.b, "b.xsd")).toMatchObject({ to: null, why: "Two repositories declare it." });
  });

  test("a schema that comes with knobas is last, and XML Schema's own is built in", () => {
    const bundled = new Map([[NS.b, "@knobas/b.xsd"]]);
    expect(locate(inn({ "main.xsd": NS.a }, { bundled }), "main.xsd", NS.a, "import", NS.b, "b.xsd")).toMatchObject({ to: "@knobas/b.xsd", how: "bundled" });
    expect(locate(inn({ "main.xsd": NS.a, "b.xsd": NS.b }, { bundled }), "main.xsd", NS.a, "import", NS.b, "b.xsd")).toMatchObject({ to: "b.xsd", how: "beside" });
    expect(locate(inn({ "main.xsd": NS.a }), "main.xsd", NS.a, "import", "http://www.w3.org/2001/XMLSchema", null)).toMatchObject({ how: "builtin" });
  });
});
