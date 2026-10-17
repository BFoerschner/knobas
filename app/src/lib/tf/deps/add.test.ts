/**
 * *Add ▾* and the route dialog's rules (#876, R-11.20, R-11.21), in
 * prototype-6's words.
 */
import { describe, expect, test } from "vitest";

import type { AssetType, Dependencies, DepItem, RouteRow } from "../../ipc/assets";
import { hasScheme, routeEditsOf, routeNameOf, typeChoices, usualLine } from "./add";

const type = (id: string, label: string, suggests: string[] = []): AssetType => ({ id, label, monogram: "??", properties: [], suggests });
const TYPES = [
  type("site", "Site", ["hypervisor"]),
  type("vm", "VM", ["container_engine", "service", "database_server", "reverse_proxy"]),
  type("container_engine", "Container engine", ["container"]),
  type("service", "Service"),
  type("database_server", "Database server", ["database"]),
  type("reverse_proxy", "Reverse proxy"),
  type("custom", "Custom"),
];

function item(id: string, kind: string, name: string): DepItem {
  return { id, kind, name, health: null, state: null, monitors: 0, environment: null, owner: null };
}

const READ: Dependencies = {
  layers: [],
  kinds: [],
  items: [item("asset:vm", "vm", "vm-pay-01"), item("asset:svc", "service", "payouts-svc")],
  relations: [],
};

describe("typeChoices", () => {
  test("the layer's types come first, then every other type", () => {
    const { here, rest } = typeChoices(["service", "reverse_proxy", "route"], TYPES);
    expect(here.map((t) => t.label)).toEqual(["Service", "Reverse proxy"]);
    expect(rest.map((t) => t.label)).toEqual(["Site", "VM", "Container engine", "Database server", "Custom"]);
  });

  test("from the bar, with no layer, every type is in the rest", () => {
    expect(typeChoices([], TYPES).here).toEqual([]);
    expect(typeChoices([], TYPES).rest).toHaveLength(7);
  });
});

describe("usualLine", () => {
  test("names the parent's usual children", () => {
    expect(usualLine("asset:vm", READ, TYPES)).toBe(
      "Usual inside vm-pay-01: Container engine, Service, Database server, Reverse proxy. Any type is allowed.",
    );
  });

  test("a parent with no usual children says so", () => {
    expect(usualLine("asset:svc", READ, TYPES)).toBe("Nothing in particular usually goes inside payouts-svc. Any type is allowed.");
  });

  test("the top of the estate", () => {
    expect(usualLine(null, READ, TYPES)).toBe("At the top of the estate: usually a site or a hypervisor.");
  });
});

describe("the address", () => {
  test("must carry a scheme", () => {
    expect(hasScheme("https://payouts.tidewater.test")).toBe(true);
    expect(hasScheme("tcp://db:5432")).toBe(true);
    expect(hasScheme("payouts.tidewater.test")).toBe(false);
    expect(hasScheme("https://")).toBe(false);
  });

  test("names the route when the name is left empty", () => {
    expect(routeNameOf("https://payouts.tidewater.test/health")).toBe("payouts.tidewater.test");
    expect(routeNameOf("tcp://db:5432")).toBe("db:5432");
  });
});

describe("routeEditsOf", () => {
  const row: RouteRow = {
    id: "route:1",
    asset_id: "asset:proxy",
    asset_name: "traefik",
    target_id: "asset:svc",
    target_name: "payouts-svc",
    name: "payouts",
    url: "https://payouts.example",
    visibility: "internal",
    properties: [],
  };

  test("an unchanged form sends nothing", () => {
    expect(routeEditsOf(row, { url: "https://payouts.example", name: "payouts", target: "asset:svc", visibility: "internal" })).toEqual([]);
  });

  test("sends only what changed, and Nothing knobas knows clears the target", () => {
    expect(routeEditsOf(row, { url: "https://payouts.example/v2", name: "", target: null, visibility: "public" })).toEqual([
      { field: "url", value: "https://payouts.example/v2" },
      { field: "name", value: "payouts.example" },
      { field: "target", value: null },
      { field: "visibility", value: "public" },
    ]);
  });
});
