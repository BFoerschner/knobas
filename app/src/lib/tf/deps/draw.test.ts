/**
 * Dependencies' folding, filters, *Find* and arrow keys (#872, R-11.15,
 * R-11.17, R-11.18), over a large generated estate: the rules are only worth
 * anything at two thousand items, where a band that showed everything would
 * not fit on a screen.
 */
import { describe, expect, test } from "vitest";

import type { Dependencies, DepItem, DepRelation } from "../../ipc/assets";
import { drawOf, FIND_SHOWS, graphOf, hitsLine, moreLine, nextTile, NO_GESTURES, tileEl, touches, type Gestures } from "./draw";
import { settingsOf } from "./views";

function item(over: Partial<DepItem> & Pick<DepItem, "id" | "kind">): DepItem {
  return { name: over.id, health: "ok", state: null, monitors: 1, environment: null, owner: null, ...over };
}

const KINDS = [
  { id: "service", label: "Service", monogram: "SV" },
  { id: "vm", label: "VM", monogram: "VM" },
  { id: "ticket", label: "Ticket", monogram: "TK" },
];
const LAYERS = [
  { id: "work", name: "Work", kinds: ["ticket"] },
  { id: "services", name: "Services", kinds: ["service"] },
  { id: "machines", name: "Machines", kinds: ["vm"] },
];

/**
 * 2,000 services over 40 owners (50 each) on 200 VMs, ten of them broken in
 * owner `team-3` and two warning in `team-7`; every service inside its VM.
 */
function estate(): Dependencies {
  const items: DepItem[] = [];
  const relations: DepRelation[] = [];
  for (let v = 0; v < 200; v++) items.push(item({ id: `vm${v}`, kind: "vm", environment: v % 2 ? "prod" : "stage" }));
  for (let s = 0; s < 2000; s++) {
    const owner = `team-${s % 40}`;
    const broken = owner === "team-3" && s < 400;
    const warning = owner === "team-7" && s < 80;
    items.push(
      item({
        id: `s${s}`,
        kind: "service",
        name: `svc-${String(s).padStart(4, "0")}`,
        owner,
        environment: s % 2 ? "prod" : "stage",
        health: broken ? "err" : warning ? "warn" : "ok",
      }),
    );
    relations.push({ from: `s${s}`, to: `vm${s % 200}`, relation: "inside" });
  }
  return { layers: LAYERS, kinds: KINDS, items, relations };
}

const BIG = estate();
const byOwner = settingsOf({ services: { group: "owner" } });
const gestures = (over: Partial<Gestures>): Gestures => ({ ...NO_GESTURES, ...over });

describe("folding a large estate (R-11.17)", () => {
  const drawn = drawOf(BIG, byOwner, NO_GESTURES);
  const services = drawn.bands.find((band) => band.id === "services")!;

  test("counts and summaries use thousands separators", () => {
    expect(services.summary).toBe("2,000 services");
    expect(services.wrong).toBe("10 broken, 2 warning");
    expect(drawn.rail.find((row) => row.id === "services")).toMatchObject({ count: 2000, problems: 12 });
  });

  test("problems never fold: every broken and warning service is a tile of its own", () => {
    const problems = BIG.items.filter((i) => i.health === "err" || i.health === "warn");
    expect(problems).toHaveLength(12);
    for (const problem of problems) expect(drawn.where.get(problem.id)).toBe(tileEl(problem.id));
  });

  test("the groups holding problems open first; the healthy ones fold into chips", () => {
    expect(services.open.map((group) => group.name)).toEqual(["team-3", "team-7"]);
    expect(services.shut).toHaveLength(38);
    expect(services.shut[0]).toMatchObject({ name: expect.stringMatching(/^team-/), count: 50, wrong: "" });
    expect(services.shutLabel).toBe("Folded, all fine");
  });

  test("an open group shows its problems and a few more, then the rest as one line", () => {
    const team3 = services.open[0]!;
    // Compact (more than 30 in the layer) and big (more than 24): a cap of six.
    expect(services.compact).toBe(true);
    expect(team3.tiles).toHaveLength(10);
    expect(team3.tiles.every((tile) => tile.health === "err")).toBe(true);
    expect(team3.more).toMatchObject({ count: 40, wrong: "" });
    expect(moreLine(team3.more!.count, team3.more!.wrong)).toBe("40 more, all fine");
    const team7 = services.open[1]!;
    expect(team7.tiles.map((tile) => tile.health)).toEqual(["warn", "warn", "ok", "ok", "ok", "ok"]);
    expect(team7.more!.count).toBe(44);
  });

  test("every item drawn has a place: its tile, its group's more, its chip", () => {
    for (const service of BIG.items.filter((i) => i.kind === "service")) expect(drawn.where.has(service.id)).toBe(true);
    const chip = services.shut[0]!;
    const member = BIG.items.find((i) => i.owner === chip.name)!;
    expect(drawn.where.get(member.id)).toBe(chip.el);
  });

  test("below the fold count every group stays open", () => {
    const roomy = drawOf(BIG, { ...byOwner, fold_at: 5000 }, NO_GESTURES).bands.find((b) => b.id === "services")!;
    expect(roomy.shut).toHaveLength(0);
    expect(roomy.open).toHaveLength(40);
  });

  test("a layer folds to its summary, and its el stands for every item in it", () => {
    const folded = drawOf(BIG, settingsOf({ machines: { folded: true } }), NO_GESTURES);
    const machines = folded.bands.find((band) => band.id === "machines")!;
    expect(machines.folded).toBe(true);
    expect(machines.open).toEqual([]);
    expect(folded.where.get("vm7")).toBe(machines.el);
    expect(drawOf(BIG, settingsOf({ machines: { folded: true } }), gestures({ fold: new Map([["machines", false]]) })).bands.find((b) => b.id === "machines")!.folded).toBe(false);
  });

  test("a chip opened by hand opens, and an open group shut by hand folds", () => {
    const key = services.shut[0]!.key;
    const opened = drawOf(BIG, byOwner, gestures({ open: new Set([key]) })).bands.find((b) => b.id === "services")!;
    expect(opened.open.map((group) => group.key)).toContain(key);
    const shut = drawOf(BIG, byOwner, gestures({ shut: new Set(["services|team-3"]) })).bands.find((b) => b.id === "services")!;
    expect(shut.open.map((group) => group.name)).toEqual(["team-7"]);
    expect(shut.shut.find((group) => group.name === "team-3")!.wrong).toBe("10 broken");
  });
});

