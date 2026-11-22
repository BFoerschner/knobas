/** Two archives compared (2026-11-22): what differs by name, CRC and size, families and libraries folded, the words. */
import { expect, test } from "vitest";

import type { ArchiveIndex, Lib } from "../ipc/entity";
import { compareJars, compareWords, headLine } from "./jar-diff";
import { DUPLICATE } from "./jar";

/** An archive of `entries`, each `[name, size, crc]`, sorted as the backend sorts them. */
function jar(entries: [string, number, number][], over: Partial<ArchiveIndex> = {}): ArchiveIndex {
  const sorted = [...entries].sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0));
  return {
    handle: "h",
    kind: "jar",
    size: 1,
    total: 0,
    packed: 0,
    names: sorted.map(([n]) => n).join("\n"),
    sizes: sorted.map(([, s]) => s),
    packed_sizes: sorted.map(([, s]) => s),
    crcs: sorted.map(([, , c]) => c),
    flags: sorted.map(() => 0),
    manifest: [],
    maven: [],
    release: null,
    releases: [],
    libs: [],
    prefix: 0,
    warnings: [],
    ...over,
  };
}

const shape = (c: ReturnType<typeof compareJars>) => c.rows.map((r) => `${r.change} ${r.key}`);

test("an entry is the same only where its name, CRC and size all match", () => {
  const before = jar([["a.txt", 5, 1], ["b.txt", 5, 2], ["c.txt", 5, 3], ["gone.txt", 1, 9]]);
  const after = jar([["a.txt", 5, 1], ["b.txt", 5, 22], ["c.txt", 6, 3], ["new.txt", 1, 8]]);
  const c = compareJars(before, after);
  expect(shape(c)).toEqual(["changed b.txt", "changed c.txt", "removed gone.txt", "added new.txt"]);
  expect([c.same, c.changed, c.added, c.removed]).toEqual([1, 2, 1, 1]);
  expect(c.rows[0]).toMatchObject({ kind: "file", old: { size: 5 }, new: { size: 5 } });
});

test("a class and its nested classes are one row, changed where any member is", () => {
  const before = jar([["a/Foo.class", 100, 1], ["a/Foo$Bar.class", 10, 2], ["a/Foo$Baz.class", 10, 3], ["a/Same.class", 7, 4]]);
  const after = jar([["a/Foo.class", 100, 1], ["a/Foo$Bar.class", 10, 20], ["a/Foo$Baz.class", 10, 3], ["a/Same.class", 7, 4]]);
  const c = compareJars(before, after);
  expect(shape(c)).toEqual(["changed a/Foo.class"]);
  expect(c.rows[0]!.new).toEqual({ path: "a/Foo.class", size: 120, members: ["a/Foo.class", "a/Foo$Bar.class", "a/Foo$Baz.class"] });
  expect(c.same).toBe(1);
  // A nested class only on one side changes its family, not a row of its own.
  const grew = compareJars(jar([["a/Foo.class", 100, 1]]), jar([["a/Foo.class", 100, 1], ["a/Foo$1.class", 4, 5]]));
  expect(shape(grew)).toEqual(["changed a/Foo.class"]);
});

