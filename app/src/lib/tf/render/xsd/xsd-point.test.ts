import { expect, test } from "vitest";

import { candidatesElsewhere, elsewhereOf, withPointed } from "./xsd-point";

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