describe("a new tile is drawn where it can flash (#876, R-11.20)", () => {
  test("opens its folded layer, its chip and its 'more', and draws no other tile of the group", () => {
    const settings = settingsOf({ services: { group: "owner" }, machines: { folded: true } });
    // s1012 is team-12's, past the cap of its group: in a chip without the reveal.
    expect(drawOf(BIG, settings, NO_GESTURES).where.get("s1012")).not.toBe(tileEl("s1012"));
    const shown = drawOf(BIG, settings, gestures({ reveal: "s1012" }));
    expect(shown.where.get("s1012")).toBe(tileEl("s1012"));
    const team = shown.bands.find((band) => band.id === "services")!.open.find((group) => group.name === "team-12")!;
    expect(team.tiles.length).toBeLessThan(team.count);
    expect(shown.hits).toBe(0);

    const vm = drawOf(BIG, settings, gestures({ reveal: "vm7" }));
    expect(vm.bands.find((band) => band.id === "machines")!.folded).toBe(false);
    expect(vm.where.get("vm7")).toBe(tileEl("vm7"));
  });
});

describe("Find (R-11.18)", () => {
  test("opens the folds that hold a match and counts the matches", () => {
    const settings = settingsOf({ services: { group: "owner" }, machines: { folded: true } });
    // svc-0012 is team-12's: a healthy group, folded into a chip without Find.
    const found = drawOf(BIG, settings, gestures({ query: "svc-0012" }));
    const services = found.bands.find((band) => band.id === "services")!;
    expect(services.open.map((group) => group.name)).toContain("team-12");
    expect(found.where.get("s12")).toBe(tileEl("s12"));
    expect(found.hits).toBe(1);
    expect(hitsLine("svc-0012", found.hits)).toBe("1 found");

    const vm = drawOf(BIG, settings, gestures({ query: "vm7" }));
    expect(vm.bands.find((band) => band.id === "machines")!.folded).toBe(false);
    expect(hitsLine("vm7", vm.hits)).toBe(`${vm.hits} found, Enter for the next`);
    expect(hitsLine("nothing-like-it", 0)).toBe("none found");
    expect(hitsLine("  ", 0)).toBe("");
  });

  test("a group with one match draws the match and its usual few, not every member (#2212)", () => {
    const team12 = (drawn: ReturnType<typeof drawOf>) =>
      drawn.bands.find((band) => band.id === "services")!.open.find((group) => group.name === "team-12")!;
    const found = drawOf(BIG, byOwner, gestures({ query: "svc-0012" }));
    const group = team12(found);
    // Compact and big: a cap of six, the match first.
    expect(group.tiles).toHaveLength(6);
    expect(group.tiles[0]!.id).toBe("s12");
    expect(group.more).toMatchObject({ count: 44, wrong: "" });
    const member = group.more && BIG.items.find((i) => i.owner === "team-12" && !group.tiles.includes(i))!;
    expect(found.where.get(member!.id)).toBe(group.more!.el);

    // The group's own "more" still draws the rest.
    expect(team12(drawOf(BIG, byOwner, gestures({ query: "svc-0012", more: new Set(["services|team-12"]) }))).tiles).toHaveLength(50);
  });

  test("a group's first matches are tiles; the rest are counted on its more (#2212)", () => {
    // `team-12` matches each of the group's 50 by its owner.
    const found = drawOf(BIG, byOwner, gestures({ query: "team-12" }));
    expect(found.hits).toBe(50);
    const group = found.bands.find((band) => band.id === "services")!.open.find((g) => g.name === "team-12")!;
    expect(group.tiles).toHaveLength(FIND_SHOWS);
    expect(group.more).toMatchObject({ count: 50 - FIND_SHOWS, hits: 50 - FIND_SHOWS });
    expect(moreLine(group.more!.count, group.more!.wrong, group.more!.hits)).toBe("26 more, 26 found");
    expect(moreLine(40, "2 broken", 3)).toBe("40 more, 3 found, 2 broken");
    expect(moreLine(40, "", 0)).toBe("40 more, all fine");
    for (const service of BIG.items.filter((i) => i.owner === "team-12"))
      expect([tileEl(service.id), group.more!.el]).toContain(found.where.get(service.id));
  });

  test("an empty Find draws what it drew before (#2212)", () => {
    for (const blank of ["", "  "]) expect(drawOf(BIG, byOwner, gestures({ query: blank })).bands).toEqual(drawOf(BIG, byOwner, NO_GESTURES).bands);
  });
});