test("a library across a version is one changed row; two of one name in a folder are left apart", () => {
  const libs = (xs: [string, string][]): Lib[] => xs.map(([name, version]) => ({ entry: `BOOT-INF/lib/${name}-${version}.jar`, name, version, size: 1 }));
  const before = jar([["BOOT-INF/lib/spring-core-6.1.2.jar", 10, 1], ["BOOT-INF/lib/gson-2.10.jar", 10, 2], ["other/spring-core-6.1.2.jar", 10, 1]], {
    libs: libs([["spring-core", "6.1.2"], ["gson", "2.10"]]),
  });
  const after = jar([["BOOT-INF/lib/spring-core-6.1.3.jar", 11, 7], ["BOOT-INF/lib/gson-2.10.jar", 10, 2], ["other/spring-core-6.1.3.jar", 10, 1]], {
    libs: libs([["spring-core", "6.1.3"], ["gson", "2.10"]]),
  });
  const c = compareJars(before, after);
  expect(shape(c)).toEqual(["changed BOOT-INF/lib/spring-core-6.1.3.jar", "changed other/spring-core-6.1.3.jar"]);
  expect(c.rows[0]!.lib).toEqual({ name: "spring-core", from: "6.1.2", to: "6.1.3" });
  expect(c.rows[0]!.old?.path).toBe("BOOT-INF/lib/spring-core-6.1.2.jar");
  // The name from the file name where `libs` does not list it.
  expect(c.rows[1]!.lib).toEqual({ name: "spring-core", from: "6.1.2", to: "6.1.3" });

  const twice = compareJars(jar([["lib/a-1.jar", 1, 1], ["lib/a-2.jar", 1, 2]]), jar([["lib/a-3.jar", 1, 3]]));
  expect(shape(twice)).toEqual(["removed lib/a-1.jar", "removed lib/a-2.jar", "added lib/a-3.jar"]);
  // Another folder is no pair.
  const moved = compareJars(jar([["x/a-1.jar", 1, 1]]), jar([["y/a-2.jar", 1, 2]]));
  expect(shape(moved)).toEqual(["removed x/a-1.jar", "added y/a-2.jar"]);
});

test("a name listed twice is compared by its first entry", () => {
  const before = jar([["a.txt", 5, 1]]);
  const after = jar([["a.txt", 5, 1]]);
  after.names = "a.txt\na.txt";
  after.sizes = [5, 9];
  after.crcs = [1, 99];
  after.flags = [0, DUPLICATE];
  expect(compareJars(before, after).rows).toEqual([]);
});

test("a side with no archive lists every entry added or removed", () => {
  const one = jar([["a/Foo.class", 1, 1], ["a/Foo$X.class", 1, 2], ["b.txt", 1, 3]]);
  expect(shape(compareJars(null, one))).toEqual(["added a/Foo.class", "added b.txt"]);
  expect(shape(compareJars(one, null))).toEqual(["removed a/Foo.class", "removed b.txt"]);
  expect(compareWords(compareJars(null, one), "added")).toBe("2 entries, all added");
  expect(compareWords(compareJars(null, jar([["b.txt", 1, 3]])), "added")).toBe("1 entry, all added");
});

test("the foot names only the parts there are", () => {
  const c = { rows: [], same: 1198, changed: 3, added: 2, removed: 1 };
  expect(compareWords(c, null)).toBe("3 changed, 2 added, 1 removed; 1,198 the same");
  expect(compareWords({ ...c, added: 0, removed: 0 }, null)).toBe("3 changed; 1,198 the same");
  expect(compareWords({ ...c, changed: 0, added: 0, removed: 0 }, null)).toBe("1,198 the same");
  expect(compareWords({ ...c, same: 0 }, null)).toBe("3 changed, 2 added, 1 removed");
});

test("the head line puts an arrow only where a value differs", () => {
  const maven = (version: string) => [{ group: "t", artifact: "ledger-service", version, name: null, description: null, packaging: "jar", path: "META-INF/maven/t/ledger-service/" }];
  const a = jar([], { kind: "boot-jar", release: "Java 17", maven: maven("1.4.0") });
  const b = jar([], { kind: "boot-jar", release: "Java 21", maven: maven("1.5.0") });
  expect(headLine(a, b, "ledger-service.jar")).toBe("ledger-service 1.4.0 → 1.5.0 · Spring Boot JAR · Java 17 → 21");
  expect(headLine(a, { ...b, release: "Java 17" }, "ledger-service.jar")).toBe("ledger-service 1.4.0 → 1.5.0 · Spring Boot JAR · Java 17");
  expect(headLine(null, a, "ledger-service.jar")).toBe("ledger-service 1.4.0 · Spring Boot JAR · Java 17");
  expect(headLine(jar([]), jar([]), "plain.jar")).toBe("plain · JAR");
});
