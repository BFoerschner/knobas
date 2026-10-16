import { describe, expect, test } from "vitest";

import type { Dependencies, DepItem } from "../../ipc/assets";
import { bandsOf, railOf, stateOf, unplacedOf } from "./deps";

function item(over: Partial<DepItem> & Pick<DepItem, "id" | "kind">): DepItem {
  return {
    name: over.id,
    health: null,
    state: null,
    monitors: 0,
    environment: null,
    owner: null,
    ...over,
  };
}

const KINDS = [
  { id: "ticket", label: "Ticket", monogram: "TK" },
  { id: "pr", label: "Pull request", monogram: "PR" },
  { id: "pipeline", label: "Pipeline", monogram: "PL" },
  { id: "service", label: "Service", monogram: "SV" },
  { id: "reverse_proxy", label: "Reverse proxy", monogram: "RP" },
  { id: "vm", label: "VM", monogram: "VM" },
  { id: "network", label: "Network", monogram: "NW" },
];

function read(items: DepItem[]): Dependencies {
  return {
    layers: [
      { id: "work", name: "Work", kinds: ["ticket", "pr"] },
      { id: "delivery", name: "Delivery", kinds: ["pipeline"] },
      { id: "services", name: "Services", kinds: ["service", "reverse_proxy"] },
      { id: "machines", name: "Machines", kinds: ["vm"] },
    ],
    kinds: KINDS,
    items,
    relations: [],
  };
}

describe("a tile's one state word (R-11.12)", () => {
  test("an asset says down, warning, its checks, or that nothing checks it", () => {
    expect(stateOf(item({ id: "a", kind: "vm", health: "err", monitors: 1 }))).toEqual({ tone: "err", word: "down" });
    expect(stateOf(item({ id: "a", kind: "vm", health: "warn", monitors: 1 }))).toEqual({ tone: "warn", word: "warning" });
    expect(stateOf(item({ id: "a", kind: "vm", health: "ok", monitors: 1 }))).toEqual({ tone: "ok", word: "1 check" });
    expect(stateOf(item({ id: "a", kind: "vm", health: "ok", monitors: 3 }))).toEqual({ tone: "ok", word: "3 checks" });
    expect(stateOf(item({ id: "a", kind: "vm", monitors: 0 }))).toEqual({ tone: "none", word: "no check" });
  });

  test("a pipeline says failed, running or passed by its newest run", () => {
    expect(stateOf(item({ id: "p", kind: "pipeline", health: "err" })).word).toBe("failed");
    expect(stateOf(item({ id: "p", kind: "pipeline", health: "info" }))).toEqual({ tone: "info", word: "running" });
    expect(stateOf(item({ id: "p", kind: "pipeline", health: "ok" }))).toEqual({ tone: "ok", word: "passed" });
    expect(stateOf(item({ id: "p", kind: "pipeline" }))).toEqual({ tone: "plain", word: "" });
  });

  test("an open pull request is outlined blue and a ticket says its status", () => {
    expect(stateOf(item({ id: "r", kind: "pr", health: "info", state: "open" }))).toEqual({ tone: "info", word: "open" });
    expect(stateOf(item({ id: "t", kind: "ticket", state: "In Progress" }))).toEqual({ tone: "plain", word: "In Progress" });
  });
});

describe("the bands (R-11.11)", () => {
  test("each layer says what it holds, what is wrong and draws problems first", () => {
    const bands = bandsOf(
      read([
        item({ id: "s1", kind: "service", name: "b-svc", health: "ok", monitors: 1 }),
        item({ id: "s2", kind: "service", name: "a-svc", health: "warn", monitors: 1 }),
        item({ id: "s3", kind: "service", name: "c-svc", health: "err", monitors: 1 }),
        item({ id: "rp", kind: "reverse_proxy", name: "edge", monitors: 0 }),
        item({ id: "s4", kind: "service", name: "d-svc", health: "err", monitors: 1 }),
      ]),
      new Set(),
    );
    const services = bands.find((band) => band.id === "services")!;
    expect(services.summary).toBe("4 services, 1 reverse proxy");
    expect(services.wrong).toBe("2 broken, 1 warning");
    expect(services.items.map((tile) => tile.name)).toEqual(["c-svc", "d-svc", "a-svc", "edge", "b-svc"]);
    expect(services.mix).toEqual([
      { tone: "err", count: 2 },
      { tone: "warn", count: 1 },
      { tone: "none", count: 1 },
      { tone: "ok", count: 1 },
    ]);
  });

  test("a layer with nothing wrong is all fine, and an empty one says so", () => {
    const bands = bandsOf(read([item({ id: "vm", kind: "vm", health: "ok", monitors: 1 })]), new Set());
    const machines = bands.find((band) => band.id === "machines")!;
    expect(machines.summary).toBe("1 VM");
    expect(machines.wrong).toBe("");
    expect(bands.find((band) => band.id === "delivery")!.items).toEqual([]);
  });

  test("above 30 items a layer's tiles shrink to one line", () => {
    const many = Array.from({ length: 31 }, (_, n) => item({ id: `s${n}`, kind: "service" }));
    expect(bandsOf(read(many), new Set()).find((band) => band.id === "services")!.compact).toBe(true);
    expect(bandsOf(read(many.slice(1)), new Set()).find((band) => band.id === "services")!.compact).toBe(false);
  });

  test("a hidden layer draws no band", () => {
    expect(bandsOf(read([]), new Set(["work"])).map((band) => band.id)).toEqual(["delivery", "services", "machines"]);
  });
});

describe("the rail (R-11.14)", () => {
  test("every layer with its count and its problems, hidden ones too", () => {
    const rail = railOf(
      read([
        item({ id: "s1", kind: "service", health: "err" }),
        item({ id: "s2", kind: "service" }),
        item({ id: "t", kind: "ticket" }),
      ]),
      new Set(["work"]),
    );
    expect(rail).toEqual([
      { id: "work", name: "Work", count: 1, problems: 0, shown: false },
      { id: "delivery", name: "Delivery", count: 0, problems: 0, shown: true },
      { id: "services", name: "Services", count: 2, problems: 1, shown: true },
      { id: "machines", name: "Machines", count: 0, problems: 0, shown: true },
    ]);
  });
});

describe("items in no layer (R-11.13, story 147)", () => {
  test("are counted and named by kind, never dropped", () => {
    const unplaced = unplacedOf(
      read([
        item({ id: "n1", kind: "network" }),
        item({ id: "n2", kind: "network" }),
        item({ id: "x", kind: "something_new" }),
        item({ id: "vm", kind: "vm" }),
      ]),
    );
    expect(unplaced).toEqual({ count: 3, kinds: ["Network", "something_new"] });
  });
});