describe("what a tile touches, and the filters (R-11.15)", () => {
  const small: Dependencies = {
    layers: LAYERS,
    kinds: KINDS,
    items: [
      item({ id: "t", kind: "ticket" }),
      item({ id: "a", kind: "service", environment: "prod" }),
      item({ id: "b", kind: "service", environment: "stage", health: "err" }),
      item({ id: "c", kind: "service", environment: "prod" }),
      item({ id: "vm", kind: "vm" }),
    ],
    relations: [
      { from: "t", to: "a", relation: "related" },
      { from: "a", to: "vm", relation: "inside" },
      { from: "c", to: "vm", relation: "inside" },
      { from: "b", to: "c", relation: "depends-on" },
    ],
  };
  const ids = (drawn: ReturnType<typeof drawOf>) =>
    drawn.bands.flatMap((band) => band.open.flatMap((group) => group.tiles.map((tile) => tile.id))).sort();

  test("touching walks each direction, not the whole connected graph", () => {
    // a reaches vm and is reached by t; c shares the vm but is neither.
    expect([...touches(graphOf(small), "a")].sort()).toEqual(["a", "t", "vm"]);
  });

  test("a route is walked toward the asset that exposes it, never the other way (#1456)", () => {
    const read: Dependencies = {
      layers: LAYERS,
      kinds: KINDS,
      items: ["site", "vm", "c1", "c2", "r1", "r2", "r3", "tk"].map((id) => item({ id, kind: id === "tk" ? "ticket" : "service" })),
      relations: [
        { from: "vm", to: "site", relation: "inside" },
        { from: "c1", to: "vm", relation: "inside" },
        { from: "c2", to: "vm", relation: "inside" },
        // The read emits `exposes` as exposing asset -> route.
        { from: "vm", to: "r1", relation: "exposes" },
        { from: "vm", to: "r2", relation: "exposes" },
        { from: "vm", to: "r3", relation: "exposes" },
        { from: "r1", to: "c1", relation: "lands-on" },
        { from: "r2", to: "c2", relation: "lands-on" },
        { from: "tk", to: "c1", relation: "deployed-to" },
      ],
    };
    const of = (id: string) => [...touches(graphOf(read), id)].sort();
    expect(of("c1")).toEqual(["c1", "r1", "site", "tk", "vm"]);
    expect(of("r1")).toEqual(["c1", "r1", "site", "vm"]);
    expect(of("r3")).toEqual(["r3", "site", "vm"]);
  });

  test("Show only what this touches", () => {
    expect(ids(drawOf(small, settingsOf({}), gestures({ trace: "a" })))).toEqual(["a", "t", "vm"]);
  });

  test("Problems and what they touch", () => {
    const settings = { ...settingsOf({}), problems: true };
    expect(ids(drawOf(small, settings, NO_GESTURES))).toEqual(["b", "c", "vm"]);
    expect(drawOf(small, settings, NO_GESTURES).problems).toBe(1);
  });

  test("the environment chips keep what has no environment", () => {
    const settings = { ...settingsOf({}), environments: ["prod"] };
    expect(ids(drawOf(small, settings, NO_GESTURES))).toEqual(["a", "c", "t", "vm"]);
  });
});

describe("the arrow keys", () => {
  const from = { left: 100, top: 100 };
  const others = [
    { left: 300, top: 100 },
    { left: 200, top: 101 },
    { left: 20, top: 100 },
    { left: 110, top: 160 },
    { left: 400, top: 150 },
    { left: 90, top: 40 },
  ];
  test("move along the row, or to the nearest row above or below", () => {
    expect(nextTile(from, others, "ArrowRight")).toBe(1);
    expect(nextTile(from, others, "ArrowLeft")).toBe(2);
    expect(nextTile(from, others, "ArrowDown")).toBe(3);
    expect(nextTile(from, others, "ArrowUp")).toBe(5);
    expect(nextTile(from, [], "ArrowUp")).toBe(-1);
  });
});
