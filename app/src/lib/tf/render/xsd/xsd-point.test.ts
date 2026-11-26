import { expect, test } from "vitest";

import { candidatesElsewhere, downloadedSha, elsewhereOf, pointSorted, pointText, stillUsed, withDownload, withPointed, type Candidate } from "./xsd-point";

test("another repository's files: this repository's own are left out, so one other is not counted as two", () => {
  const list = [
    { repo: "gitea:tidewater/ledger-api", branch: "main", path: "schemas/a.xsd" },
    { repo: "gitea:tidewater/catalog-tools", branch: "develop", path: "lib/a.xsd" },
  ];
  expect(elsewhereOf("gitea:tidewater/ledger-api", list)).toEqual([{ repo: "gitea:tidewater/catalog-tools", name: "tidewater/catalog-tools", branch: "develop", path: "lib/a.xsd" }]);
});

test("a pick from another repository is kept with its name; the last pick for a namespace and location wins", () => {
  const [c] = candidatesElsewhere("urn:a", [{ repo: "gitea:acme/schemas", name: "acme/schemas", branch: "main", path: "a.xsd" }]);
  expect(c).toEqual({ repo: "gitea:acme/schemas", repoName: "acme/schemas", branch: "main", path: "a.xsd", declares: "urn:a" });
  const first = withPointed([], "urn:a", "a.xsd", c!);
  expect(first).toEqual([{ namespace: "urn:a", location: "a.xsd", to: { repo: "gitea:acme/schemas", path: "a.xsd", name: "acme/schemas" } }]);
  const mine = { repo: "r", repoName: null, path: "x/a.xsd", declares: "urn:a" };
  expect(withPointed(first, "urn:a", "a.xsd", mine)).toEqual([{ namespace: "urn:a", location: "a.xsd", to: { repo: "r", path: "x/a.xsd" } }]);
});

test("a download is kept in the place of the answer it replaces; its file stays while another entry uses it", () => {
  const sha = (c: string) => c.repeat(64);
  const list = [
    { namespace: "urn:a", location: "a.xsd", to: { repo: "r", path: "a.xsd" } },
    { namespace: "urn:b", location: "https://schemas.example/b.xsd", to: { url: "https://schemas.example/b.xsd", sha256: sha("1"), at: "2026-11-01" } },
    { namespace: "urn:c", location: "c.xsd", to: { url: "https://schemas.example/b.xsd", sha256: sha("1"), at: "2026-11-01" } },
  ];
  const again = withDownload(list, "urn:b", "https://schemas.example/b.xsd", { url: "https://schemas.example/b.xsd", sha256: sha("2"), size: 9, at: "2026-11-24" });
  expect(again.map((p) => p.namespace)).toEqual(["urn:a", "urn:b", "urn:c"]);
  expect(again[1]!.to).toEqual({ url: "https://schemas.example/b.xsd", sha256: sha("2"), at: "2026-11-24" });
  // The old file still answers urn:c; once none uses it, it can go.
  expect(stillUsed(again, sha("1"), -1)).toBe(true);
  expect(stillUsed(again, sha("1"), 2)).toBe(false);
  expect(downloadedSha(`@download/${sha("a")}`)).toBe(sha("a"));
  expect(downloadedSha("@download/../x")).toBeNull();
  expect(downloadedSha(`@repo/r/${sha("a")}`)).toBeNull();
});

test("the picker ranks a file by its repository's name and path, and sorts what fits first, this repository's before another's (#2535)", () => {
  const here = (path: string, declares = "urn:a"): Candidate => ({ repo: "r", repoName: null, path, declares });
  const there: Candidate = { repo: "o", repoName: "other", branch: "main", path: "a.xsd", declares: "urn:a" };
  expect(pointText(here("xsd/a.xsd"))).toBe(" xsd/a.xsd");
  expect(pointText(there)).toBe("other a.xsd");
  const fits = (c: Candidate) => c.declares === "urn:a";
  const ranked = [there, here("deep/long/a.xsd"), here("b.xsd", "urn:b"), here("a.xsd")];
  // Typed: what fits, then this repository's, each in the ranking's order.
  expect(pointSorted(ranked, fits, true).map((c) => `${c.repoName ?? ""}:${c.path}`)).toEqual([":deep/long/a.xsd", ":a.xsd", "other:a.xsd", ":b.xsd"]);
  // Nothing typed: the shorter path first.
  expect(pointSorted(ranked, fits, false).map((c) => `${c.repoName ?? ""}:${c.path}`)).toEqual([":a.xsd", ":deep/long/a.xsd", "other:a.xsd", ":b.xsd"]);
});
