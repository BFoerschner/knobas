import { afterEach, describe, expect, it } from "vitest";

import { buildsLooked, forgetLooking, itemLooked, startLooking } from "./looking.svelte";
import type { LookingAgain } from "../../ipc/sources";

/** A bridge whose `looking:again` the test tells by hand. */
function bridge() {
  const handlers: ((event: { payload: unknown }) => void)[] = [];
  return {
    listen: (event: string, handler: (event: { payload: unknown }) => void) => {
      if (event === "looking:again") handlers.push(handler);
      return Promise.resolve(() => void handlers.splice(handlers.indexOf(handler), 1));
    },
    tell: (payload: LookingAgain) => handlers.forEach((handler) => handler({ payload })),
  };
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

afterEach(() => forgetLooking());

describe("what knobas looks at again after a branch moved (#2585)", () => {
  it("starts from what the backend looks at now, so a page opened during a schedule says so", async () => {
    const { listen } = bridge();
    startLooking(listen, () => Promise.resolve([{ scope: "forge:repo:tide/pay", code: true, builds: true, items: [] }]));
    await settle();
    expect(buildsLooked("forge:repo:tide/pay")).toBe(true);
    expect(buildsLooked("forge:repo:tide/other")).toBe(false);
  });

  it("follows each start, stopped side and end the event tells", async () => {
    const { listen, tell } = bridge();
    startLooking(listen, () => Promise.resolve([]));
    await settle();
    expect(buildsLooked("forge:repo:tide/pay")).toBe(false);
    tell({ scope: "forge:repo:tide/pay", code: true, builds: true, items: [] });
    expect(buildsLooked("forge:repo:tide/pay")).toBe(true);
    tell({ scope: "forge:repo:tide/pay", code: true, builds: false, items: [] });
    expect(buildsLooked("forge:repo:tide/pay")).toBe(false);
    tell({ scope: "forge:repo:tide/pay", code: false, builds: false, items: [] });
    expect(buildsLooked("forge:repo:tide/pay")).toBe(false);
  });

  it("an event heard before the first read answers is not undone by that read", async () => {
    const { listen, tell } = bridge();
    let answer: (rows: LookingAgain[]) => void = () => {};
    startLooking(listen, () => new Promise((resolve) => (answer = resolve)));
    tell({ scope: "forge:repo:tide/pay", code: false, builds: false, items: [] });
    answer([{ scope: "forge:repo:tide/pay", code: true, builds: true, items: [] }]);
    await settle();
    expect(buildsLooked("forge:repo:tide/pay"), "the schedule ended after the read was asked").toBe(false);
  });
});

describe("an item looked at again after a write (#2595, #2615)", () => {
  it("is looked at while a schedule reads it by id, in its own scope or another's, and not once it moved or ended", async () => {
    const { listen, tell } = bridge();
    startLooking(listen, () => Promise.resolve([{ scope: "jira:PAY-1", code: false, builds: false, items: ["jira:PAY-1", "jira:PAY-2"] }]));
    await settle();
    expect(itemLooked("jira:PAY-1")).toBe(true);
    expect(itemLooked("jira:PAY-2"), "a linked ticket, in the written item's scope").toBe(true);
    expect(itemLooked("jira:PAY-3")).toBe(false);
    // A branch write's pull request is read in its repository's scope.
    tell({ scope: "forge:repo:tide/pay", code: true, builds: true, items: ["forge:pr:tide/pay#4"] });
    expect(itemLooked("forge:pr:tide/pay#4")).toBe(true);
    expect(itemLooked("forge:repo:tide/pay"), "a scope is not an item read by id").toBe(false);
    // PAY-1 moved: its side stopped, PAY-2 is still read.
    tell({ scope: "jira:PAY-1", code: false, builds: false, items: ["jira:PAY-2"] });
    expect(itemLooked("jira:PAY-1")).toBe(false);
    expect(itemLooked("jira:PAY-2")).toBe(true);
    // The schedule ended.
    tell({ scope: "jira:PAY-1", code: false, builds: false, items: [] });
    expect(itemLooked("jira:PAY-2")).toBe(false);
    tell({ scope: "forge:repo:tide/pay", code: false, builds: false, items: [] });
    expect(itemLooked("forge:pr:tide/pay#4")).toBe(false);
  });
});

describe("the window's listening (#2615)", () => {
  it("stops when the app's window goes, and a start after that listens again", async () => {
    const { listen, tell } = bridge();
    const stop = startLooking(listen, () => Promise.resolve([]));
    await settle();
    expect(startLooking(listen, () => Promise.resolve([])), "a second start is the first one's").toBeTypeOf("function");
    stop();
    tell({ scope: "jira:PAY-1", code: false, builds: false, items: ["jira:PAY-1"] });
    expect(itemLooked("jira:PAY-1"), "stopped: nothing heard").toBe(false);
    startLooking(listen, () => Promise.resolve([]));
    await settle();
    tell({ scope: "jira:PAY-1", code: false, builds: false, items: ["jira:PAY-1"] });
    expect(itemLooked("jira:PAY-1")).toBe(true);
  });

  it("a stop before the listening was set up still stops it", async () => {
    const { listen, tell } = bridge();
    startLooking(listen, () => Promise.resolve([]))();
    await settle();
    tell({ scope: "jira:PAY-1", code: false, builds: false, items: ["jira:PAY-1"] });
    expect(itemLooked("jira:PAY-1")).toBe(false);
  });
});
